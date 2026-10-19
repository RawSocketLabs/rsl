//! Register-level models of libusdr's front-end boards on the M.2 key A+E connector, as
//! its `device/ext_*` drivers reach them over the connector's I2C bus: the `PCIe` front
//! end, the M.2-to-`PCIe` breakout, XMASS, `SuperSync` and `SimpleSync`. Shared by the
//! simulated boards whose libusdr probes that connector (`usdr-sim`, `xsdr-sim`), so the
//! shared `sdr-frontend` bring-ups are compared with libusdr's on every driver.
//!
//! A host sim attaches a [`FrontEnd`] as a [`Model`] and hands it the transfers to its
//! connector's bus; [`Model::chip`] names the chip answering at an address, if any, and
//! the host answers the idle bus otherwise. The connector's GPIO core (FPGA register 3)
//! is only traced by the host: no modelled chip depends on the pins it sets.
//!
//! [`chips`] also offers the plain register files and the temperature sensor a host board
//! carries itself (`usdr-sim`'s PMIC and sensor). Each modelling assumption is stated,
//! with its evidence, where it is encoded.
#![forbid(unsafe_code)]

pub mod chips;
mod model;

pub use model::{FrontEnd, Model};
