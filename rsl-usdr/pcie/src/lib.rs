//! The `usdr_pcie_uram` Linux driver's userspace interface, for the Wavelet Lab uSDR.
//!
//! This crate is the only place in the rsl-usdr stack that uses `unsafe`: ioctls against
//! the driver, the mapped register page and the mapped DMA buffers. It knows the driver's
//! protocol and nothing about the board beyond the layout the caller passes in;
//! `rsl-usdr`'s `pcie` feature builds its `Bus` on [`PcieDevice`].
//!
//! Linux on 64-bit targets only; elsewhere the crate is empty.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "linux")]
mod device;
#[cfg(target_os = "linux")]
mod layout;
#[cfg(target_os = "linux")]
mod ring;
#[cfg(target_os = "linux")]
mod uapi;

#[cfg(target_os = "linux")]
pub use device::{OpenError, PcieDevice, PcieError, discover};
#[cfg(target_os = "linux")]
pub use layout::{BoardLayout, Core, IndexedSpace, LayoutTooLarge, Stream};

/// The driver's structure sizes, field offsets and request codes as this crate encodes them,
/// by the C name; the oracle compares them with the header.
#[cfg(target_os = "linux")]
#[doc(hidden)]
#[must_use]
pub fn abi() -> Vec<(&'static str, u64)> {
    use core::mem::offset_of;

    use uapi::{DevLayout, SdmaConf, Si2c, Spi32, Uuid, WaitOob};
    let size = |bytes: usize| bytes as u64;
    vec![
        ("sizeof(pcie_driver_uuid)", size(size_of::<Uuid>())),
        (
            "sizeof(pcie_driver_devlayout)",
            size(size_of::<DevLayout>()),
        ),
        ("sizeof(pcie_driver_spi32)", size(size_of::<Spi32>())),
        ("sizeof(pcie_driver_si2c)", size(size_of::<Si2c>())),
        (
            "offsetof(pcie_driver_si2c, rdb)",
            size(offset_of!(Si2c, rdb)),
        ),
        (
            "offsetof(pcie_driver_si2c, wrb_p)",
            size(offset_of!(Si2c, wrb_p)),
        ),
        ("sizeof(pcie_driver_sdma_conf)", size(size_of::<SdmaConf>())),
        (
            "offsetof(pcie_driver_sdma_conf, out_vma_off)",
            size(offset_of!(SdmaConf, out_vma_off)),
        ),
        (
            "offsetof(pcie_driver_sdma_conf, out_vma_length)",
            size(offset_of!(SdmaConf, out_vma_length)),
        ),
        ("sizeof(pcie_driver_woa_oob)", size(size_of::<WaitOob>())),
        (
            "offsetof(pcie_driver_woa_oob, oobdata)",
            size(offset_of!(WaitOob, oobdata)),
        ),
        (
            "offsetof(pcie_driver_devlayout, idx_regsp_vbase)",
            size(offset_of!(DevLayout, idx_regsp_vbase)),
        ),
        (
            "offsetof(pcie_driver_devlayout, stream_cap)",
            size(offset_of!(DevLayout, stream_cap)),
        ),
        (
            "offsetof(pcie_driver_devlayout, bucket_base)",
            size(offset_of!(DevLayout, bucket_base)),
        ),
        ("PCIE_DRIVER_GET_UUID", u64::from(uapi::GET_UUID)),
        ("PCIE_DRIVER_CLAIM", u64::from(uapi::CLAIM)),
        ("PCIE_DRIVER_SET_DEVLAYOUT", u64::from(uapi::SET_DEVLAYOUT)),
        (
            "PCIE_DRIVER_SPI32_TRANSACT",
            u64::from(uapi::SPI32_TRANSACT),
        ),
        ("PCIE_DRIVER_SI2C_TRANSACT", u64::from(uapi::SI2C_TRANSACT)),
        ("PCIE_DRIVER_DMA_CONF", u64::from(uapi::DMA_CONF)),
        ("PCIE_DRIVER_DMA_UNCONF", u64::from(uapi::DMA_UNCONF)),
        ("PCIE_DRIVER_DMA_WAIT_OOB", u64::from(uapi::DMA_WAIT_OOB)),
        ("PCIE_DRIVER_CLAIM_VERSION", u64::from(uapi::CLAIM_VERSION)),
        ("PCIE_DRIVER_DMA_RELEASE", u64::from(uapi::DMA_RELEASE)),
    ]
}
