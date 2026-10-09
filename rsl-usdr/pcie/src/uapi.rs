// Copyright (c) 2023-2024 Wavelet Lab
// SPDX-License-Identifier: MIT
//
// Transcribed from `pcie_uram_driver_if.h` (vendored under
// `oracle/libusdr/lib/lowlevel/pcie_uram/`), dual-licensed (GPL-2.0 WITH Linux-syscall-note)
// OR MIT; taken under MIT. The oracle checks every size, offset and opcode here against
// that header.

//! The `usdr_pcie_uram` driver's ioctl structures and request codes.
//!
//! The driver has no 32-bit compatibility layer and these structures carry pointers and
//! `off_t`/`size_t`, so only 64-bit targets are supported.

use rustix::ioctl::{Opcode, opcode};

#[cfg(not(target_pointer_width = "64"))]
compile_error!("usdr_pcie_uram's ioctl structures are laid out for 64-bit userspace only");

/// The ioctl group (`PCIE_DRIVER_MAGIC`).
const MAGIC: u8 = 0xdd;

/// Most SPI cores a layout describes (`MAX_SPI_COUNT`).
pub(crate) const MAX_SPI: usize = 8;
/// Most I2C cores (`MAX_I2C_COUNT`).
pub(crate) const MAX_I2C: usize = 4;
/// Most indexed register spaces (`MAX_INDEXED_SPACES`).
pub(crate) const MAX_INDEXED: usize = 4;
/// Most DMA streams (`MAX_STREAM_COUNT`).
pub(crate) const MAX_STREAMS: usize = 16;
/// Most interrupt FIFOs (`MAX_MEMORY_COUNT`).
pub(crate) const MAX_FIFOS: usize = 4;

/// `struct pcie_driver_uuid`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Uuid {
    pub(crate) id: [u8; 16],
}

/// `struct pcie_driver_devlayout`: the board's cores and their interrupts, which the
/// driver needs to service SPI, I2C and DMA.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DevLayout {
    pub(crate) spi_cnt: u32,
    pub(crate) i2c_cnt: u32,
    pub(crate) idx_regsp_cnt: u32,
    pub(crate) interrupt_count: u32,
    pub(crate) interrupt_core: u32,
    pub(crate) interrupt_base: u32,
    pub(crate) streams_count: u32,
    pub(crate) intfifo_count: u32,
    pub(crate) spi_base: [u32; MAX_SPI],
    pub(crate) spi_core: [u32; MAX_SPI],
    pub(crate) spi_int_number: [u32; MAX_SPI],
    pub(crate) i2c_base: [u32; MAX_I2C],
    pub(crate) i2c_core: [u32; MAX_I2C],
    pub(crate) i2c_int_number: [u32; MAX_I2C],
    pub(crate) idx_regsp_base: [u32; MAX_INDEXED],
    pub(crate) idx_regsp_vbase: [u32; MAX_INDEXED],
    pub(crate) stream_cfg_base: [u32; MAX_STREAMS],
    pub(crate) stream_cnf_base: [u32; MAX_STREAMS],
    pub(crate) stream_core: [u32; MAX_STREAMS],
    pub(crate) stream_cap: [u32; MAX_STREAMS],
    pub(crate) stream_int_number: [u32; MAX_STREAMS],
    pub(crate) intfifo_uaddr: [u32; MAX_FIFOS],
    pub(crate) intfifo_length: [u32; MAX_FIFOS],
    pub(crate) intfifo_flags: [u32; MAX_FIFOS],
    pub(crate) bucket_core: u32,
    pub(crate) bucket_base: u32,
    pub(crate) bucket_count: u32,
    pub(crate) poll_event_rd: i32,
    pub(crate) poll_event_wr: i32,
}

/// `struct pcie_driver_spi32`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Spi32 {
    pub(crate) buscfg: u32,
    pub(crate) dw_io: u32,
}

/// `struct pcie_driver_si2c`. The trailing pointers are for transfers over eight bytes,
/// which the uSDR's I2C core does not allow; they stay null.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si2c {
    pub(crate) addr: u32,
    pub(crate) wcnt: u32,
    pub(crate) rcnt: u32,
    pub(crate) wrb: [u8; 8],
    pub(crate) rdb: [u8; 8],
    pub(crate) wrb_p: *mut core::ffi::c_void,
    pub(crate) rdb_p: *mut core::ffi::c_void,
}

/// `enum sdma_stream_type`: `STREAM_MMAPED`, the buffers mapped into userspace.
pub(crate) const STREAM_MMAPED: u32 = 0;

/// `struct pcie_driver_sdma_conf`. `off_t` and `size_t` are 64-bit here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SdmaConf {
    pub(crate) kind: u32,
    pub(crate) sno: u32,
    pub(crate) dma_bufs: u32,
    pub(crate) dma_buf_sz: u32,
    pub(crate) out_vma_off: i64,
    pub(crate) out_vma_length: u64,
}

/// `struct pcie_driver_woa_oob`: a DMA wait's stream and timeout in, out-of-band records
/// out.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct WaitOob {
    /// `(timeout_ms << 8) | stream`.
    pub(crate) streamnoto: u32,
    /// The record buffer's length in; the bytes written out.
    pub(crate) ooblength: u32,
    pub(crate) oobdata: *mut core::ffi::c_void,
}

/// `PCIE_DRIVER_GET_UUID`.
pub(crate) const GET_UUID: Opcode = opcode::read::<Uuid>(MAGIC, 0);
/// `PCIE_DRIVER_CLAIM`: also resets the driver's cached I2C address table.
pub(crate) const CLAIM: Opcode = opcode::none(MAGIC, 1);
/// `PCIE_DRIVER_SET_DEVLAYOUT`.
pub(crate) const SET_DEVLAYOUT: Opcode = opcode::write::<DevLayout>(MAGIC, 2);
/// `PCIE_DRIVER_SPI32_TRANSACT`.
pub(crate) const SPI32_TRANSACT: Opcode = opcode::read_write::<Spi32>(MAGIC, 4);
/// `PCIE_DRIVER_SI2C_TRANSACT`.
pub(crate) const SI2C_TRANSACT: Opcode = opcode::read_write::<Si2c>(MAGIC, 5);
/// `PCIE_DRIVER_DMA_CONF`.
pub(crate) const DMA_CONF: Opcode = opcode::read_write::<SdmaConf>(MAGIC, 16);
/// `PCIE_DRIVER_DMA_UNCONF`: the stream number is the argument itself.
pub(crate) const DMA_UNCONF: Opcode = opcode::write::<u32>(MAGIC, 16);
/// `PCIE_DRIVER_DMA_WAIT_OOB`: returns how many buffers are ready.
pub(crate) const DMA_WAIT_OOB: Opcode = opcode::read_write::<WaitOob>(MAGIC, 17);
/// `PCIE_DRIVER_CLAIM_VERSION`: the version is the argument itself.
pub(crate) const CLAIM_VERSION: Opcode = opcode::write::<u32>(MAGIC, 23);
/// `PCIE_DRIVER_DMA_RELEASE`: the stream number is the argument itself; the driver writes
/// its bits 31:8 to the stream's confirm register, so they must be zero.
pub(crate) const DMA_RELEASE: Opcode = opcode::write::<u32>(MAGIC, 24);

/// The interface version this transport speaks (`CLAIM_VERSION`'s argument).
pub(crate) const INTERFACE_VERSION: u32 = 3;

// Layout checks against the C header, for 64-bit targets; the oracle repeats them against
// the header itself.
const _: () = {
    assert!(size_of::<Uuid>() == 16);
    assert!(size_of::<DevLayout>() == 596);
    assert!(core::mem::offset_of!(DevLayout, spi_base) == 32);
    assert!(core::mem::offset_of!(DevLayout, i2c_base) == 128);
    assert!(core::mem::offset_of!(DevLayout, idx_regsp_base) == 176);
    assert!(core::mem::offset_of!(DevLayout, stream_cfg_base) == 208);
    assert!(core::mem::offset_of!(DevLayout, intfifo_uaddr) == 528);
    assert!(core::mem::offset_of!(DevLayout, bucket_core) == 576);
    assert!(core::mem::offset_of!(DevLayout, poll_event_wr) == 592);
    assert!(size_of::<Spi32>() == 8);
    assert!(size_of::<Si2c>() == 48);
    assert!(core::mem::offset_of!(Si2c, rdb) == 20);
    assert!(core::mem::offset_of!(Si2c, wrb_p) == 32);
    assert!(size_of::<SdmaConf>() == 32);
    assert!(core::mem::offset_of!(SdmaConf, out_vma_off) == 16);
    assert!(size_of::<WaitOob>() == 16);
    assert!(core::mem::offset_of!(WaitOob, oobdata) == 8);
};
