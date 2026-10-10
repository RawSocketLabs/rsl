//! The board description the driver needs before it can service SPI, I2C and DMA
//! (`PCIE_DRIVER_SET_DEVLAYOUT`).
//!
//! The driver accepts a layout once per module load and silently ignores any later one, so a
//! wrong value persists for every user of the device until the module is reloaded. The
//! caller owns the values (they are gateware facts); this module only encodes them.

use crate::uapi::{self, DevLayout};

/// A core the driver drives directly: its register, its type code and its interrupt line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Core {
    /// The core's register.
    pub base: u32,

    /// The core's type code (libusdr's `device_cores.h`).
    pub core: u32,

    /// The interrupt it raises.
    pub irq: u32,
}

/// A register window reaching addresses past the mapped page: an index register, the data
/// register after it, and the first address it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedSpace {
    /// The index register; data is the next one.
    pub base: u32,

    /// The first address the window reaches.
    pub first: u32,
}

/// A DMA stream's registers, core and capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stream {
    /// The stream's confirm register (released buffers are acknowledged here).
    pub confirm_base: u32,

    /// Where the stream's configuration space starts (through the indexed window).
    pub config_base: u32,

    /// The stream core's type code.
    pub core: u32,

    /// The interrupt it raises.
    pub irq: u32,

    /// The DMA capability word: buffer counts and sizes the engine allows.
    pub capability: u32,
}

/// A board's cores, as the driver needs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardLayout {
    /// The UUID the driver reports for this board (`PCIE_DRIVER_GET_UUID`).
    pub uuid: [u8; 16],

    /// Interrupt lines the gateware has.
    pub interrupt_count: u32,

    /// The interrupt routing register.
    pub interrupt_base: u32,

    /// SPI cores, at most 8.
    pub spi: &'static [Core],

    /// I2C cores, at most 4.
    pub i2c: &'static [Core],

    /// Indexed register windows, at most 4.
    pub indexed: &'static [IndexedSpace],

    /// DMA streams, RX first, at most 16.
    pub streams: &'static [Stream],

    /// The event bucket's register and core; the driver requires exactly one.
    pub bucket: (u32, u32),

    /// The events `poll` reports as readable and writable.
    pub poll_events: (i32, i32),
}

/// A layout the driver's structure cannot hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("board layout has too many {0}")]
pub struct LayoutTooLarge(pub &'static str);

impl BoardLayout {
    /// The bytes `PCIE_DRIVER_SET_DEVLAYOUT` sends: the driver's 596-byte structure, every
    /// slot beyond the counts zero.
    ///
    /// # Errors
    ///
    /// [`LayoutTooLarge`] if a list exceeds the structure's slots.
    pub fn encode(self) -> Result<[u8; 596], LayoutTooLarge> {
        let layout = self.to_uapi()?;
        let words = [
            layout.spi_cnt,
            layout.i2c_cnt,
            layout.idx_regsp_cnt,
            layout.interrupt_count,
            layout.interrupt_core,
            layout.interrupt_base,
            layout.streams_count,
            layout.intfifo_count,
        ]
        .into_iter()
        .chain(layout.spi_base)
        .chain(layout.spi_core)
        .chain(layout.spi_int_number)
        .chain(layout.i2c_base)
        .chain(layout.i2c_core)
        .chain(layout.i2c_int_number)
        .chain(layout.idx_regsp_base)
        .chain(layout.idx_regsp_vbase)
        .chain(layout.stream_cfg_base)
        .chain(layout.stream_cnf_base)
        .chain(layout.stream_core)
        .chain(layout.stream_cap)
        .chain(layout.stream_int_number)
        .chain(layout.intfifo_uaddr)
        .chain(layout.intfifo_length)
        .chain(layout.intfifo_flags)
        .chain([layout.bucket_core, layout.bucket_base, layout.bucket_count])
        .chain([
            u32::from_ne_bytes(layout.poll_event_rd.to_ne_bytes()),
            u32::from_ne_bytes(layout.poll_event_wr.to_ne_bytes()),
        ]);
        let mut bytes = [0; 596];
        for (chunk, word) in bytes.chunks_exact_mut(4).zip(words) {
            chunk.copy_from_slice(&word.to_ne_bytes());
        }
        Ok(bytes)
    }

    /// The driver's structure.
    pub(crate) fn to_uapi(self) -> Result<DevLayout, LayoutTooLarge> {
        let count = |len: usize, max: usize, what| {
            if len > max {
                return Err(LayoutTooLarge(what));
            }
            Ok(u32::try_from(len).expect("invariant: at most 16"))
        };
        let mut layout = DevLayout {
            spi_cnt: count(self.spi.len(), uapi::MAX_SPI, "SPI cores")?,
            i2c_cnt: count(self.i2c.len(), uapi::MAX_I2C, "I2C cores")?,
            idx_regsp_cnt: count(self.indexed.len(), uapi::MAX_INDEXED, "indexed windows")?,
            interrupt_count: self.interrupt_count,
            interrupt_base: self.interrupt_base,
            streams_count: count(self.streams.len(), uapi::MAX_STREAMS, "streams")?,
            bucket_base: self.bucket.0,
            bucket_core: self.bucket.1,
            bucket_count: 1,
            poll_event_rd: self.poll_events.0,
            poll_event_wr: self.poll_events.1,
            ..DevLayout::default()
        };
        for (i, spi) in self.spi.iter().enumerate() {
            layout.spi_base[i] = spi.base;
            layout.spi_core[i] = spi.core;
            layout.spi_int_number[i] = spi.irq;
        }
        for (i, i2c) in self.i2c.iter().enumerate() {
            layout.i2c_base[i] = i2c.base;
            layout.i2c_core[i] = i2c.core;
            layout.i2c_int_number[i] = i2c.irq;
        }
        for (i, window) in self.indexed.iter().enumerate() {
            layout.idx_regsp_base[i] = window.base;
            layout.idx_regsp_vbase[i] = window.first;
        }
        for (i, stream) in self.streams.iter().enumerate() {
            layout.stream_cnf_base[i] = stream.confirm_base;
            layout.stream_cfg_base[i] = stream.config_base;
            layout.stream_core[i] = stream.core;
            layout.stream_int_number[i] = stream.irq;
            layout.stream_cap[i] = stream.capability;
        }
        Ok(layout)
    }
}
