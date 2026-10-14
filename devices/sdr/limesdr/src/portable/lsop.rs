//! The `LimeSDR` board code's low-level operations, libusdr's `ls_op` classes as
//! `device/u3_limesdr/limesdr_ctrl.c` uses them, and how the control channel carries each
//! as one LMS64C exchange.
//!
//! In libusdr the FT601 transport does that carrying (`lowlevel/usb_ft601/`), and it is not
//! in the pinned source. The mapping here is the oracle shim's
//! (`oracle/shim/sim_plugin.c`): an assumption, stated per operation, which the golden's
//! `req`/`rep` lines encode. `limesdr-oracle` proves the driver and the shim agree, not
//! that the board's firmware does.

use rsl_lowlevel::{FpgaAccess, SpiAddr};

use super::lms64c::{Frame, STATUS_COMPLETED, Space};

/// The SPI word's write flag (bit 31; bits 30:16 are the address, 15:0 the data).
const SPI_WRITE: u32 = 1 << 31;

/// One low-level operation of the board code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LsOp {
    /// Write a 16-bit gateware register (`lowlevel_reg_wr16`). Assumption: one
    /// `CMD_BRDSPI_WR` record, `reg << 16 | value`.
    Write {
        /// The register.
        reg: u16,
        /// The value written.
        value: u16,
    },

    /// Read a 16-bit gateware register (`lowlevel_reg_rd16`). Assumption: one
    /// `CMD_BRDSPI_RD` address.
    Read {
        /// The register.
        reg: u16,
    },

    /// Read a 32-bit value from two consecutive gateware registers (`lowlevel_reg_rd32`).
    /// Assumption: one `CMD_BRDSPI_RD` of `reg` and `reg + 1`, `reg` the low half.
    ReadWide {
        /// The register holding the low half.
        reg: u16,
    },

    /// Shift a 32-bit word through the LMS7002M's SPI (`lowlevel_spi_tr32`, bus 0).
    /// Assumption: a word with the write flag goes out unchanged as a `CMD_LMS7002_WR`
    /// record; any other reads the address in bits 30:16 with `CMD_LMS7002_RD`, and its
    /// 16-bit value comes back zero-extended.
    Spi {
        /// The word.
        word: u32,
    },

    /// Pulse the LMS7002M's reset (`USDR_LSOP_CUSTOM_CMD` 0, "LMS7 RESET"). Assumption:
    /// `CMD_LMS7002_RST` with `LMS_RST_PULSE`.
    Lms7Reset,
}

/// A reply that does not carry the operation's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadReply {
    /// The firmware did not complete the command: the reply's status, which is not
    /// [`STATUS_COMPLETED`]. libusdr's shim reports it as `-EIO`.
    Refused(u8),

    /// The reply answers another command: the one it names. libusdr's shim does not check
    /// this (a deliberate addition: a reply out of step with its request is not its result).
    Mismatched(u8),
}

impl LsOp {
    /// The request frame that carries this operation.
    pub(crate) fn request(self) -> Frame {
        match self {
            Self::Write { reg, value } => {
                Frame::write(Space::Board, &[u32::from(reg) << 16 | u32::from(value)])
            }
            Self::Read { reg } => Frame::read(Space::Board, &[reg]),
            Self::ReadWide { reg } => Frame::read(Space::Board, &[reg, reg.wrapping_add(1)]),
            Self::Spi { word } if word & SPI_WRITE != 0 => Frame::write(Space::Lms7, &[word]),
            Self::Spi { word } => {
                let [hi, lo, ..] = word.to_be_bytes();
                Frame::read(Space::Lms7, &[u16::from_be_bytes([hi, lo])])
            }
            Self::Lms7Reset => Some(Frame::lms7_reset_pulse()),
        }
        .expect("invariant: one or two records fit a frame")
    }

    /// What the operation returns, from the reply to its [`request`](Self::request): the
    /// register or word read, zero-extended, or 0 for a write or the reset.
    ///
    /// # Errors
    ///
    /// [`BadReply::Mismatched`] when the reply names another command than the request,
    /// [`BadReply::Refused`] when its status is not [`STATUS_COMPLETED`]. The rest of the
    /// header is not checked, as libusdr's shim checks none of it.
    pub(crate) fn reply(self, reply: &Frame) -> Result<u32, BadReply> {
        if reply.command() != self.request().command() {
            return Err(BadReply::Mismatched(reply.command()));
        }
        if reply.status() != STATUS_COMPLETED {
            return Err(BadReply::Refused(reply.status()));
        }
        let value = |index| {
            u32::from(
                reply
                    .read_value(index)
                    .expect("invariant: a payload holds records 0 and 1"),
            )
        };
        Ok(match self {
            Self::Read { .. } => value(0),
            Self::Spi { word } if word & SPI_WRITE == 0 => value(0),
            Self::ReadWide { .. } => value(0) | value(1) << 16,
            Self::Write { .. } | Self::Spi { .. } | Self::Lms7Reset => 0,
        })
    }

    /// The access this operation is, for a failure report: a gateware register, or the
    /// LMS7002M register its SPI word addresses (bits 30:16) on bus 0; `None` for the reset.
    pub(crate) const fn fpga_access(self) -> Option<FpgaAccess> {
        match self {
            Self::Write { reg, value } => Some(FpgaAccess::Write {
                reg: reg as u32,
                value: value as u32,
            }),
            Self::Read { reg } | Self::ReadWide { reg } => {
                Some(FpgaAccess::Read { reg: reg as u32 })
            }
            Self::Spi { word } => Some(FpgaAccess::Spi {
                target: SpiAddr(0),
                word,
                reg: word >> 16 & 0x7fff,
                write: word & SPI_WRITE != 0,
            }),
            Self::Lms7Reset => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lms64c::FRAME_LEN;
    use super::*;

    /// A completed reply to `command` carrying `records` from the payload's start.
    fn completed(command: u8, records: &[u32]) -> Frame {
        let mut bytes = [0; FRAME_LEN];
        bytes[0] = command;
        bytes[1] = STATUS_COMPLETED;
        for (slot, record) in bytes[8..].chunks_exact_mut(4).zip(records) {
            slot.copy_from_slice(&record.to_be_bytes());
        }
        Frame::from_bytes(bytes)
    }

    #[test]
    fn spi_words_choose_their_command_by_the_write_flag() {
        let write = LsOp::Spi { word: 0x8020_ffff }.request();
        assert_eq!(write.command(), 0x21);
        assert_eq!(write.to_bytes()[8..12], [0x80, 0x20, 0xff, 0xff]);
        let read = LsOp::Spi { word: 0x002f_0000 }.request();
        assert_eq!(read.command(), 0x22);
        assert_eq!(read.to_bytes()[8..10], [0x00, 0x2f]);
    }

    #[test]
    fn a_wide_read_takes_the_next_register_as_its_high_half() {
        let op = LsOp::ReadWide { reg: 0x0072 };
        assert_eq!(op.request().block_count(), 2);
        assert_eq!(
            op.reply(&completed(0x56, &[0x0072_6664, 0x0073_0066])),
            Ok(0x0066_6664)
        );
    }

    #[test]
    fn reads_are_zero_extended_and_writes_return_nothing() {
        let read = completed(0x22, &[0x002f_3841]);
        assert_eq!(LsOp::Spi { word: 0x002f_0000 }.reply(&read), Ok(0x3841));
        let write = completed(0x21, &[0x002f_3841]);
        assert_eq!(LsOp::Spi { word: 0x802f_0000 }.reply(&write), Ok(0));
        assert_eq!(LsOp::Lms7Reset.reply(&completed(0x20, &[])), Ok(0));
    }

    #[test]
    fn a_reply_to_another_command_is_not_this_operations_result() {
        let write_reply = completed(0x21, &[0x002f_3841]);
        assert_eq!(
            LsOp::Spi { word: 0x002f_0000 }.reply(&write_reply),
            Err(BadReply::Mismatched(0x21))
        );
    }

    #[test]
    fn any_status_but_completed_is_refused() {
        let mut bytes = completed(0x56, &[]).to_bytes();
        bytes[1] = 5;
        assert_eq!(
            LsOp::Read { reg: 0x000a }.reply(&Frame::from_bytes(bytes)),
            Err(BadReply::Refused(5))
        );
    }
}
