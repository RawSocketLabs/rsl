//! How a sequence reads the outcome an executor reported for its action.

use hal::{FpgaAccess, SequenceAction, SequenceFailure};

use super::fpga;

/// The value `action` reported, or the failure that ends the sequence. `chip` names the
/// device behind an I²C access or SPI exchange; gateware registers are always the FPGA's. A
/// wait cannot fail: its outcome only says it has passed.
///
/// # Errors
///
/// The executor's error, with the access it struck.
pub(crate) fn outcome<E>(
    action: SequenceAction,
    result: Result<u32, E>,
    chip: &'static str,
) -> Result<u32, SequenceFailure<E>> {
    result.or_else(|source| match action {
        SequenceAction::Sleep(_) => Ok(0),
        SequenceAction::Access(access) => Err(SequenceFailure::Transport {
            chip,
            access,
            source,
        }),
        SequenceAction::Fpga(access @ FpgaAccess::Spi { .. }) => {
            Err(SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            })
        }
        SequenceAction::Fpga(access) => Err(SequenceFailure::FpgaTransport {
            chip: fpga::NAME,
            access,
            source,
        }),
    })
}
