//! Fail-closed X.509 path construction and validation.
//!
//! ## Boundary
//!
//! This crate consumes already parsed [`rsl_x509::Certificate`] values and issuer public keys.
//! It constructs a path, verifies the exact signed bytes, checks validity, CA/path constraints,
//! critical extensions, optional key purpose and usage, and optional service identity. Root-store
//! loading, clocks, and revocation transport remain platform inputs. TLS/SSH negotiation and
//! transcript binding remain protocol-crate responsibilities.
//!
//! ## Standards ownership
//!
//! RFC 5280 §6 controls path processing. RFC 9525 §6 controls optional DNS service-identity
//! matching. Signature encodings come from RFC 5480, RFC 8410, and RFC 4055 through `rsl-x509`.
//! See `STANDARDS.md`.
//!
//! This implementation is unaudited and makes no production-security claim. It deliberately
//! implements a fail-closed profile rather than claiming the complete RFC 5280 policy tree or
//! name-constraints algorithm: unsupported critical extensions are rejected.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::fmt;
use rsl_crypto::signature::{
    ecdsa_p256::{EcdsaP256Signature, EcdsaP256VerifyingKey},
    ecdsa_p384::{EcdsaP384Signature, EcdsaP384VerifyingKey},
    ed448::{Ed448Signature, Ed448VerifyingKey},
    ed25519::{Ed25519Signature, Ed25519VerifyingKey},
    rsa_pss::{RsaPssSha256VerifyingKey, RsaPssSignature},
};
use rsl_x509::{Certificate, GeneralName, PublicKey, SignatureAlgorithm, Time, oid};

const DEFAULT_MAX_CANDIDATE_CHECKS: usize = 1_024;

/// Result type for path construction and validation.
pub type Result<T> = core::result::Result<T, Error>;

/// A public path-validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    /// Failure category.
    pub kind: ErrorKind,
}

impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

/// Path-validation failure categories.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A certificate's typed X.509 fields or extension value are invalid.
    InvalidCertificate,
    /// No supplied intermediate/anchor sequence forms a valid issuer path.
    PathNotFound,
    /// The configured maximum path depth was exceeded.
    PathTooDeep,
    /// Path construction exhausted its configured issuer-candidate work budget.
    PathSearchLimitExceeded,
    /// The issuer public key or certificate signature is malformed or does not verify.
    InvalidSignature,
    /// A well-formed signature or key algorithm is unsupported.
    UnsupportedAlgorithm,
    /// Validation time precedes `notBefore`.
    NotYetValid,
    /// Validation time follows `notAfter`.
    Expired,
    /// A non-anchor issuer lacks an affirmative CA basic constraint.
    NotCertificateAuthority,
    /// A CA key-usage extension does not permit certificate signing.
    KeyUsageViolation,
    /// A CA path-length constraint was exceeded.
    PathLengthExceeded,
    /// A critical extension is not implemented by this validation profile.
    UnsupportedCriticalExtension,
    /// Extended key usage excludes the requested purpose.
    PurposeMismatch,
    /// The requested leaf key usage is absent from an existing key-usage extension.
    RequiredKeyUsageMissing,
    /// No subject alternative name matched the requested service identity.
    ServiceIdentityMismatch,
    /// The revocation source reported the certificate revoked.
    Revoked,
    /// Policy required a definitive revocation status but none was available.
    RevocationUnknown,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PKI validation error: {:?}", self.kind)
    }
}

impl core::error::Error for Error {}

impl From<rsl_x509::Error> for Error {
    fn from(error: rsl_x509::Error) -> Self {
        let kind = if error.kind == rsl_x509::ErrorKind::UnsupportedAlgorithm {
            ErrorKind::UnsupportedAlgorithm
        } else {
            ErrorKind::InvalidCertificate
        };
        Self::new(kind)
    }
}

/// Application purpose requested from an end-entity certificate.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Purpose {
    /// Do not require a particular extended-key purpose.
    #[default]
    Any,
    /// TLS server authentication.
    ServerAuth,
    /// TLS client authentication.
    ClientAuth,
}

/// Optional leaf key-usage requirement chosen by the consuming protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequiredKeyUsage {
    /// The key may authenticate a digital signature.
    DigitalSignature,
    /// The key may encipher another key.
    KeyEncipherment,
    /// The key may perform key agreement.
    KeyAgreement,
}

/// Result supplied by a platform/application revocation source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RevocationStatus {
    /// The source has affirmative current evidence that the certificate is not revoked.
    Good,
    /// The source reports the certificate revoked.
    Revoked,
    /// No definitive status is available.
    Unknown,
}

/// Platform hook for CRL, OCSP, stapling, or a private revocation mechanism.
pub trait RevocationChecker {
    /// Returns status for `certificate` under its selected `issuer`.
    fn status(&self, certificate: &Certificate<'_>, issuer: &Certificate<'_>) -> RevocationStatus;
}

/// How unknown revocation status affects validation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum RevocationMode {
    /// Reject only an affirmative revoked status.
    #[default]
    SoftFail,
    /// Require a definitive good status for every non-anchor certificate.
    RequireKnown,
}

/// Typestate marker: no leaf supplied yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct MissingLeaf;

/// Typestate marker containing the selected leaf.
#[derive(Clone, Copy, Debug)]
pub struct HasLeaf<'cert, 'der>(&'cert Certificate<'der>);

/// Typestate marker: no trust anchors supplied yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct MissingAnchors;

/// Typestate marker containing the selected trust anchors.
#[derive(Clone, Copy, Debug)]
pub struct HasAnchors<'cert, 'der>(&'cert [Certificate<'der>]);

/// Typestate marker: no validation time supplied yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct MissingTime;

/// Typestate marker containing the validation time.
#[derive(Clone, Copy, Debug)]
pub struct HasTime(Time);

/// Entry point for the typestate path-validation builder.
#[derive(Clone, Copy, Debug, Default)]
pub struct PathValidator;

impl PathValidator {
    /// Starts a builder. `.validate()` becomes available only after setting a leaf, at least one
    /// trust-anchor slice, and a validation time.
    #[must_use]
    pub fn builder<'cert, 'der>()
    -> PathBuilder<'cert, 'der, MissingLeaf, MissingAnchors, MissingTime> {
        PathBuilder {
            leaf: MissingLeaf,
            anchors: MissingAnchors,
            time: MissingTime,
            intermediates: &[],
            purpose: Purpose::Any,
            required_key_usage: None,
            dns_name: None,
            max_depth: 8,
            max_candidate_checks: DEFAULT_MAX_CANDIDATE_CHECKS,
            revocation: None,
            revocation_mode: RevocationMode::SoftFail,
        }
    }
}

/// A path-validation configuration whose required inputs are represented by type parameters.
#[derive(Clone, Copy)]
pub struct PathBuilder<'cert, 'der, Leaf, Anchors, AtTime> {
    leaf: Leaf,
    anchors: Anchors,
    time: AtTime,
    intermediates: &'cert [Certificate<'der>],
    purpose: Purpose,
    required_key_usage: Option<RequiredKeyUsage>,
    dns_name: Option<&'cert str>,
    max_depth: usize,
    max_candidate_checks: usize,
    revocation: Option<&'cert dyn RevocationChecker>,
    revocation_mode: RevocationMode,
}

impl<'cert, 'der, Anchors, AtTime> PathBuilder<'cert, 'der, MissingLeaf, Anchors, AtTime> {
    /// Supplies the end-entity certificate and advances the leaf typestate.
    #[must_use]
    pub fn leaf(
        self,
        leaf: &'cert Certificate<'der>,
    ) -> PathBuilder<'cert, 'der, HasLeaf<'cert, 'der>, Anchors, AtTime> {
        PathBuilder {
            leaf: HasLeaf(leaf),
            anchors: self.anchors,
            time: self.time,
            intermediates: self.intermediates,
            purpose: self.purpose,
            required_key_usage: self.required_key_usage,
            dns_name: self.dns_name,
            max_depth: self.max_depth,
            max_candidate_checks: self.max_candidate_checks,
            revocation: self.revocation,
            revocation_mode: self.revocation_mode,
        }
    }
}

impl<'cert, 'der, Leaf, AtTime> PathBuilder<'cert, 'der, Leaf, MissingAnchors, AtTime> {
    /// Supplies trusted certificates and advances the trust-anchor typestate.
    #[must_use]
    pub fn trust_anchors(
        self,
        anchors: &'cert [Certificate<'der>],
    ) -> PathBuilder<'cert, 'der, Leaf, HasAnchors<'cert, 'der>, AtTime> {
        PathBuilder {
            leaf: self.leaf,
            anchors: HasAnchors(anchors),
            time: self.time,
            intermediates: self.intermediates,
            purpose: self.purpose,
            required_key_usage: self.required_key_usage,
            dns_name: self.dns_name,
            max_depth: self.max_depth,
            max_candidate_checks: self.max_candidate_checks,
            revocation: self.revocation,
            revocation_mode: self.revocation_mode,
        }
    }
}

impl<'cert, 'der, Leaf, Anchors> PathBuilder<'cert, 'der, Leaf, Anchors, MissingTime> {
    /// Supplies the caller's current UTC time and advances the time typestate.
    #[must_use]
    pub fn at_time(self, time: Time) -> PathBuilder<'cert, 'der, Leaf, Anchors, HasTime> {
        PathBuilder {
            leaf: self.leaf,
            anchors: self.anchors,
            time: HasTime(time),
            intermediates: self.intermediates,
            purpose: self.purpose,
            required_key_usage: self.required_key_usage,
            dns_name: self.dns_name,
            max_depth: self.max_depth,
            max_candidate_checks: self.max_candidate_checks,
            revocation: self.revocation,
            revocation_mode: self.revocation_mode,
        }
    }
}

impl<'cert, 'der, Leaf, Anchors, AtTime> PathBuilder<'cert, 'der, Leaf, Anchors, AtTime> {
    /// Supplies untrusted candidate intermediate certificates.
    #[must_use]
    pub fn intermediates(mut self, intermediates: &'cert [Certificate<'der>]) -> Self {
        self.intermediates = intermediates;
        self
    }

    /// Requires an extended-key purpose when the extension is present.
    #[must_use]
    pub const fn purpose(mut self, purpose: Purpose) -> Self {
        self.purpose = purpose;
        self
    }

    /// Requires one leaf key-usage bit if the leaf carries `keyUsage`.
    #[must_use]
    pub const fn required_key_usage(mut self, usage: RequiredKeyUsage) -> Self {
        self.required_key_usage = Some(usage);
        self
    }

    /// Requires a DNS-ID match in the leaf `subjectAltName`; common-name fallback is never used.
    /// The input must already be an ASCII DNS name or IDNA A-label form.
    #[must_use]
    pub fn dns_name(mut self, dns_name: &'cert str) -> Self {
        self.dns_name = Some(dns_name);
        self
    }

    /// Sets the maximum number of certificates including leaf and anchor.
    #[must_use]
    pub const fn max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Sets the maximum number of issuer candidates path construction may inspect.
    ///
    /// This bounds work independently of the number of certificates supplied by an untrusted
    /// peer. The default is 1,024 candidate checks.
    #[must_use]
    pub const fn max_candidate_checks(mut self, max_candidate_checks: usize) -> Self {
        self.max_candidate_checks = max_candidate_checks;
        self
    }

    /// Supplies revocation status and chooses whether unknown status is fatal.
    #[must_use]
    pub fn revocation(
        mut self,
        checker: &'cert dyn RevocationChecker,
        mode: RevocationMode,
    ) -> Self {
        self.revocation = Some(checker);
        self.revocation_mode = mode;
        self
    }
}

impl fmt::Debug for PathBuilder<'_, '_, MissingLeaf, MissingAnchors, MissingTime> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PathBuilder<MissingLeaf, MissingAnchors, MissingTime>")
    }
}

impl<'cert, 'der> PathBuilder<'cert, 'der, HasLeaf<'cert, 'der>, HasAnchors<'cert, 'der>, HasTime> {
    /// Constructs and validates one deterministic path. Candidate order is anchors first, then
    /// intermediates in caller order; invalid candidates do not prevent trying later candidates.
    ///
    /// # Errors
    ///
    /// No valid path exists or any configured validation rule fails.
    pub fn validate(self) -> Result<ValidatedPath<'cert, 'der>> {
        if self.anchors.0.is_empty() {
            return Err(Error::new(ErrorKind::PathNotFound));
        }
        let mut chain = vec![self.leaf.0];
        let mut remaining_candidate_checks = self.max_candidate_checks;
        if !build_path(
            self.leaf.0,
            self.intermediates,
            self.anchors.0,
            self.max_depth,
            &mut remaining_candidate_checks,
            &mut chain,
        )? {
            return Err(Error::new(ErrorKind::PathNotFound));
        }
        validate_chain(
            &chain,
            self.time.0,
            self.purpose,
            self.required_key_usage,
            self.dns_name,
            self.revocation,
            self.revocation_mode,
        )?;
        Ok(ValidatedPath {
            certificates: chain,
        })
    }
}

/// A leaf-to-anchor certificate path that passed the configured checks.
#[derive(Clone, Debug)]
pub struct ValidatedPath<'cert, 'der> {
    certificates: Vec<&'cert Certificate<'der>>,
}

impl<'cert, 'der> ValidatedPath<'cert, 'der> {
    /// Certificates from leaf through the selected trust anchor.
    #[must_use]
    pub fn certificates(&self) -> &[&'cert Certificate<'der>] {
        &self.certificates
    }

    /// Validated end-entity certificate.
    #[must_use]
    pub fn leaf(&self) -> &'cert Certificate<'der> {
        self.certificates[0]
    }

    /// Selected trust anchor.
    #[must_use]
    pub fn trust_anchor(&self) -> &'cert Certificate<'der> {
        self.certificates[self.certificates.len() - 1]
    }
}

fn build_path<'cert, 'der>(
    child: &'cert Certificate<'der>,
    intermediates: &'cert [Certificate<'der>],
    anchors: &'cert [Certificate<'der>],
    max_depth: usize,
    remaining_candidate_checks: &mut usize,
    chain: &mut Vec<&'cert Certificate<'der>>,
) -> Result<bool> {
    if chain.len() > max_depth {
        return Err(Error::new(ErrorKind::PathTooDeep));
    }
    for issuer in anchors {
        take_candidate_check(remaining_candidate_checks)?;
        if issuer.encoded() == child.encoded() {
            return Ok(true);
        }
        if issuer_matches(child, issuer)? && verify_certificate_signature(child, issuer).is_ok() {
            if chain.len() == max_depth {
                return Err(Error::new(ErrorKind::PathTooDeep));
            }
            chain.push(issuer);
            return Ok(true);
        }
    }
    for issuer in intermediates {
        take_candidate_check(remaining_candidate_checks)?;
        if chain.iter().any(|seen| seen.encoded() == issuer.encoded())
            || !issuer_matches(child, issuer)?
            || verify_certificate_signature(child, issuer).is_err()
        {
            continue;
        }
        chain.push(issuer);
        if build_path(
            issuer,
            intermediates,
            anchors,
            max_depth,
            remaining_candidate_checks,
            chain,
        )? {
            return Ok(true);
        }
        chain.pop();
    }
    Ok(false)
}

fn take_candidate_check(remaining: &mut usize) -> Result<()> {
    let Some(next) = remaining.checked_sub(1) else {
        return Err(Error::new(ErrorKind::PathSearchLimitExceeded));
    };
    *remaining = next;
    Ok(())
}

fn issuer_matches(child: &Certificate<'_>, issuer: &Certificate<'_>) -> Result<bool> {
    if child.tbs_certificate().issuer().encoded() != issuer.tbs_certificate().subject().encoded() {
        return Ok(false);
    }
    let authority = child
        .tbs_certificate()
        .extension(oid::AUTHORITY_KEY_IDENTIFIER)
        .map(rsl_x509::Extension::authority_key_identifier)
        .transpose()?
        .flatten();
    let subject = issuer
        .tbs_certificate()
        .extension(oid::SUBJECT_KEY_IDENTIFIER)
        .map(rsl_x509::Extension::subject_key_identifier)
        .transpose()?
        .flatten();
    let Some(authority) = authority else {
        return Ok(true);
    };
    if let (Some(authority_key), Some(subject_key)) = (authority.key_identifier(), subject) {
        if authority_key != subject_key {
            return Ok(false);
        }
    }
    if let Some(serial) = authority.authority_cert_serial_number() {
        if serial != issuer.tbs_certificate().serial_number() {
            return Ok(false);
        }
    }
    if let Some(names) = authority.authority_cert_issuer() {
        if !names.iter().any(|name| {
            matches!(
                name,
                GeneralName::DirectoryName(directory)
                    if directory.encoded() == issuer.tbs_certificate().issuer().encoded()
            )
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Verifies a certificate over its exact `TBSCertificate` bytes with an issuer certificate.
///
/// # Errors
///
/// Malformed keys/signatures, unsupported algorithms, or a failed signature equation.
pub fn verify_certificate_signature(
    certificate: &Certificate<'_>,
    issuer: &Certificate<'_>,
) -> Result<()> {
    let scheme = certificate.signature_algorithm().signature_algorithm()?;
    let public_key = issuer
        .tbs_certificate()
        .subject_public_key_info()
        .public_key()?;
    let message = certificate.tbs_certificate().encoded();
    match (scheme, public_key) {
        (SignatureAlgorithm::Ed25519, PublicKey::Ed25519(bytes)) => {
            let key = Ed25519VerifyingKey::try_from(bytes).map_err(|_| signature_error())?;
            let signature = Ed25519Signature::try_from(certificate.signature_value())
                .map_err(|_| signature_error())?;
            key.verify(message, &signature)
                .map_err(|_| signature_error())
        }
        (SignatureAlgorithm::Ed448, PublicKey::Ed448(bytes)) => {
            let key = Ed448VerifyingKey::try_from(bytes).map_err(|_| signature_error())?;
            let signature = Ed448Signature::try_from(certificate.signature_value())
                .map_err(|_| signature_error())?;
            key.verify(None, message, &signature)
                .map_err(|_| signature_error())
        }
        (SignatureAlgorithm::EcdsaP256Sha256, PublicKey::EcdsaP256(bytes)) => {
            let key = EcdsaP256VerifyingKey::try_from(bytes).map_err(|_| signature_error())?;
            let fixed = certificate.ecdsa_signature(32)?;
            let signature =
                EcdsaP256Signature::try_from(fixed.as_slice()).map_err(|_| signature_error())?;
            key.verify_sha256(message, &signature)
                .map_err(|_| signature_error())
        }
        (SignatureAlgorithm::EcdsaP384Sha384, PublicKey::EcdsaP384(bytes)) => {
            let key = EcdsaP384VerifyingKey::try_from(bytes).map_err(|_| signature_error())?;
            let fixed = certificate.ecdsa_signature(48)?;
            let signature =
                EcdsaP384Signature::try_from(fixed.as_slice()).map_err(|_| signature_error())?;
            key.verify_sha384(message, &signature)
                .map_err(|_| signature_error())
        }
        (SignatureAlgorithm::RsaPssSha256 { salt_len }, PublicKey::Rsa { modulus, exponent }) => {
            if issuer
                .tbs_certificate()
                .subject_public_key_info()
                .algorithm()
                .oid()
                .is(oid::RSASSA_PSS)
                && issuer
                    .tbs_certificate()
                    .subject_public_key_info()
                    .algorithm()
                    .signature_algorithm()?
                    != scheme
            {
                return Err(Error::new(ErrorKind::UnsupportedAlgorithm));
            }
            let key = RsaPssSha256VerifyingKey::from_components(modulus, exponent)
                .map_err(|_| signature_error())?;
            let signature = RsaPssSignature::from_bytes(certificate.signature_value().to_vec());
            key.verify_sha256_with_salt_len(message, &signature, salt_len)
                .map_err(|_| signature_error())
        }
        _ => Err(Error::new(ErrorKind::UnsupportedAlgorithm)),
    }
}

fn signature_error() -> Error {
    Error::new(ErrorKind::InvalidSignature)
}

#[allow(clippy::too_many_arguments)]
fn validate_chain(
    chain: &[&Certificate<'_>],
    time: Time,
    purpose: Purpose,
    required_key_usage: Option<RequiredKeyUsage>,
    dns_name: Option<&str>,
    revocation: Option<&dyn RevocationChecker>,
    revocation_mode: RevocationMode,
) -> Result<()> {
    let certificates_to_validate = if chain.len() == 1 {
        chain
    } else {
        &chain[..chain.len() - 1]
    };
    for certificate in certificates_to_validate {
        validate_certificate_profile(certificate, time, purpose)?;
    }
    validate_leaf(chain[0], required_key_usage, dns_name)?;
    for index in 1..chain.len() - 1 {
        validate_ca(chain[index])?;
        if let Some(limit) = basic_constraints(chain[index])?.path_len {
            let ca_below = chain[1..index]
                .iter()
                .filter(|certificate| !is_self_issued(certificate))
                .count();
            if ca_below > limit as usize {
                return Err(Error::new(ErrorKind::PathLengthExceeded));
            }
        }
    }
    if let Some(checker) = revocation {
        for pair in chain.windows(2) {
            match checker.status(pair[0], pair[1]) {
                RevocationStatus::Revoked => return Err(Error::new(ErrorKind::Revoked)),
                RevocationStatus::Unknown if revocation_mode == RevocationMode::RequireKnown => {
                    return Err(Error::new(ErrorKind::RevocationUnknown));
                }
                RevocationStatus::Good | RevocationStatus::Unknown => {}
            }
        }
    } else if revocation_mode == RevocationMode::RequireKnown {
        return Err(Error::new(ErrorKind::RevocationUnknown));
    }
    Ok(())
}

fn validate_certificate_profile(
    certificate: &Certificate<'_>,
    time: Time,
    purpose: Purpose,
) -> Result<()> {
    let validity = certificate.tbs_certificate().validity();
    if time < validity.not_before {
        return Err(Error::new(ErrorKind::NotYetValid));
    }
    if time > validity.not_after {
        return Err(Error::new(ErrorKind::Expired));
    }
    for extension in certificate.tbs_certificate().extensions() {
        if extension.critical() && !extension.is_supported_critical() {
            return Err(Error::new(ErrorKind::UnsupportedCriticalExtension));
        }
        if extension.oid().is(oid::BASIC_CONSTRAINTS) {
            extension.basic_constraints()?;
        } else if extension.oid().is(oid::KEY_USAGE) {
            extension.key_usage()?;
        } else if extension.oid().is(oid::SUBJECT_ALT_NAME) {
            let names = extension.subject_alt_names()?;
            if extension.critical()
                && names.is_some_and(|names| {
                    names
                        .iter()
                        .any(|name| matches!(name, GeneralName::Other(_)))
                })
            {
                return Err(Error::new(ErrorKind::UnsupportedCriticalExtension));
            }
        } else if extension.oid().is(oid::EXTENDED_KEY_USAGE) {
            extension.extended_key_usage()?;
        } else if extension.oid().is(oid::SUBJECT_KEY_IDENTIFIER) {
            extension.subject_key_identifier()?;
        } else if extension.oid().is(oid::AUTHORITY_KEY_IDENTIFIER) {
            extension.authority_key_identifier()?;
        }
    }
    if certificate.tbs_certificate().subject().is_empty() {
        let san = certificate
            .tbs_certificate()
            .extension(oid::SUBJECT_ALT_NAME)
            .ok_or_else(|| Error::new(ErrorKind::InvalidCertificate))?;
        if !san.critical() {
            return Err(Error::new(ErrorKind::InvalidCertificate));
        }
    }
    validate_purpose(certificate, purpose)
}

fn validate_purpose(certificate: &Certificate<'_>, purpose: Purpose) -> Result<()> {
    let expected = match purpose {
        Purpose::Any => return Ok(()),
        Purpose::ServerAuth => oid::SERVER_AUTH,
        Purpose::ClientAuth => oid::CLIENT_AUTH,
    };
    if let Some(extension) = certificate
        .tbs_certificate()
        .extension(oid::EXTENDED_KEY_USAGE)
    {
        let purposes = extension
            .extended_key_usage()?
            .ok_or_else(|| Error::new(ErrorKind::InvalidCertificate))?;
        if !purposes
            .iter()
            .any(|purpose| purpose.is(expected) || purpose.is(oid::ANY_EXTENDED_KEY_USAGE))
        {
            return Err(Error::new(ErrorKind::PurposeMismatch));
        }
    }
    Ok(())
}

fn validate_leaf(
    certificate: &Certificate<'_>,
    required_key_usage: Option<RequiredKeyUsage>,
    dns_name: Option<&str>,
) -> Result<()> {
    if let (Some(required), Some(extension)) = (
        required_key_usage,
        certificate.tbs_certificate().extension(oid::KEY_USAGE),
    ) {
        let usage = extension
            .key_usage()?
            .ok_or_else(|| Error::new(ErrorKind::InvalidCertificate))?;
        let permitted = match required {
            RequiredKeyUsage::DigitalSignature => usage.digital_signature(),
            RequiredKeyUsage::KeyEncipherment => usage.key_encipherment(),
            RequiredKeyUsage::KeyAgreement => usage.key_agreement(),
        };
        if !permitted {
            return Err(Error::new(ErrorKind::RequiredKeyUsageMissing));
        }
    }
    if let Some(reference) = dns_name {
        validate_dns_name(certificate, reference)?;
    }
    Ok(())
}

fn validate_ca(certificate: &Certificate<'_>) -> Result<()> {
    if !basic_constraints(certificate)?.ca {
        return Err(Error::new(ErrorKind::NotCertificateAuthority));
    }
    if let Some(extension) = certificate.tbs_certificate().extension(oid::KEY_USAGE) {
        let usage = extension
            .key_usage()?
            .ok_or_else(|| Error::new(ErrorKind::InvalidCertificate))?;
        if !usage.key_cert_sign() {
            return Err(Error::new(ErrorKind::KeyUsageViolation));
        }
    }
    Ok(())
}

fn basic_constraints(certificate: &Certificate<'_>) -> Result<rsl_x509::BasicConstraints> {
    certificate
        .tbs_certificate()
        .extension(oid::BASIC_CONSTRAINTS)
        .ok_or_else(|| Error::new(ErrorKind::NotCertificateAuthority))?
        .basic_constraints()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidCertificate))
}

fn is_self_issued(certificate: &Certificate<'_>) -> bool {
    certificate.tbs_certificate().issuer().encoded()
        == certificate.tbs_certificate().subject().encoded()
}

fn validate_dns_name(certificate: &Certificate<'_>, reference: &str) -> Result<()> {
    if !valid_reference_dns_name(reference) {
        return Err(Error::new(ErrorKind::ServiceIdentityMismatch));
    }
    let extension = certificate
        .tbs_certificate()
        .extension(oid::SUBJECT_ALT_NAME)
        .ok_or_else(|| Error::new(ErrorKind::ServiceIdentityMismatch))?;
    let names = extension
        .subject_alt_names()?
        .ok_or_else(|| Error::new(ErrorKind::ServiceIdentityMismatch))?;
    if names.iter().any(
        |name| matches!(name, GeneralName::DnsName(presented) if dns_matches(reference, presented)),
    ) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::ServiceIdentityMismatch))
    }
}

fn valid_reference_dns_name(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    !name.is_empty()
        && name.is_ascii()
        && name.len() <= 253
        && !name.contains('*')
        && name.split('.').all(valid_dns_label)
}

fn dns_matches(reference: &str, presented: &str) -> bool {
    let reference = reference.strip_suffix('.').unwrap_or(reference);
    let presented = presented.strip_suffix('.').unwrap_or(presented);
    if !presented.is_ascii() || presented.is_empty() || presented.len() > 253 {
        return false;
    }
    if !presented.contains('*') {
        return presented.split('.').all(valid_dns_label)
            && reference.eq_ignore_ascii_case(presented);
    }
    let Some(suffix) = presented.strip_prefix("*.") else {
        return false;
    };
    if suffix.is_empty() || suffix.contains('*') || !suffix.split('.').all(valid_dns_label) {
        return false;
    }
    let Some((first_label, reference_suffix)) = reference.split_once('.') else {
        return false;
    };
    !first_label.is_empty() && reference_suffix.eq_ignore_ascii_case(suffix)
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use rsl_asn1::{Encoder, ObjectIdentifier, Tag};
    use rsl_crypto::signature::ed25519::Ed25519SigningKey;

    fn built(build: impl FnOnce(&mut Encoder) -> rsl_asn1::Result<()>) -> Vec<u8> {
        let mut encoder = Encoder::new();
        build(&mut encoder).unwrap();
        encoder.finish()
    }

    fn object_identifier(arcs: &[u64]) -> ObjectIdentifier {
        ObjectIdentifier::from_arcs(arcs).unwrap()
    }

    fn algorithm() -> Vec<u8> {
        built(|encoder| {
            encoder
                .sequence(|sequence| sequence.object_identifier(&object_identifier(oid::ED25519)))
        })
    }

    fn name(common_name: &str) -> Vec<u8> {
        let attribute = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&object_identifier(rsl_x509::oid::COMMON_NAME))?;
                sequence.element(Tag::UTF8_STRING, common_name.as_bytes())
            })
        });
        let set = built(|encoder| encoder.element(Tag::SET, &attribute));
        built(|encoder| encoder.element(Tag::SEQUENCE, &set))
    }

    fn extension(arcs: &[u64], critical: bool, value: &[u8]) -> Vec<u8> {
        built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&object_identifier(arcs))?;
                if critical {
                    sequence.boolean(true)?;
                }
                sequence.octet_string(value)
            })
        })
    }

    fn certificate(
        subject_name: &str,
        issuer_name: &str,
        public_key: &[u8; 32],
        issuer_key: &Ed25519SigningKey,
        ca: bool,
        dns_name: Option<&str>,
        extra_extensions: &[Vec<u8>],
    ) -> Vec<u8> {
        let signature_algorithm = algorithm();
        let version = built(|encoder| encoder.unsigned_integer(&[2]));
        let validity = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::UTC_TIME, b"260101000000Z")?;
                sequence.element(Tag::UTC_TIME, b"270101000000Z")
            })
        });
        let spki = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.encoded(&algorithm())?;
                sequence.bit_string(0, public_key)
            })
        });
        let mut extensions = Vec::new();
        if ca {
            let basic = built(|encoder| encoder.sequence(|sequence| sequence.boolean(true)));
            extensions.push(extension(oid::BASIC_CONSTRAINTS, true, &basic));
            let usage = built(|encoder| encoder.bit_string(2, &[0x04]));
            extensions.push(extension(oid::KEY_USAGE, true, &usage));
        } else {
            let usage = built(|encoder| encoder.bit_string(7, &[0x80]));
            extensions.push(extension(oid::KEY_USAGE, true, &usage));
            let eku = built(|encoder| {
                encoder.sequence(|sequence| {
                    sequence.object_identifier(&object_identifier(oid::SERVER_AUTH))
                })
            });
            extensions.push(extension(oid::EXTENDED_KEY_USAGE, false, &eku));
            if let Some(dns_name) = dns_name {
                let san = built(|encoder| {
                    encoder.sequence(|sequence| {
                        sequence.element(Tag::context(2, false), dns_name.as_bytes())
                    })
                });
                extensions.push(extension(oid::SUBJECT_ALT_NAME, false, &san));
            }
        }
        extensions.extend_from_slice(extra_extensions);
        let extension_sequence = built(|encoder| {
            encoder.sequence(|sequence| {
                for extension in &extensions {
                    sequence.encoded(extension)?;
                }
                Ok(())
            })
        });
        let tbs = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::context(0, true), &version)?;
                sequence.unsigned_integer(&[1])?;
                sequence.encoded(&signature_algorithm)?;
                sequence.encoded(&name(issuer_name))?;
                sequence.encoded(&validity)?;
                sequence.encoded(&name(subject_name))?;
                sequence.encoded(&spki)?;
                sequence.element(Tag::context(3, true), &extension_sequence)
            })
        });
        let signature = issuer_key.sign(&tbs).unwrap();
        built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.encoded(&tbs)?;
                sequence.encoded(&signature_algorithm)?;
                sequence.bit_string(0, signature.as_bytes())
            })
        })
    }

    #[test]
    fn standard_derived_ed25519_path_validates_with_typestate_inputs() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
        let root_der = certificate(
            "Test Root",
            "Test Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let leaf_der = certificate(
            "example.test",
            "Test Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            Some("example.test"),
            &[],
        );
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let anchors = vec![Certificate::from_der(&root_der).unwrap()];

        let path = PathValidator::builder()
            .leaf(&leaf)
            .trust_anchors(&anchors)
            .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
            .purpose(Purpose::ServerAuth)
            .required_key_usage(RequiredKeyUsage::DigitalSignature)
            .dns_name("EXAMPLE.TEST")
            .validate()
            .unwrap();
        assert_eq!(path.certificates().len(), 2);
        assert_eq!(path.leaf().encoded(), leaf_der);
        assert_eq!(path.trust_anchor().encoded(), root_der);
    }

    #[test]
    fn negative_dns_wildcard_is_one_leftmost_label_only() {
        assert!(dns_matches("www.example.test", "*.example.test"));
        assert!(!dns_matches("deep.www.example.test", "*.example.test"));
        assert!(!dns_matches("www.example.test", "w*.example.test"));
        assert!(!dns_matches("example.test", "*.example.test"));
    }

    #[test]
    fn negative_tampered_signature_fails() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
        let root_der = certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let mut leaf_der = certificate(
            "Leaf",
            "Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            Some("leaf.test"),
            &[],
        );
        *leaf_der.last_mut().unwrap() ^= 1;
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let root = Certificate::from_der(&root_der).unwrap();
        assert_eq!(
            verify_certificate_signature(&leaf, &root).unwrap_err().kind,
            ErrorKind::InvalidSignature
        );
    }

    #[test]
    fn negative_unknown_critical_extension_fails_closed() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
        let root_der = certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let unknown_value = built(Encoder::null);
        let unknown = extension(&[1, 2, 3, 4], true, &unknown_value);
        let leaf_der = certificate(
            "Leaf",
            "Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            None,
            &[unknown],
        );
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let anchors = [Certificate::from_der(&root_der).unwrap()];
        assert_eq!(
            PathValidator::builder()
                .leaf(&leaf)
                .trust_anchors(&anchors)
                .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::UnsupportedCriticalExtension
        );
    }

    #[test]
    fn negative_policy_inputs_fail_closed() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
        let root_der = certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let leaf_der = certificate(
            "Leaf",
            "Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            Some("leaf.test"),
            &[],
        );
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let anchors = [Certificate::from_der(&root_der).unwrap()];
        let validation = |time| {
            PathValidator::builder()
                .leaf(&leaf)
                .trust_anchors(&anchors)
                .at_time(time)
        };

        assert_eq!(
            validation(Time::new(2025, 12, 31, 23, 59, 59).unwrap())
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::NotYetValid
        );
        assert_eq!(
            validation(Time::new(2027, 1, 1, 0, 0, 1).unwrap())
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::Expired
        );
        assert_eq!(
            validation(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .purpose(Purpose::ClientAuth)
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::PurposeMismatch
        );
        assert_eq!(
            validation(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .required_key_usage(RequiredKeyUsage::KeyAgreement)
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::RequiredKeyUsageMissing
        );
        assert_eq!(
            validation(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .dns_name("other.test")
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::ServiceIdentityMismatch
        );
        assert_eq!(
            validation(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .max_depth(1)
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::PathTooDeep
        );
    }

    #[test]
    fn negative_non_ca_intermediate_is_rejected() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let intermediate_key = Ed25519SigningKey::from_seed([2; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([3; 32]);
        let root_der = certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let intermediate_der = certificate(
            "Intermediate",
            "Root",
            intermediate_key.verifying_key().as_bytes(),
            &root_key,
            false,
            None,
            &[],
        );
        let leaf_der = certificate(
            "Leaf",
            "Intermediate",
            leaf_key.verifying_key().as_bytes(),
            &intermediate_key,
            false,
            Some("leaf.test"),
            &[],
        );
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let intermediates = [Certificate::from_der(&intermediate_der).unwrap()];
        let anchors = [Certificate::from_der(&root_der).unwrap()];

        assert_eq!(
            PathValidator::builder()
                .leaf(&leaf)
                .intermediates(&intermediates)
                .trust_anchors(&anchors)
                .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::NotCertificateAuthority
        );
    }

    #[test]
    fn candidate_check_budget_bounds_path_search() {
        let root_key = Ed25519SigningKey::from_seed([1; 32]);
        let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
        let decoy_key = Ed25519SigningKey::from_seed([3; 32]);
        let root_der = certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
            &[],
        );
        let decoy_der = certificate(
            "Root",
            "Root",
            decoy_key.verifying_key().as_bytes(),
            &decoy_key,
            true,
            None,
            &[],
        );
        let leaf_der = certificate(
            "Leaf",
            "Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            None,
            &[],
        );
        let leaf = Certificate::from_der(&leaf_der).unwrap();
        let anchors = [
            Certificate::from_der(&decoy_der).unwrap(),
            Certificate::from_der(&root_der).unwrap(),
        ];

        assert_eq!(
            PathValidator::builder()
                .leaf(&leaf)
                .trust_anchors(&anchors)
                .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .max_candidate_checks(1)
                .validate()
                .unwrap_err()
                .kind,
            ErrorKind::PathSearchLimitExceeded
        );
        assert!(
            PathValidator::builder()
                .leaf(&leaf)
                .trust_anchors(&anchors)
                .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
                .max_candidate_checks(2)
                .validate()
                .is_ok()
        );
    }
}
