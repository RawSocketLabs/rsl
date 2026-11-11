//! The field-directive model: what a field's `#[br]`/`#[bw]`/`#[brw]`/`#[reserved]`
//! attributes say, how they are parsed, and which combinations are rejected.

mod auto_len;
mod br;
mod conflicts;
mod count_prefix;
mod directive;
mod reserved;
mod shape;

pub(super) use auto_len::{AutoLenSpec, AutoTarget};
pub(super) use br::{FieldBr, field_is_br_calc, field_is_ignore, field_is_temp, parse_field_br};
pub(super) use count_prefix::{CountPrefixSite, desugar_count_prefix, extract_count_prefix};
pub(super) use directive::D_BW_CALC;
pub(super) use reserved::{field_is_reserved, reserved_spec_value};
pub(super) use shape::{byte_array_len, option_elem, vec_elem};
