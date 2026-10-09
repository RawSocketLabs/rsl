//! Register access as libusdr's transports perform it: registers in the stream engine's
//! configuration space (from 0x10000) through an index/data window, the rest directly, with
//! consecutive pairs from an even register in one 64-bit write (`pcie_reg_op_iommap` in
//! libusdr's `pcie_uram_main.c`).

use crate::lowlevel::BusError;

/// An index/data register window: write a register's offset to `index`, then read or write
/// it through the register after.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Window {
    /// The index register; the data register follows it.
    pub(crate) index: u32,

    /// The first register the window reaches; registers below it are direct.
    pub(crate) first: u32,
}

impl Window {
    /// The data register.
    const fn data(self) -> u32 {
        self.index + 1
    }
}

/// A transport's direct register access.
pub(crate) trait RegisterIo {
    /// Reads one register.
    fn read(&mut self, reg: u32) -> Result<u32, BusError>;

    /// Writes one register.
    fn write(&mut self, reg: u32, value: u32) -> Result<(), BusError>;

    /// Writes an even register and the next in one access.
    fn write_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), BusError>;
}

/// Writes consecutive registers from `addr`.
pub(crate) fn write_regs(
    io: &mut dyn RegisterIo,
    window: Window,
    addr: u32,
    values: &[u32],
) -> Result<(), BusError> {
    if addr >= window.first {
        // The index is set for every word: the window does not auto-increment.
        for (reg, &value) in (addr - window.first..).zip(values) {
            io.write(window.index, reg)?;
            io.write(window.data(), value)?;
        }
        return Ok(());
    }
    let mut rest = values;
    let mut reg = addr;
    while let [first, tail @ ..] = rest {
        match tail {
            [second, after @ ..] if reg % 2 == 0 => {
                io.write_pair(reg, *first, *second)?;
                rest = after;
                reg += 2;
            }
            _ => {
                io.write(reg, *first)?;
                rest = tail;
                reg += 1;
            }
        }
    }
    Ok(())
}

/// Reads consecutive registers from `addr`.
pub(crate) fn read_regs(
    io: &mut dyn RegisterIo,
    window: Window,
    addr: u32,
    out: &mut [u32],
) -> Result<(), BusError> {
    for (reg, value) in (addr..).zip(out) {
        *value = if reg >= window.first {
            io.write(window.index, reg - window.first)?;
            io.read(window.data())?
        } else {
            io.read(reg)?
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The uSDR's window.
    const WINDOW: Window = Window {
        index: 6,
        first: 0x1_0000,
    };
    /// Its index register.
    const INDEX: u32 = 6;
    /// Its data register.
    const DATA: u32 = 7;

    /// The accesses a transport would make.
    #[derive(Debug, PartialEq, Eq)]
    enum Access {
        Read(u32),
        Write(u32, u32),
        Pair(u32, u32, u32),
    }

    /// Records accesses; reads return the register number.
    #[derive(Default)]
    struct Recorder(Vec<Access>);

    impl RegisterIo for Recorder {
        fn read(&mut self, reg: u32) -> Result<u32, BusError> {
            self.0.push(Access::Read(reg));
            Ok(reg)
        }

        fn write(&mut self, reg: u32, value: u32) -> Result<(), BusError> {
            self.0.push(Access::Write(reg, value));
            Ok(())
        }

        fn write_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), BusError> {
            self.0.push(Access::Pair(reg, first, second));
            Ok(())
        }
    }

    #[test]
    fn pairs_start_on_even_registers() {
        let mut io = Recorder::default();
        write_regs(&mut io, WINDOW, 3, &[10, 11, 12, 13]).expect("writes");
        assert_eq!(
            io.0,
            [
                Access::Write(3, 10),
                Access::Pair(4, 11, 12),
                Access::Write(6, 13)
            ]
        );
    }

    #[test]
    fn window_writes_set_the_index_for_every_word() {
        let mut io = Recorder::default();
        write_regs(&mut io, WINDOW, 0x1_0100, &[0xa, 0xb]).expect("writes");
        assert_eq!(
            io.0,
            [
                Access::Write(INDEX, 0x100),
                Access::Write(DATA, 0xa),
                Access::Write(INDEX, 0x101),
                Access::Write(DATA, 0xb),
            ]
        );
    }

    #[test]
    fn window_reads_go_through_the_data_register() {
        let mut io = Recorder::default();
        let mut out = [0; 2];
        read_regs(&mut io, WINDOW, 16, &mut out[..1]).expect("reads");
        read_regs(&mut io, WINDOW, 0x1_0006, &mut out[1..]).expect("reads");
        assert_eq!(
            io.0,
            [
                Access::Read(16),
                Access::Write(INDEX, 6),
                Access::Read(DATA)
            ]
        );
        assert_eq!(out, [16, DATA]);
    }
}
