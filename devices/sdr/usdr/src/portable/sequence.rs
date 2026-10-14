//! The register-sequence seam: `hal`'s, under the portable tree's path.

pub(crate) use hal::{
    FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure, i2c_byte,
};
