// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

use crate::ParseUnitError;

pub(crate) fn validate_range(
    value: u64,
    min: u64,
    max: u64,
    unit: &'static str,
) -> Result<(), ParseUnitError> {
    if !(min..=max).contains(&value) {
        return Err(ParseUnitError::OutOfRange {
            value,
            min,
            max,
            unit,
        });
    }
    Ok(())
}
