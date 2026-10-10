use modular_bitfield_msb::prelude::*;

/// Encodes the operation being performed, and indicates whether the message
/// is a request or a response.
///
/// The OpCode structure is defined [here](https://www.rfc-editor.org/rfc/pdfrfc/rfc1002.txt.pdf).
/// ```text
///   0   1   2   3   4
/// +---+---+---+---+---+
/// | R |     OPCODE    |
/// +---+---+---+---+---+
/// ```
///
/// Where `R` is used to indicate whether this is a request or a response, and
/// the `OPCODE` is the operation being performed.
///
/// In this library the [Op](crate::name::codes::Op) enum is used to interact with the operation. The RFC defined opcodes are as follows:
///
/// | Opcode | Name | Description |
/// |------|----|-----------|
/// | 0 | Query | Indicates a query. |
/// | 5 | Registration | Indicates a registration. |
/// | 6 | Release | Indicates a release. |
/// | 7 | WACK | Indicates a WACK. |
/// | 8 | Refresh | Indicates a refresh. |
/// | 9 | AltRefresh | Alternate value for refresh (due to typo/conflict in RFC - see section 4.2.1.1 and section 4.2.4 for further details). |
/// | 15 | MultiHomedRegistration | Indicates a multi-homed registration - added after RFC 1002. |
///
#[bitfield(filled = false)]
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
pub struct OpCode {
    /// Indicates if the message is a request or a response.
    pub response: bool,

    /// Indicates the operation being performed.
    pub op: Op,
}

/// Inidcates the operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, BitfieldSpecifier)]
#[bits = 4]
pub enum Op {
    /// Indicates a query.
    Query = 0,

    /// Indicates a registration.
    Registration = 5,

    /// Indicates a release.
    Release = 6,

    /// Indicates a WACK.
    Wack = 7,

    /// Indicates a refresh.
    Refresh = 8,

    /// Alternate value for refresh (due to typo/conflict in RFC - see section 4.2.1.1 and section
    /// 4.2.4 for further details).
    AltRefresh = 9,

    // The following are not part of the RFC, but are available to the consumer of this library for custom use.
    Custom1 = 1,
    Custom2 = 2,
    Custom3 = 3,
    Custom4 = 4,
    Custom10 = 10,
    Custom11 = 11,
    Custom12 = 12,
    Custom13 = 13,
    Custom14 = 14,

    #[cfg(not(feature = "nbte"))]
    Custom15 = 15,

    #[cfg(feature = "nbte")]
    /// Indicates a multi-homed registration.
    MultiHomedRegistration = 15,
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn opcodes() {
        let mut opcode = OpCode::new();

        opcode.set_op(Op::Custom3);

        assert_eq!(opcode.op(), Op::Custom3);
    }

    #[test]
    #[cfg(feature = "nbte")]
    fn multi_homed() {
        let mut opcode = OpCode::new();

        opcode.set_op(Op::MultiHomedRegistration);

        assert_eq!(opcode.op(), Op::MultiHomedRegistration);
    }

    #[test]
    #[cfg(not(feature = "nbte"))]
    fn opcode15() {
        let mut opcode = OpCode::new();

        opcode.set_op(Op::Custom15);

        assert_eq!(opcode.op(), Op::Custom15);
    }
}
