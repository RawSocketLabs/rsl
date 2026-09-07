use bnb::bin;
use thiserror::Error;

/// The subnegotiation version byte used by RFC 1929 username/password messages.
pub const USERNAME_PASSWORD_VERSION: u8 = 1;
const MAX_LEN: usize = u8::MAX as usize;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
enum ValidationError {
    #[error("username must contain at least one byte")]
    EmptyUsername,
    #[error("username length {user} exceeds {MAX_LEN}")]
    UsernameTooLong { user: usize },
    #[error("password must contain at least one byte")]
    EmptyPassword,
    #[error("password length {pass} exceeds {MAX_LEN}")]
    PasswordTooLong { pass: usize },
}

/// A username/password authentication request: `VER`, `ULEN`, `UNAME`, `PLEN`, `PASSWD`
/// (RFC 1929 §2).
///
/// Credentials remain raw bytes so decoding never loses non-UTF-8 input. The builder requires
/// both fields to contain between 1 and 255 bytes; decoding remains permissive, while both
/// encoded lengths are derived and overflow-checked.
//~ models rfc1929#2 part="username/password request"
#[bin(big, validate = validate_username_password_request)]
#[derive(Clone, PartialEq, Eq)]
pub struct UsernamePasswordRequest {
    /// The subnegotiation version. The builder defaults to [`USERNAME_PASSWORD_VERSION`].
    #[reserved_with(USERNAME_PASSWORD_VERSION)]
    pub version: u8,
    /// The username bytes.
    #[brw(count_prefix = u8)]
    pub username: Vec<u8>,
    /// The password bytes.
    #[brw(count_prefix = u8)]
    pub password: Vec<u8>,
}

fn validate_username_password_request(
    request: &UsernamePasswordRequest,
) -> Result<(), ValidationError> {
    match (request.username.len(), request.password.len()) {
        (0, _) => Err(ValidationError::EmptyUsername),
        (_, 0) => Err(ValidationError::EmptyPassword),
        (user, _) if user > MAX_LEN => Err(ValidationError::UsernameTooLong { user }),
        (_, pass) if pass > MAX_LEN => Err(ValidationError::PasswordTooLong { pass }),
        _ => Ok(()),
    }
}

/// A username/password authentication status byte (RFC 1929 §2).
///
/// Zero indicates success; every nonzero value indicates failure. The byte-backed model makes
/// the contradictory `Failure(0)` state unrepresentable while preserving every wire value.
//~ models rfc1929#2 registry="STATUS"
#[bin(wire = u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UsernamePasswordStatus(u8);

impl UsernamePasswordStatus {
    /// Authentication succeeded (`0x00`).
    pub const SUCCESS: Self = Self(0);

    /// Returns the preserved wire status byte.
    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }

    /// Returns whether authentication succeeded.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 == 0
    }

    /// Returns whether authentication failed.
    #[must_use]
    pub const fn is_failure(self) -> bool {
        !self.is_success()
    }
}

impl From<u8> for UsernamePasswordStatus {
    fn from(code: u8) -> Self {
        Self(code)
    }
}

impl From<UsernamePasswordStatus> for u8 {
    fn from(status: UsernamePasswordStatus) -> Self {
        status.code()
    }
}

impl From<&UsernamePasswordStatus> for u8 {
    fn from(status: &UsernamePasswordStatus) -> Self {
        status.code()
    }
}

impl bnb::FixedBitLen for UsernamePasswordStatus {
    const BIT_LEN: u32 = u8::BITS;
}

/// A username/password authentication response: `VER`, `STATUS` (RFC 1929 §2).
//~ models rfc1929#2 part="username/password response"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsernamePasswordResponse {
    /// The subnegotiation version. The builder defaults to [`USERNAME_PASSWORD_VERSION`].
    #[reserved_with(USERNAME_PASSWORD_VERSION)]
    pub version: u8,
    /// Zero for success; any nonzero value indicates failure.
    pub status: UsernamePasswordStatus,
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn status_classifies_and_preserves_every_byte() {
        assert!(UsernamePasswordStatus::from(0).is_success());
        for code in 0u8..=u8::MAX {
            let status = UsernamePasswordStatus::from(code);
            assert_eq!(status.code(), code, "status {code:#04x}");
            assert_eq!(status.is_failure(), code != 0, "status {code:#04x}");
        }
    }
}
