use modular_bitfield::prelude::*;

/// Available flags for a NetBIOS Name Service (NBNS) packet.
///
/// The flags are defined as follows:
/// ```text
///   0   1   2   3   4   5   6
/// +---+---+---+---+---+---+---+
/// |AA |TC |RD |RA | 0 | 0 | B |
/// +---+---+---+---+---+---+---+
/// ```
///
///
/// |Symbol| Description|
/// |------|------------|
/// |AA    | **Authoritative Answer:** <br> Must be zero (0) if `R` flag of `OPCODE` is zero(0). <br><br> If `R` flag is one (1) then if `AA` is one (1) then the node responding is an authority for the domain name. <br><br> End nodes responding to queries always set this bit in responses. |
/// |TC    | **Truncated:** <br> Set if this message was truncated because the datagram carrying it would be greater than 576 bytes in length. Use TCP to get the information from the NetBIOS Name Server.|
/// |RD    | **Recursion Desired:** <br> May only be set on a request to a NetBIOS Name Server. <br><br> The NBNS will copy its state into the response packet. <br><br> If one (1) the NBNS will iterate on the query, registration, or release.|
/// |RA    | **Recursion Available:** <br> Only valid in responses from a NetBIOS Name Server -- must be zero in all other responses. <br><br> If one (1) then the NBNS supports recursive query, registration, and release. <br><br> If zero (0) then the end-node must iterate for query and challenge for registration.|
/// |B     | **Broadcast:** <br> = 1: packet was broadcast or multicast <br> = 0: unicast|
/// |0     | **Reserved:** <br> These bits are not utilized by the RFC. The library however does expose these bits to be manipulated.|
#[bitfield(filled = false)]
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
pub struct Flags {
    /// Indicates if the response is authoritative.
    pub authoritative: bool,

    /// Indicates if the response is truncated.
    pub truncated: bool,

    /// Indicates if the response desires recursion.
    pub recursion_desired: bool,

    /// Indicates if the response has recursion available.
    pub recursion_available: bool,

    /// The field is reserved but can be set by the user to other values.
    pub reserved: B2,

    /// Indicates if the response is broadcast.
    pub broadcast: bool,
}

#[cfg(test)]
mod unit {
    use super::*;
    use modular_bitfield::prelude::B2;

    #[test]
    fn flags() {
        let mut flags = Flags::new();

        flags.set_authoritative(true);
        flags.set_truncated(true);
        flags.set_recursion_desired(true);
        flags.set_recursion_available(true);
        flags.set_broadcast(true);
        flags.set_reserved(B2::from_bytes(0x03).unwrap());

        println!("{:?}", flags);
    }
}
