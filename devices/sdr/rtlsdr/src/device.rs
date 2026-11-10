//! [`Device`]: one open dongle.

use crate::access::{Access, I2cAddress};
use crate::control::ControlPort;
use crate::error::Error;
use crate::portable::pending::Gap;
use crate::portable::r820t2::Registers;
use crate::portable::sequence::{CLOSE, OPEN, Step};

/// The ledger row for every board's open but the V5's. prov: RTL-GAP-07
const OTHER_BOARD: &str = "RTL-GAP-07";

/// One RTL-SDR dongle behind a [`ControlPort`].
pub struct Device {
    /// The dongle's transport.
    port: Box<dyn ControlPort>,
}

impl Device {
    /// Brings the dongle up over `port` with the `NESDR SMArt v5`'s captured open ([`OPEN`]),
    /// the only board's open evidenced.
    ///
    /// # Errors
    ///
    /// [`Error::Gap`] (`RTL-GAP-07`) when the board is not one the open was captured on, as
    /// far as the open can tell: something answers at a probed address, or the tuner's R0
    /// is not the R820T2's chip id. [`Error::Bus`] on a transport failure, including a
    /// stall where the V5 answers (no tuner at its address, say). The dongle is left
    /// part-way either way.
    pub fn open(port: Box<dyn ControlPort>) -> Result<Self, Error> {
        let mut device = Self { port };
        device.run(OPEN)?;
        Ok(device)
    }

    /// Shuts the dongle down with the V5's captured close ([`CLOSE`]) and gives up the
    /// port. Dropping a `Device` without it sends nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] on a transport failure.
    pub fn close(mut self) -> Result<(), Error> {
        self.run(CLOSE)
    }

    /// Runs `steps`, in order, stopping at the first failure.
    fn run(&mut self, steps: &[Step]) -> Result<(), Error> {
        let mut access = Access::new(&mut *self.port);
        for step in steps {
            match *step {
                Step::Paged {
                    page,
                    register,
                    data,
                } => access.write_paged(page, register, data)?,
                Step::Block {
                    block,
                    address,
                    data,
                } => access.write_block(block, address, data)?,
                Step::Gate { open } => access.set_gate(open)?,
                Step::Probe { address, byte } => {
                    let address = i2c(address);
                    let wrote = access.i2c_write(address, &[byte]).is_ok();
                    let read = access.i2c_read(address, &mut [0]).is_ok();
                    if wrote || read {
                        return Err(Gap::new(OTHER_BOARD).into());
                    }
                }
                Step::Identify { address } => {
                    let mut r0 = [0];
                    let len = access.i2c_read(i2c(address), &mut r0)?;
                    let identified = Registers::from_read(&r0[..len])
                        .expect("invariant: at most one register read")
                        .has_chip_id();
                    if !identified {
                        return Err(Gap::new(OTHER_BOARD).into());
                    }
                }
                Step::I2cWrite { address, data } => access.i2c_write(i2c(address), data)?,
                Step::I2cRead { address, len } => {
                    access.i2c_read(i2c(address), &mut vec![0; len])?;
                }
            }
        }
        Ok(())
    }
}

/// A sequence's I2C address.
fn i2c(address: u8) -> I2cAddress {
    I2cAddress::new(address).expect("invariant: the sequences' addresses are 7-bit")
}
