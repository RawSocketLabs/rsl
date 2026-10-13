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

    /// See [`LmlPads`](crate::LmlPads). `LML_0x0022`.
    LmlPads = 0x0022,

    /// See [`LmlModes`](crate::LmlModes). `LML_0x0023`.
    LmlModes = 0x0023,

    /// Port 1's [`LmlPortMap`](crate::LmlPortMap). `LML_0x0024`.
    Lml1Map = 0x0024,

    /// Port 2's [`LmlPortMap`](crate::LmlPortMap). `LML_0x0027`.
    Lml2Map = 0x0027,

    /// See [`LmlClockMux`](crate::LmlClockMux). `LML_0x002A`.
    LmlClockMux = 0x002a,

    /// See [`LmlClockSource`](crate::LmlClockSource). `LML_0x002B`.
    LmlClockSource = 0x002b,

    /// See [`LmlClockDivider`](crate::LmlClockDivider). `LML_0x002C`.
    LmlClockDivider = 0x002c,

    /// See [`CgenEnable`](crate::CgenEnable). `CGEN_ENABLE`.
    CgenEnable = 0x0086,

    /// See [`CgenFracLow`](crate::CgenFracLow). `CGEN_0x0087`.
    CgenFracLow = 0x0087,

    /// See [`CgenIntFrac`](crate::CgenIntFrac). `CGEN_0x0088`.
    CgenIntFrac = 0x0088,

    /// See [`CgenDivider`](crate::CgenDivider). `CGEN_0x0089`.
    CgenDivider = 0x0089,

    /// See [`CgenVco`](crate::CgenVco). `CGEN_0x008B`.
    CgenVco = 0x008b,

    /// The CGEN [`VcoComparators`](crate::VcoComparators), read only. `CGEN_0x008C`.
    CgenComparators = 0x008c,

    /// See [`CdsInvert`](crate::CdsInvert). `CDS_0x00AD`.
    CdsInvert = 0x00ad,

    /// See [`CdsDelay`](crate::CdsDelay). `CDS_0x00AE`.
    CdsDelay = 0x00ae,

    /// See [`TbbControl`](crate::TbbControl). `TBB_0x0105`, per channel.
    TbbControl = 0x0105,

    /// See [`TbbAmplifier`](crate::TbbAmplifier). `TBB_0x0108`, per channel.
    TbbAmplifier = 0x0108,

    /// See [`TbbFilterCal`](crate::TbbFilterCal). `TBB_0x0109`, per channel.
    TbbFilterCal = 0x0109,

    /// See [`TbbTest`](crate::TbbTest). `TBB_0x010A`, per channel.
    TbbTest = 0x010a,

    /// See [`TbbS5`](crate::TbbS5). `TBB_LPFS5`, per channel.
    TbbS5 = 0x010b,

    /// See [`RxGains`](crate::RxGains). `RFE_0x0113`, per channel.
    RxGains = 0x0113,

    /// See [`RbbControl`](crate::RbbControl). `RBB_0x0115`, per channel.
    RbbControl = 0x0115,

    /// See [`RbbHighFilter`](crate::RbbHighFilter). `RBB_0x0116`, per channel.
    RbbHighFilter = 0x0116,

    /// See [`RbbLowFilter`](crate::RbbLowFilter). `RBB_0x0117`, per channel.
    RbbLowFilter = 0x0117,

    /// See [`RbbInput`](crate::RbbInput). `RBB_0x0118`, per channel.
    RbbInput = 0x0118,

    /// See [`RbbPga`](crate::RbbPga). `RBB_0x0119`, per channel.
    RbbPga = 0x0119,

    /// See [`RbbPgaFeedback`](crate::RbbPgaFeedback). `RBB_0x011A`, per channel.
    RbbPgaFeedback = 0x011a,

    /// See [`SxxControl`](crate::SxxControl). `SXX_0x011C`, per synthesizer.
    SxxControl = 0x011c,

    /// See [`SxxFracLow`](crate::SxxFracLow). `SXX_0x011D`, per synthesizer.
    SxxFracLow = 0x011d,

    /// See [`SxxIntFrac`](crate::SxxIntFrac). `SXX_0x011E`, per synthesizer.
    SxxIntFrac = 0x011e,

    /// See [`SxxLoDivider`](crate::SxxLoDivider). `SXX_0x011F`, per synthesizer.
    SxxLoDivider = 0x011f,

    /// See [`SxxVcoBias`](crate::SxxVcoBias). `SXX_0x0120`, per synthesizer.
    SxxVcoBias = 0x0120,

    /// See [`SxxVco`](crate::SxxVco). `SXX_0x0121`, per synthesizer.
    SxxVco = 0x0121,

    /// See [`SxxChargePump`](crate::SxxChargePump). `SXX_0x0122`, per synthesizer.
    SxxChargePump = 0x0122,

    /// The synthesizer's [`VcoComparators`](crate::VcoComparators), read only. `SXX_0x0123`.
    SxxComparators = 0x0123,

    /// See [`TxTspInterpolation`](crate::TxTspInterpolation). `TXTSP_0x0203`.
    TxTspInterpolation = 0x0203,

    /// The TX processor's [`BistStatus`](crate::BistStatus), read only. `TXTSP_0x0209`.
    TxTspBistStatus = 0x0209,

    /// The TX NCO's [`NcoMode`](crate::NcoMode). `TXNCO_0x0240`.
    TxNcoMode = 0x0240,

    /// The TX NCO's frequency word, bits 31:16. `TXNCO_0x0242`.
    TxNcoFcwHigh = 0x0242,

    /// The TX NCO's frequency word, bits 15:0. `TXNCO_0x0243`.
    TxNcoFcwLow = 0x0243,

    /// See [`RxTspDecimation`](crate::RxTspDecimation). `RXTSP_0x0403`.
    RxTspDecimation = 0x0403,

    /// See [`RxTspDc`](crate::RxTspDc). `RXTSP_0x0404`.
    RxTspDc = 0x0404,

    /// The RX processor's [`BistStatus`](crate::BistStatus), read only. `RXTSP_0x040E`.
    RxTspBistStatus = 0x040e,

    /// The RX NCO's [`NcoMode`](crate::NcoMode). `RXNCO_0x0440`.
    RxNcoMode = 0x0440,

    /// The RX NCO's frequency word, bits 31:16. `RXNCO_0x0442`.
    RxNcoFcwHigh = 0x0442,

    /// The RX NCO's frequency word, bits 15:0. `RXNCO_0x0443`.
    RxNcoFcwLow = 0x0443,
}
