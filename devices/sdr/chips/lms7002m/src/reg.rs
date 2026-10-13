//! The register address map: every register a ported operation reads or writes.
//!
//! Addresses and register names are libusdr's generated header (`gen/def_lms7002m.h`, from
//! `hw/lms7002m/lms7002m.yaml`), which names each register by its block and address.

use bnb::BitEnum;

/// Register addresses (15 bits; bit 15 of the SPI address half is the write flag).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u16, closed)]
#[repr(u16)]
pub enum Reg {
    /// See [`LmlControl`](crate::LmlControl). `LML_0x0020`.
    LmlControl = 0x0020,

    /// See [`MimoSiso`](crate::MimoSiso). `LML_0x002E`.
    MimoSiso = 0x002e,

    /// See [`Version`](crate::Version). `LML_0x002F`, read only.
    Version = 0x002f,

    /// See [`Afe`](crate::Afe). `AFE_0x0082`.
    Afe = 0x0082,

    /// See [`Xbuf`](crate::Xbuf). `XBUF_0x0085`.
    Xbuf = 0x0085,

    /// See [`LdoEnables`](crate::LdoEnables). `LDO_0x0092`.
    LdoEnables = 0x0092,

    /// See [`LdoEnablesLoad`](crate::LdoEnablesLoad). `LDO_0x0093`.
    LdoEnablesLoad = 0x0093,

    /// See [`LdoBypassLoad`](crate::LdoBypassLoad). `LDO_0x0095`.
    LdoBypassLoad = 0x0095,

    /// See [`LdoBypass`](crate::LdoBypass). `LDO_0x0096`.
    LdoBypass = 0x0096,

    /// See [`LdoDigital`](crate::LdoDigital). `LDO_0x00A6`.
    LdoDigital = 0x00a6,

    /// See [`TxFrontEnd`](crate::TxFrontEnd). `TRF_0x0100`, per channel.
    TxFrontEnd = 0x0100,

    /// See [`TxPad`](crate::TxPad). `TRF_0x0101`, per channel.
    TxPad = 0x0101,

    /// See [`TxBand`](crate::TxBand). `TRF_0x0103`.
    TxBand = 0x0103,

    /// See [`RxFrontEnd`](crate::RxFrontEnd). `RFE_0x010C`, per channel.
    RxFrontEnd = 0x010c,

    /// See [`RxInput`](crate::RxInput). `RFE_0x010D`, per channel.
    RxInput = 0x010d,

    /// See [`RxBias`](crate::RxBias). `RFE_0x010F`, per channel.
    RxBias = 0x010f,

    /// See [`EnDir`](crate::EnDir). `SXX_0x0124`, per channel.
    EnDir = 0x0124,

    /// See [`TxTspMode`](crate::TxTspMode). `TXTSP_0x0200`.
    TxTspMode = 0x0200,

    /// See [`TxTspBypass`](crate::TxTspBypass). `TXTSP_0x0208`.
    TxTspBypass = 0x0208,

    /// See [`RxTspMode`](crate::RxTspMode). `RXTSP_0x0400`.
    RxTspMode = 0x0400,

    /// See [`RxTspBypass`](crate::RxTspBypass). `RXTSP_0x040C`.
    RxTspBypass = 0x040c,
}
