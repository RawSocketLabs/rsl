//! The uSDR over `PCIe`, through the `usdr_pcie_uram` Linux driver (`rsl-usdr-pcie`).
//!
//! [`PcieBus::open`] opens a device node (`/dev/usdrN`), checks it is a uSDR and hands the
//! driver the board's layout; the result is a [`Bus`] for
//! [`Device::builder`](crate::Device::builder). Registers go through the mapped BAR, with
//! the stream engine's configuration space reached through the index/data window; SPI, I2C
//! and DMA through the driver.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use rsl_usdr_pcie::{BoardLayout, Core, IndexedSpace, PcieDevice, PcieError, Stream};

use super::window::{self, RegisterIo, Window};
use crate::error::Error;
use crate::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};

/// The uSDR's cores as the driver needs them: libusdr's `s_params_m2_lm6_1_rev000` in
/// `device/m2_lm6_1/m2_lm6_1.c`, as its `PCIe` transport encodes them. The oracle checks the
/// encoding byte for byte against libusdr's.
///
/// The driver accepts a layout once per module load and ignores later ones.
pub const USDR_LAYOUT: BoardLayout = BoardLayout {
    // `M2_LM6_1_DEVICE_ID` (`device/device_ids.h`).
    uuid: [
        0x1f, 0x09, 0xe2, 0x53, 0xc8, 0xad, 0x42, 0xa1, 0x81, 0xab, 0x96, 0x0f, 0x73, 0xeb, 0x3c,
        0x62,
    ],
    interrupt_count: 8,
    // `M2PCI_REG_INT`.
    interrupt_base: 15,
    // `M2PCI_REG_SPI0`, a simple SPI core, `M2PCI_INT_SPI_0`.
    spi: &[Core {
        base: 2,
        core: 0x0001,
        irq: 2,
    }],
    // `M2PCI_REG_I2C`, a simple dual I2C core, `M2PCI_INT_I2C_0`.
    i2c: &[Core {
        base: 1,
        core: 0x0101,
        irq: 6,
    }],
    // `M2PCI_REG_WR_BADDR` reaching `VIRT_CFG_SFX_BASE`.
    indexed: &[IndexedSpace {
        base: 6,
        first: 0x1_0000,
    }],
    streams: &[
        // RX: `M2PCI_REG_WR_RXDMA_CONFIRM`, `VIRT_CFG_SFX_BASE`, `M2PCI_INT_RX`.
        Stream {
            confirm_base: 4,
            config_base: 0x1_0000,
            core: 0x0003,
            irq: 0,
            capability: 0x855,
        },
        // TX: `M2PCI_REG_WR_TXDMA_CNF_L`, `VIRT_CFG_SFX_BASE + 512`, `M2PCI_INT_TX`.
        Stream {
            confirm_base: 12,
            config_base: 0x1_0200,
            core: 0x0103,
            irq: 1,
            capability: 0x555,
        },
    ],
    // `M2PCI_REG_WR_PNTFY_CFG`, a 16-byte bucket.
    bucket: (8, 0x0009),
    // `M2PCI_INT_RX`, `M2PCI_INT_TX`.
    poll_events: (0, 1),
};

/// The register window [`USDR_LAYOUT`] gives the driver.
const WINDOW: Window = Window {
    index: USDR_LAYOUT.indexed[0].base,
    first: USDR_LAYOUT.indexed[0].first,
};

/// A uSDR on `PCIe`.
#[derive(Debug)]
pub struct PcieBus {
    /// The open device.
    device: PcieDevice,
}

impl PcieBus {
    /// Opens the uSDR at `path`, such as `/dev/usdr0`.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] if the node cannot be opened, is in use, is another board, or the
    /// driver refuses the layout.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        PcieDevice::open(path.as_ref(), USDR_LAYOUT)
            .map(|device| Self { device })
            .map_err(|error| Error::Open(BusError::Other(Box::new(error))))
    }

    /// The device nodes of every uSDR the driver has found, in name order.
    #[must_use]
    pub fn discover() -> Vec<PathBuf> {
        rsl_usdr_pcie::discover()
    }
}

impl RegisterIo for PcieDevice {
    fn read(&mut self, reg: u32) -> Result<u32, BusError> {
        self.read_reg(reg).map_err(bus_error)
    }

    fn write(&mut self, reg: u32, value: u32) -> Result<(), BusError> {
        self.write_reg(reg, value).map_err(bus_error)
    }

    fn write_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), BusError> {
        self.write_reg_pair(reg, first, second).map_err(bus_error)
    }
}

impl Bus for PcieBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        window::read_regs(&mut self.device, WINDOW, addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        window::write_regs(&mut self.device, WINDOW, addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.device
            .spi32(target.bus().into(), word)
            .map_err(bus_error)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.device
            .i2c(dev.bus(), dev.addr(), write, read)
            .map_err(bus_error)
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.device.dma_open(block_bytes).map_err(bus_error)
    }

    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.device.dma_recv(timeout, consume).map_err(bus_error)
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        self.device.dma_close().map_err(bus_error)
    }
}

/// The bus error for a device failure.
fn bus_error(error: PcieError) -> BusError {
    match error {
        PcieError::Timeout => BusError::Timeout,
        PcieError::Disconnected => BusError::Disconnected,
        other => BusError::Other(Box::new(other)),
    }
}
