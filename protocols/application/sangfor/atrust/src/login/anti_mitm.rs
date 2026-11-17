//! What the anti-MITM checks found.

/// The outcome of the checks a client makes on an `authConfig` answer (spec §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AntiMitm {
    /// The answer carried no anti-MITM object, so nothing was checked. A peer in the middle
    /// can produce this by leaving the object out.
    Absent,
    /// Every check that applied passed. When the object does not enable the certificate
    /// check, that is the challenge and the signature only, both of which depend on nothing
    /// but the object and a constant, so a peer in the middle can produce them too.
    Passed,
    /// This check failed. Those after it were not made.
    Failed(Check),
}

/// One of the anti-MITM checks, in the order they are made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Check {
    /// The gateway's answer to its own challenge (spec §3.3).
    Challenge,
    /// The signature over the anti-MITM object (spec §3.3).
    Signature,
    /// The certificate of the connection against the ones the gateway names (spec §3.5).
    Certificate,
    /// The second-stage request and its signed answer (spec §3.6).
    SecondStage,
}
