//! The choices a login leaves to its caller.

/// How a login behaves where the protocol leaves a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    /// Whether a failed anti-MITM check stops the login. The vendor's clients carry on and
    /// only report it (spec §5.3), which is the default here: **by default the password is
    /// sent to a gateway that failed its checks**, and the failure is in
    /// [`Login::anti_mitm`](super::Login::anti_mitm).
    ///
    /// Set this to stop instead: nothing is sent after a failed check, and the cookies the
    /// failing answers set are dropped. It stops on a check that fails, not on one that is
    /// not made: an answer with no anti-MITM object, or one that does not enable the
    /// certificate check, still gets the password. Both are in `Login::anti_mitm` to act on.
    pub strict_anti_mitm: bool,
    /// How many requests of the secondary chain are made before giving up. A login needs at
    /// least one, so zero fails every login after its password is accepted.
    pub max_steps: usize,
}

impl Default for Options {
    /// Anti-MITM failures are reported, not fatal, as in the vendor's clients, and the chain
    /// is followed for eight requests, about the reference client's bound.
    fn default() -> Self {
        Self {
            strict_anti_mitm: false,
            max_steps: 8,
        }
    }
}
