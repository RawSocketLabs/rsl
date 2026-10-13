//! Register value types, one module per register page of libusdr's YAML.

mod afe;
mod ldo;
mod lml;
mod rfe;
mod sxx;
mod trf;
mod tsp;

pub use afe::{AdcMode, Afe, AfeMux1, AfeMux2, Xbuf};
pub use ldo::{LdoBypass, LdoBypassLoad, LdoDigital, LdoEnables, LdoEnablesLoad};
pub use lml::{LmlControl, Mac, MimoSiso, SUPPORTED, Version};
pub use rfe::{NextChannel, RxBias, RxFrontEnd, RxInput, RxPath};
pub use sxx::EnDir;
pub use trf::{TxBand, TxFrontEnd, TxPad};
pub use tsp::{
    Capture, CaptureAdc, MixerDirection, RxTspBypass, RxTspMode, TsgMode, TsgScale, TspInput,
    TxTspBypass, TxTspMode,
};
