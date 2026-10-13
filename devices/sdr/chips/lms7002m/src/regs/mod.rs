//! Register value types, one module per register page of libusdr's YAML (the processors'
//! and the `LimeLight` interface's split in two by role).

mod afe;
mod baseband;
mod cgen;
mod dsp;
mod ldo;
mod limelight;
mod lml;
mod rfe;
mod sxx;
mod trf;
mod tsp;

pub use afe::{AdcMode, Afe, AfeMux1, AfeMux2, Xbuf};
pub use baseband::{
    PgaInput, RbbControl, RbbHighFilter, RbbInput, RbbLowFilter, RbbPga, RbbPgaFeedback,
    TbbAmplifier, TbbControl, TbbFilterCal, TbbS5, TbbTest,
};
pub use cgen::{
    CgenDivider, CgenEnable, CgenFracLow, CgenIntFrac, CgenVco, VcoComparators, VcoReading,
};
pub use dsp::{BistStatus, NcoMode, RxTspDc, RxTspDecimation, TxTspInterpolation};
pub use ldo::{LdoBypass, LdoBypassLoad, LdoDigital, LdoEnables, LdoEnablesLoad};
pub use limelight::{
    CdsDelay, CdsInvert, LmlClockDivider, LmlClockMux, LmlClockSource, LmlDirection, LmlModes,
    LmlPads, LmlPortMap, LmlRxSource, LmlSample, MclkSource,
};
pub use lml::{LmlControl, Mac, MimoSiso, SUPPORTED, Version};
pub use rfe::{NextChannel, RxBias, RxFrontEnd, RxGains, RxInput, RxPath};
pub use sxx::{
    EnDir, SxxChargePump, SxxControl, SxxFracLow, SxxIntFrac, SxxLoDivider, SxxVco, SxxVcoBias,
    SxxVcoSelect,
};
pub use trf::{TxBand, TxFrontEnd, TxPad};
pub use tsp::{
    Capture, CaptureAdc, MixerDirection, RxTspBypass, RxTspMode, TsgMode, TsgScale, TspInput,
    TxTspBypass, TxTspMode,
};
