//! rsl-usdr's private LMK05318 plan, staging, map and register types, compiled into a
//! parity test as the driver's `chips::lmk05318` module tree (no testing API is public).
//! The including test supplies `crate::cal::search::gcd`.

#[path = "../../../src/chips/lmk05318/apll.rs"]
mod apll;
#[path = "../../../src/chips/lmk05318/control.rs"]
mod control;
#[path = "../../../src/chips/lmk05318/map.rs"]
pub(crate) mod map;
#[path = "../../../src/chips/lmk05318/output.rs"]
mod output;
#[path = "../../../src/chips/lmk05318/plan.rs"]
pub(crate) mod plan;
#[path = "../../../src/chips/lmk05318/reg.rs"]
mod reg;
#[path = "../../../src/chips/lmk05318/staging.rs"]
pub(crate) mod staging;
#[path = "../../../src/chips/lmk05318/xo.rs"]
mod xo;
