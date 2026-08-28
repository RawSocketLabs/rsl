//! Borrowed X.509 certificate structures and canonical construction.
//!
//! ## Standards ownership
//!
//! RFC 5280 §4 controls certificate syntax, names, validity, `SubjectPublicKeyInfo`, and the
//! implemented extensions. RFC 5480 owns NIST-curve public-key identifiers. RFC 8410 owns
//! Ed25519/Ed448 identifiers. RFC 4055 owns RSASSA-PSS parameters. See `STANDARDS.md`.
//!
//! Parsing is strict DER through `rsl-asn1`. [`TbsCertificate::encoded`] returns the original
//! complete DER element that an issuer signed; callers must verify those bytes directly.
//! [`builder::CertificateBuilder`] constructs new V3 certificates through mandatory validity and
//! public-key typestates. Construction yields syntax, never trusted state; trust decisions remain
//! in `rsl-pki`.
//!
//! This implementation is unaudited and makes no production-security claim.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

/// Canonical X.509 certificate construction and signing contracts.
pub mod builder;

use alloc::{vec, vec::Vec};
use core::{fmt, str};
use rsl_asn1::{BitString, Class, Decoder, Element, ObjectIdentifier, Tag};

/// Common object identifiers used by the supported X.509 profile.
pub mod oid {
    /// `commonName`.
    pub const COMMON_NAME: &[u64] = &[2, 5, 4, 3];
    /// `subjectKeyIdentifier`.
    pub const SUBJECT_KEY_IDENTIFIER: &[u64] = &[2, 5, 29, 14];
    /// `keyUsage`.
    pub const KEY_USAGE: &[u64] = &[2, 5, 29, 15];
    /// `subjectAltName`.
    pub const SUBJECT_ALT_NAME: &[u64] = &[2, 5, 29, 17];
    /// `basicConstraints`.
    pub const BASIC_CONSTRAINTS: &[u64] = &[2, 5, 29, 19];
    /// `nameConstraints`.
    pub const NAME_CONSTRAINTS: &[u64] = &[2, 5, 29, 30];
    /// `certificatePolicies`.
    pub const CERTIFICATE_POLICIES: &[u64] = &[2, 5, 29, 32];
    /// `authorityKeyIdentifier`.
    pub const AUTHORITY_KEY_IDENTIFIER: &[u64] = &[2, 5, 29, 35];
    /// `extendedKeyUsage`.
    pub const EXTENDED_KEY_USAGE: &[u64] = &[2, 5, 29, 37];
    /// TLS server authentication extended-key purpose.
    pub const SERVER_AUTH: &[u64] = &[1, 3, 6, 1, 5, 5, 7, 3, 1];
    /// TLS client authentication extended-key purpose.
    pub const CLIENT_AUTH: &[u64] = &[1, 3, 6, 1, 5, 5, 7, 3, 2];
    /// `anyExtendedKeyUsage`.
    pub const ANY_EXTENDED_KEY_USAGE: &[u64] = &[2, 5, 29, 37, 0];
    /// RSA public-key algorithm.
    pub const RSA_ENCRYPTION: &[u64] = &[1, 2, 840, 113_549, 1, 1, 1];
    /// RSASSA-PSS signature and public-key algorithm.
    pub const RSASSA_PSS: &[u64] = &[1, 2, 840, 113_549, 1, 1, 10];
    /// MGF1.
    pub const MGF1: &[u64] = &[1, 2, 840, 113_549, 1, 1, 8];
    /// SHA-256.
    pub const SHA256: &[u64] = &[2, 16, 840, 1, 101, 3, 4, 2, 1];
    /// ECDSA with SHA-256.
    pub const ECDSA_WITH_SHA256: &[u64] = &[1, 2, 840, 10045, 4, 3, 2];
    /// ECDSA with SHA-384.
    pub const ECDSA_WITH_SHA384: &[u64] = &[1, 2, 840, 10045, 4, 3, 3];
    /// Generic elliptic-curve public key.
    pub const EC_PUBLIC_KEY: &[u64] = &[1, 2, 840, 10045, 2, 1];
    /// NIST P-256 named curve.
    pub const SECP256R1: &[u64] = &[1, 2, 840, 10045, 3, 1, 7];
    /// NIST P-384 named curve.
    pub const SECP384R1: &[u64] = &[1, 3, 132, 0, 34];
    /// Ed25519 public key and signature.
    pub const ED25519: &[u64] = &[1, 3, 101, 112];
    /// Ed448 public key and signature.
    pub const ED448: &[u64] = &[1, 3, 101, 113];
}

/// Result type for X.509 parsing and schema interpretation.
pub type Result<T> = core::result::Result<T, Error>;

/// An X.509 parse or profile error.
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

/// X.509 failure categories.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The underlying input is not strict DER.
    Der,
    /// A required field was absent, duplicated, out of order, or had the wrong tag.
    InvalidStructure,
    /// The certificate version is unsupported or inconsistent with optional fields.
    InvalidVersion,
    /// The serial number violates the RFC 5280 profile.
    InvalidSerialNumber,
    /// An algorithm identifier or its parameters are malformed.
    InvalidAlgorithmIdentifier,
    /// The algorithm is well-formed but outside the supported profile.
    UnsupportedAlgorithm,
    /// A validity time is malformed or names an impossible calendar date.
    InvalidTime,
    /// A distinguished name is malformed.
    InvalidName,
    /// A public key encoding is malformed.
    InvalidPublicKey,
    /// An extension is duplicated or malformed.
    InvalidExtension,
    /// The outer and `TBSCertificate` signature identifiers differ.
    SignatureAlgorithmMismatch,
    /// A signature bit string or ECDSA signature value is malformed.
    InvalidSignature,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "X.509 error: {:?}", self.kind)
    }
}

impl core::error::Error for Error {}

impl From<rsl_asn1::Error> for Error {
    fn from(_: rsl_asn1::Error) -> Self {
        Self::new(ErrorKind::Der)
    }
}

fn structure<T>() -> Result<T> {
    Err(Error::new(ErrorKind::InvalidStructure))
}

fn sequence(element: Element<'_>) -> Result<Decoder<'_>> {
    Ok(element.expect(Tag::SEQUENCE)?.children()?)
}

fn collect<'a>(decoder: &mut Decoder<'a>) -> Result<Vec<Element<'a>>> {
    let mut elements = Vec::new();
    while !decoder.is_finished() {
        elements.push(decoder.read()?);
    }
    Ok(elements)
}

/// X.509 certificate version.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Version {
    /// Version 1, encoded by an absent default field.
    V1,
    /// Version 2.
    V2,
    /// Version 3.
    V3,
}

/// An RFC 5280 `AlgorithmIdentifier` with its exact parameter encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlgorithmIdentifier<'a> {
    oid: ObjectIdentifier,
    parameters: Option<Element<'a>>,
    encoded: &'a [u8],
}

impl<'a> AlgorithmIdentifier<'a> {
    /// Algorithm object identifier.
    #[must_use]
    pub fn oid(&self) -> &ObjectIdentifier {
        &self.oid
    }

    /// Optional parameters exactly as encoded.
    #[must_use]
    pub const fn parameters(&self) -> Option<Element<'a>> {
        self.parameters
    }

    /// Exact complete DER encoding.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Interprets a certificate-signature identifier supported by `rsl-crypto`.
    ///
    /// # Errors
    ///
    /// Invalid parameters or an unsupported algorithm.
    pub fn signature_algorithm(&self) -> Result<SignatureAlgorithm> {
        if self.oid.is(oid::ECDSA_WITH_SHA256) {
            require_absent(self.parameters)?;
            Ok(SignatureAlgorithm::EcdsaP256Sha256)
        } else if self.oid.is(oid::ECDSA_WITH_SHA384) {
            require_absent(self.parameters)?;
            Ok(SignatureAlgorithm::EcdsaP384Sha384)
        } else if self.oid.is(oid::ED25519) {
            require_absent(self.parameters)?;
            Ok(SignatureAlgorithm::Ed25519)
        } else if self.oid.is(oid::ED448) {
            require_absent(self.parameters)?;
            Ok(SignatureAlgorithm::Ed448)
        } else if self.oid.is(oid::RSASSA_PSS) {
            parse_pss_parameters(self.parameters)
        } else {
            Err(Error::new(ErrorKind::UnsupportedAlgorithm))
        }
    }
}

fn parse_algorithm(element: Element<'_>) -> Result<AlgorithmIdentifier<'_>> {
    let encoded = element.encoded();
    let mut decoder =
        sequence(element).map_err(|_| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?;
    let oid = decoder
        .read()
        .and_then(Element::object_identifier)
        .map_err(|_| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?;
    let parameters = if decoder.is_finished() {
        None
    } else {
        Some(
            decoder
                .read()
                .map_err(|_| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?,
        )
    };
    if !decoder.is_finished() {
        return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier));
    }
    Ok(AlgorithmIdentifier {
        oid,
        parameters,
        encoded,
    })
}

fn require_absent(parameters: Option<Element<'_>>) -> Result<()> {
    if parameters.is_none() {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier))
    }
}

fn require_null_or_absent(parameters: Option<Element<'_>>) -> Result<()> {
    match parameters {
        None => Ok(()),
        Some(parameter) if parameter.tag() == Tag::NULL && parameter.contents().is_empty() => {
            Ok(())
        }
        Some(_) => Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier)),
    }
}

/// Supported certificate-signature profiles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SignatureAlgorithm {
    /// ECDSA P-256 with SHA-256.
    EcdsaP256Sha256,
    /// ECDSA P-384 with SHA-384.
    EcdsaP384Sha384,
    /// Pure Ed25519.
    Ed25519,
    /// Pure Ed448.
    Ed448,
    /// RSASSA-PSS with SHA-256, MGF1-SHA-256, trailer field 1, and an explicit salt length.
    RsaPssSha256 {
        /// Verifier input `sLen`.
        salt_len: usize,
    },
}

fn parse_pss_parameters(parameters: Option<Element<'_>>) -> Result<SignatureAlgorithm> {
    let parameters = parameters.ok_or_else(|| Error::new(ErrorKind::UnsupportedAlgorithm))?;
    let mut fields =
        sequence(parameters).map_err(|_| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?;
    let mut hash_sha256 = false;
    let mut mgf_sha256 = false;
    let mut salt_len = 20_usize;
    let mut trailer = 1_u64;
    let mut previous = None;
    while !fields.is_finished() {
        let field = fields.read()?;
        if field.tag().class != Class::ContextSpecific || !field.tag().constructed {
            return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier));
        }
        if previous.is_some_and(|number| field.tag().number <= number) {
            return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier));
        }
        previous = Some(field.tag().number);
        match field.tag().number {
            0 => {
                let algorithm = parse_explicit_algorithm(field)?;
                require_null_or_absent(algorithm.parameters)?;
                hash_sha256 = algorithm.oid.is(oid::SHA256);
            }
            1 => {
                let algorithm = parse_explicit_algorithm(field)?;
                if !algorithm.oid.is(oid::MGF1) {
                    return Err(Error::new(ErrorKind::UnsupportedAlgorithm));
                }
                let nested = algorithm
                    .parameters
                    .ok_or_else(|| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?;
                let hash = parse_algorithm(nested)?;
                require_null_or_absent(hash.parameters)?;
                mgf_sha256 = hash.oid.is(oid::SHA256);
            }
            2 => {
                salt_len = usize::try_from(parse_explicit_integer(field)?)
                    .map_err(|_| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?;
                if salt_len == 20 {
                    return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier));
                }
            }
            3 => {
                trailer = parse_explicit_integer(field)?;
                if trailer == 1 {
                    return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier));
                }
            }
            _ => return Err(Error::new(ErrorKind::InvalidAlgorithmIdentifier)),
        }
    }
    if hash_sha256 && mgf_sha256 && trailer == 1 {
        Ok(SignatureAlgorithm::RsaPssSha256 { salt_len })
    } else {
        Err(Error::new(ErrorKind::UnsupportedAlgorithm))
    }
}

fn parse_explicit_algorithm(field: Element<'_>) -> Result<AlgorithmIdentifier<'_>> {
    let mut explicit = field.children()?;
    let algorithm = parse_algorithm(explicit.read()?)?;
    explicit.finish()?;
    Ok(algorithm)
}

fn parse_explicit_integer(field: Element<'_>) -> Result<u64> {
    let mut explicit = field.children()?;
    let value = explicit.read()?.unsigned_u64()?;
    explicit.finish()?;
    Ok(value)
}

/// One attribute type and value in a distinguished name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeTypeAndValue<'a> {
    oid: ObjectIdentifier,
    value: Element<'a>,
}

impl<'a> AttributeTypeAndValue<'a> {
    /// Attribute type.
    #[must_use]
    pub fn oid(&self) -> &ObjectIdentifier {
        &self.oid
    }

    /// Attribute value with its original string tag and bytes.
    #[must_use]
    pub const fn value(&self) -> Element<'a> {
        self.value
    }
}

/// A set of attributes forming one relative distinguished name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelativeDistinguishedName<'a>(Vec<AttributeTypeAndValue<'a>>);

impl<'a> RelativeDistinguishedName<'a> {
    /// Attributes in DER order.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeTypeAndValue<'a>] {
        &self.0
    }
}

/// An X.500 distinguished name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name<'a> {
    encoded: &'a [u8],
    rdns: Vec<RelativeDistinguishedName<'a>>,
}

impl<'a> Name<'a> {
    /// Relative distinguished names in sequence order.
    #[must_use]
    pub fn rdns(&self) -> &[RelativeDistinguishedName<'a>] {
        &self.rdns
    }

    /// Exact DER encoding used for minimum RFC 5280 binary name comparison.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Whether the name is the empty sequence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rdns.is_empty()
    }

    /// First common-name string when it uses UTF-8, `PrintableString`, or `IA5String`.
    #[must_use]
    pub fn common_name(&self) -> Option<&'a str> {
        self.rdns
            .iter()
            .flat_map(RelativeDistinguishedName::attributes)
            .find(|attribute| attribute.oid.is(oid::COMMON_NAME))
            .and_then(|attribute| match attribute.value.tag() {
                Tag::UTF8_STRING => attribute.value.utf8_string().ok(),
                Tag::PRINTABLE_STRING => attribute.value.ascii_string(Tag::PRINTABLE_STRING).ok(),
                Tag::IA5_STRING => attribute.value.ascii_string(Tag::IA5_STRING).ok(),
                _ => None,
            })
    }
}

fn parse_name(element: Element<'_>) -> Result<Name<'_>> {
    let encoded = element.encoded();
    let mut name_sequence = sequence(element).map_err(|_| Error::new(ErrorKind::InvalidName))?;
    let mut rdns = Vec::new();
    while !name_sequence.is_finished() {
        let set = name_sequence
            .read()
            .map_err(|_| Error::new(ErrorKind::InvalidName))?;
        if set.tag() != Tag::SET {
            return Err(Error::new(ErrorKind::InvalidName));
        }
        let mut attributes = set.children()?;
        let mut values = Vec::new();
        while !attributes.is_finished() {
            let mut pair = sequence(attributes.read()?)?;
            let oid = pair.read()?.object_identifier()?;
            if pair.is_finished() {
                return Err(Error::new(ErrorKind::InvalidName));
            }
            let value = pair.read()?;
            pair.finish()?;
            values.push(AttributeTypeAndValue { oid, value });
        }
        if values.is_empty() {
            return Err(Error::new(ErrorKind::InvalidName));
        }
        rdns.push(RelativeDistinguishedName(values));
    }
    Ok(Name { encoded, rdns })
}

/// A UTC calendar instant represented by an X.509 validity field.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Time {
    /// Four-digit year.
    pub year: u16,
    /// Month, 1 through 12.
    pub month: u8,
    /// Day of month.
    pub day: u8,
    /// Hour, 0 through 23.
    pub hour: u8,
    /// Minute, 0 through 59.
    pub minute: u8,
    /// Second, 0 through 59.
    pub second: u8,
}

impl Time {
    /// Constructs a checked UTC instant.
    ///
    /// # Errors
    ///
    /// A component lies outside the Gregorian calendar ranges used by X.509.
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Result<Self> {
        let value = Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        };
        if value.is_valid() {
            Ok(value)
        } else {
            Err(Error::new(ErrorKind::InvalidTime))
        }
    }

    fn is_valid(self) -> bool {
        let leap = self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0);
        let days = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return false,
        };
        (1..=days).contains(&self.day) && self.hour < 24 && self.minute < 60 && self.second < 60
    }
}

/// Inclusive certificate validity interval.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Validity {
    /// First valid instant.
    pub not_before: Time,
    /// Last valid instant.
    pub not_after: Time,
}

fn parse_validity(element: Element<'_>) -> Result<Validity> {
    let mut fields = sequence(element)?;
    let not_before = parse_time(fields.read()?)?;
    let not_after = parse_time(fields.read()?)?;
    fields.finish()?;
    if not_before > not_after {
        return Err(Error::new(ErrorKind::InvalidTime));
    }
    Ok(Validity {
        not_before,
        not_after,
    })
}

fn parse_time(element: Element<'_>) -> Result<Time> {
    let (year, tail) = if element.tag() == Tag::UTC_TIME {
        let text = element.ascii_string(Tag::UTC_TIME)?;
        if text.len() != 13 || !text.ends_with('Z') {
            return Err(Error::new(ErrorKind::InvalidTime));
        }
        let year = decimal(&text.as_bytes()[..2])?;
        (
            if year >= 50 { 1900 + year } else { 2000 + year },
            &text.as_bytes()[2..12],
        )
    } else if element.tag() == Tag::GENERALIZED_TIME {
        let text = element.ascii_string(Tag::GENERALIZED_TIME)?;
        if text.len() != 15 || !text.ends_with('Z') {
            return Err(Error::new(ErrorKind::InvalidTime));
        }
        let year = decimal(&text.as_bytes()[..4])?;
        if year < 2050 {
            return Err(Error::new(ErrorKind::InvalidTime));
        }
        (year, &text.as_bytes()[4..14])
    } else {
        return Err(Error::new(ErrorKind::InvalidTime));
    };
    Time::new(
        year,
        u8::try_from(decimal(&tail[0..2])?).map_err(|_| Error::new(ErrorKind::InvalidTime))?,
        u8::try_from(decimal(&tail[2..4])?).map_err(|_| Error::new(ErrorKind::InvalidTime))?,
        u8::try_from(decimal(&tail[4..6])?).map_err(|_| Error::new(ErrorKind::InvalidTime))?,
        u8::try_from(decimal(&tail[6..8])?).map_err(|_| Error::new(ErrorKind::InvalidTime))?,
        u8::try_from(decimal(&tail[8..10])?).map_err(|_| Error::new(ErrorKind::InvalidTime))?,
    )
}

fn decimal(bytes: &[u8]) -> Result<u16> {
    bytes.iter().try_fold(0_u16, |value, byte| {
        if byte.is_ascii_digit() {
            Ok(value * 10 + u16::from(byte - b'0'))
        } else {
            Err(Error::new(ErrorKind::InvalidTime))
        }
    })
}

/// A subject public key and its algorithm identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectPublicKeyInfo<'a> {
    algorithm: AlgorithmIdentifier<'a>,
    subject_public_key: BitString<'a>,
}

impl<'a> SubjectPublicKeyInfo<'a> {
    /// Public-key algorithm identifier.
    #[must_use]
    pub const fn algorithm(&self) -> &AlgorithmIdentifier<'a> {
        &self.algorithm
    }

    /// Exact public-key bit string payload.
    #[must_use]
    pub const fn subject_public_key(&self) -> BitString<'a> {
        self.subject_public_key
    }

    /// Interprets the key under the supported algorithm profile.
    ///
    /// # Errors
    ///
    /// Invalid parameters, key syntax, or an unsupported algorithm.
    pub fn public_key(&self) -> Result<PublicKey<'a>> {
        if self.subject_public_key.unused_bits() != 0 {
            return Err(Error::new(ErrorKind::InvalidPublicKey));
        }
        let bytes = self.subject_public_key.bytes();
        if self.algorithm.oid.is(oid::ED25519) {
            require_absent(self.algorithm.parameters)?;
            require_length(bytes, 32)?;
            Ok(PublicKey::Ed25519(bytes))
        } else if self.algorithm.oid.is(oid::ED448) {
            require_absent(self.algorithm.parameters)?;
            require_length(bytes, 57)?;
            Ok(PublicKey::Ed448(bytes))
        } else if self.algorithm.oid.is(oid::EC_PUBLIC_KEY) {
            let curve = self
                .algorithm
                .parameters
                .ok_or_else(|| Error::new(ErrorKind::InvalidAlgorithmIdentifier))?
                .object_identifier()?;
            if curve.is(oid::SECP256R1) {
                require_length(bytes, 65)?;
                Ok(PublicKey::EcdsaP256(bytes))
            } else if curve.is(oid::SECP384R1) {
                require_length(bytes, 97)?;
                Ok(PublicKey::EcdsaP384(bytes))
            } else {
                Err(Error::new(ErrorKind::UnsupportedAlgorithm))
            }
        } else if self.algorithm.oid.is(oid::RSA_ENCRYPTION)
            || self.algorithm.oid.is(oid::RSASSA_PSS)
        {
            if self.algorithm.oid.is(oid::RSA_ENCRYPTION) {
                require_null_or_absent(self.algorithm.parameters)?;
            } else {
                self.algorithm.signature_algorithm()?;
            }
            let (modulus, exponent) = parse_rsa_public_key(bytes)?;
            Ok(PublicKey::Rsa { modulus, exponent })
        } else {
            Err(Error::new(ErrorKind::UnsupportedAlgorithm))
        }
    }
}

fn parse_spki(element: Element<'_>) -> Result<SubjectPublicKeyInfo<'_>> {
    let mut fields = sequence(element)?;
    let algorithm = parse_algorithm(fields.read()?)?;
    let subject_public_key = fields.read()?.bit_string()?;
    fields.finish()?;
    Ok(SubjectPublicKeyInfo {
        algorithm,
        subject_public_key,
    })
}

fn require_length(bytes: &[u8], expected: usize) -> Result<()> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidPublicKey))
    }
}

fn parse_rsa_public_key(input: &[u8]) -> Result<(&[u8], &[u8])> {
    let element = rsl_asn1::decode_exact(input)?;
    let mut fields = sequence(element)?;
    let modulus = fields.read()?.unsigned_bytes()?;
    let exponent = fields.read()?.unsigned_bytes()?;
    fields.finish()?;
    if modulus.is_empty() || exponent.is_empty() {
        return Err(Error::new(ErrorKind::InvalidPublicKey));
    }
    Ok((modulus, exponent))
}

/// A parsed public key in an algorithm-specific wire representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicKey<'a> {
    /// SEC 1 uncompressed P-256 point.
    EcdsaP256(&'a [u8]),
    /// SEC 1 uncompressed P-384 point.
    EcdsaP384(&'a [u8]),
    /// RFC 8410 Ed25519 bytes.
    Ed25519(&'a [u8]),
    /// RFC 8410 Ed448 bytes.
    Ed448(&'a [u8]),
    /// Unsigned RSA modulus and public exponent.
    Rsa {
        /// Modulus magnitude.
        modulus: &'a [u8],
        /// Public-exponent magnitude.
        exponent: &'a [u8],
    },
}

/// One RFC 5280 certificate extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Extension<'a> {
    oid: ObjectIdentifier,
    critical: bool,
    value: &'a [u8],
}

impl<'a> Extension<'a> {
    /// Extension identifier.
    #[must_use]
    pub fn oid(&self) -> &ObjectIdentifier {
        &self.oid
    }

    /// Whether an implementation must understand the extension to accept the certificate.
    #[must_use]
    pub const fn critical(&self) -> bool {
        self.critical
    }

    /// DER value carried inside `extnValue`'s octet string.
    #[must_use]
    pub const fn value(&self) -> &'a [u8] {
        self.value
    }

    /// Parses `basicConstraints` when this is that extension.
    ///
    /// # Errors
    ///
    /// A matching extension has invalid syntax.
    pub fn basic_constraints(&self) -> Result<Option<BasicConstraints>> {
        if !self.oid.is(oid::BASIC_CONSTRAINTS) {
            return Ok(None);
        }
        let element = rsl_asn1::decode_exact(self.value)?;
        let mut fields = sequence(element)?;
        let ca = if fields.is_finished() {
            false
        } else {
            let first = fields.read()?;
            if first.tag() == Tag::BOOLEAN {
                let value = first.boolean()?;
                if !value {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
                true
            } else {
                return Err(Error::new(ErrorKind::InvalidExtension));
            }
        };
        if fields.is_finished() {
            Ok(Some(BasicConstraints { ca, path_len: None }))
        } else {
            let path_len = fields.read()?.unsigned_u64()?;
            fields.finish()?;
            if !ca {
                return Err(Error::new(ErrorKind::InvalidExtension));
            }
            Ok(Some(BasicConstraints {
                ca,
                path_len: Some(
                    u32::try_from(path_len).map_err(|_| Error::new(ErrorKind::InvalidExtension))?,
                ),
            }))
        }
    }

    /// Parses `keyUsage` when this is that extension.
    ///
    /// # Errors
    ///
    /// A matching extension is not a canonical named bit list or exceeds bit 8.
    pub fn key_usage(&self) -> Result<Option<KeyUsage<'a>>> {
        if !self.oid.is(oid::KEY_USAGE) {
            return Ok(None);
        }
        let bits = rsl_asn1::decode_exact(self.value)?.bit_string()?;
        if bits.bit_len() == 0 || bits.bit_len() > 9 {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let highest = (0..bits.bit_len()).rev().find(|index| bits.bit(*index));
        if highest.map(|index| index + 1) != Some(bits.bit_len()) {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        Ok(Some(KeyUsage { bits }))
    }

    /// Parses `subjectAltName` when this is that extension.
    ///
    /// # Errors
    ///
    /// A matching extension has invalid `GeneralName` syntax.
    pub fn subject_alt_names(&self) -> Result<Option<Vec<GeneralName<'a>>>> {
        if !self.oid.is(oid::SUBJECT_ALT_NAME) {
            return Ok(None);
        }
        let element = rsl_asn1::decode_exact(self.value)?;
        let mut names = sequence(element)?;
        let mut parsed = Vec::new();
        while !names.is_finished() {
            parsed.push(parse_general_name(names.read()?)?);
        }
        if parsed.is_empty() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        Ok(Some(parsed))
    }

    /// Parses `extendedKeyUsage` when this is that extension.
    ///
    /// # Errors
    ///
    /// A matching extension is empty, malformed, or contains duplicate purposes.
    pub fn extended_key_usage(&self) -> Result<Option<Vec<ObjectIdentifier>>> {
        if !self.oid.is(oid::EXTENDED_KEY_USAGE) {
            return Ok(None);
        }
        let element = rsl_asn1::decode_exact(self.value)?;
        let mut purposes = sequence(element)?;
        let mut parsed = Vec::new();
        while !purposes.is_finished() {
            let oid = purposes.read()?.object_identifier()?;
            if parsed.contains(&oid) {
                return Err(Error::new(ErrorKind::InvalidExtension));
            }
            parsed.push(oid);
        }
        if parsed.is_empty() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        Ok(Some(parsed))
    }

    /// Parses `subjectKeyIdentifier` when this is that extension.
    ///
    /// # Errors
    ///
    /// A matching extension is not a non-empty octet string.
    pub fn subject_key_identifier(&self) -> Result<Option<&'a [u8]>> {
        if !self.oid.is(oid::SUBJECT_KEY_IDENTIFIER) {
            return Ok(None);
        }
        let value = rsl_asn1::decode_exact(self.value)?;
        value.expect(Tag::OCTET_STRING)?;
        if value.contents().is_empty() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        Ok(Some(value.contents()))
    }

    /// Parses `authorityKeyIdentifier`.
    ///
    /// # Errors
    ///
    /// A matching extension has malformed or duplicate fields.
    pub fn authority_key_identifier(&self) -> Result<Option<AuthorityKeyIdentifier<'a>>> {
        if !self.oid.is(oid::AUTHORITY_KEY_IDENTIFIER) {
            return Ok(None);
        }
        let element = rsl_asn1::decode_exact(self.value)?;
        let mut fields = sequence(element)?;
        let mut key_identifier = None;
        let mut authority_cert_issuer = None;
        let mut authority_cert_serial_number = None;
        let mut previous = None;
        while !fields.is_finished() {
            let field = fields.read()?;
            if field.tag().class != Class::ContextSpecific
                || previous.is_some_and(|number| field.tag().number <= number)
            {
                return Err(Error::new(ErrorKind::InvalidExtension));
            }
            previous = Some(field.tag().number);
            if field.tag().number == 0 {
                if field.tag().constructed || field.contents().is_empty() {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
                key_identifier = Some(field.contents());
            } else if field.tag().number == 1 {
                if !field.tag().constructed {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
                let mut names = Decoder::new(field.contents());
                let mut parsed = Vec::new();
                while !names.is_finished() {
                    parsed.push(parse_general_name(names.read()?)?);
                }
                if parsed.is_empty() {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
                authority_cert_issuer = Some(parsed);
            } else if field.tag().number == 2 {
                if field.tag().constructed {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
                authority_cert_serial_number = Some(positive_integer_contents(field.contents())?);
            } else {
                return Err(Error::new(ErrorKind::InvalidExtension));
            }
        }
        if authority_cert_issuer.is_some() != authority_cert_serial_number.is_some() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        Ok(Some(AuthorityKeyIdentifier {
            key_identifier,
            authority_cert_issuer,
            authority_cert_serial_number,
        }))
    }

    /// Whether the path validator understands the semantics of this critical extension.
    #[must_use]
    pub fn is_supported_critical(&self) -> bool {
        self.oid.is(oid::SUBJECT_KEY_IDENTIFIER)
            || self.oid.is(oid::KEY_USAGE)
            || self.oid.is(oid::SUBJECT_ALT_NAME)
            || self.oid.is(oid::BASIC_CONSTRAINTS)
            || self.oid.is(oid::AUTHORITY_KEY_IDENTIFIER)
            || self.oid.is(oid::EXTENDED_KEY_USAGE)
    }
}

/// Issuer selectors carried by an `authorityKeyIdentifier` extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityKeyIdentifier<'a> {
    key_identifier: Option<&'a [u8]>,
    authority_cert_issuer: Option<Vec<GeneralName<'a>>>,
    authority_cert_serial_number: Option<&'a [u8]>,
}

impl<'a> AuthorityKeyIdentifier<'a> {
    /// Optional key identifier.
    #[must_use]
    pub const fn key_identifier(&self) -> Option<&'a [u8]> {
        self.key_identifier
    }

    /// Optional issuer names paired with the authority certificate serial number.
    #[must_use]
    pub fn authority_cert_issuer(&self) -> Option<&[GeneralName<'a>]> {
        self.authority_cert_issuer.as_deref()
    }

    /// Optional authority certificate serial-number magnitude.
    #[must_use]
    pub const fn authority_cert_serial_number(&self) -> Option<&'a [u8]> {
        self.authority_cert_serial_number
    }
}

fn positive_integer_contents(contents: &[u8]) -> Result<&[u8]> {
    if contents.is_empty()
        || contents[0] & 0x80 != 0
        || contents.len() > 1 && contents[0] == 0 && contents[1] & 0x80 == 0
    {
        return Err(Error::new(ErrorKind::InvalidExtension));
    }
    let magnitude = if contents.len() > 1 && contents[0] == 0 {
        &contents[1..]
    } else {
        contents
    };
    if magnitude.iter().all(|byte| *byte == 0) {
        return Err(Error::new(ErrorKind::InvalidExtension));
    }
    Ok(magnitude)
}

fn parse_extension(element: Element<'_>) -> Result<Extension<'_>> {
    let mut fields = sequence(element).map_err(|_| Error::new(ErrorKind::InvalidExtension))?;
    let oid = fields.read()?.object_identifier()?;
    let next = fields.read()?;
    let (critical, value) = if next.tag() == Tag::BOOLEAN {
        let critical = next.boolean()?;
        if !critical {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = fields.read()?.expect(Tag::OCTET_STRING)?.contents();
        (true, value)
    } else {
        (false, next.expect(Tag::OCTET_STRING)?.contents())
    };
    fields.finish()?;
    Ok(Extension {
        oid,
        critical,
        value,
    })
}

/// Basic CA and path-length constraints.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BasicConstraints {
    /// Whether the certificate may issue certificates.
    pub ca: bool,
    /// Maximum number of non-self-issued intermediate CA certificates below this certificate.
    pub path_len: Option<u32>,
}

/// RFC 5280 key-usage named bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyUsage<'a> {
    bits: BitString<'a>,
}

impl KeyUsage<'_> {
    /// `digitalSignature` bit.
    #[must_use]
    pub fn digital_signature(self) -> bool {
        self.bits.bit(0)
    }

    /// `keyEncipherment` bit.
    #[must_use]
    pub fn key_encipherment(self) -> bool {
        self.bits.bit(2)
    }

    /// `keyAgreement` bit.
    #[must_use]
    pub fn key_agreement(self) -> bool {
        self.bits.bit(4)
    }

    /// `keyCertSign` bit.
    #[must_use]
    pub fn key_cert_sign(self) -> bool {
        self.bits.bit(5)
    }

    /// `cRLSign` bit.
    #[must_use]
    pub fn crl_sign(self) -> bool {
        self.bits.bit(6)
    }
}

/// Supported and preserved `GeneralName` alternatives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneralName<'a> {
    /// Internet mail address.
    Rfc822Name(&'a str),
    /// DNS name.
    DnsName(&'a str),
    /// URI.
    Uri(&'a str),
    /// Four- or sixteen-byte network address.
    IpAddress(&'a [u8]),
    /// X.500 directory name.
    DirectoryName(Name<'a>),
    /// A syntactically valid but uninterpreted alternative.
    Other(Element<'a>),
}

fn parse_general_name(element: Element<'_>) -> Result<GeneralName<'_>> {
    if element.tag().class != Class::ContextSpecific || element.contents().is_empty() {
        return Err(Error::new(ErrorKind::InvalidExtension));
    }
    match (element.tag().number, element.tag().constructed) {
        (1, false) => Ok(GeneralName::Rfc822Name(ascii(element.contents())?)),
        (2, false) => Ok(GeneralName::DnsName(ascii(element.contents())?)),
        (6, false) => Ok(GeneralName::Uri(ascii(element.contents())?)),
        (7, false) if matches!(element.contents().len(), 4 | 16) => {
            Ok(GeneralName::IpAddress(element.contents()))
        }
        (4, true) => {
            let mut explicit = element.children()?;
            let name = parse_name(explicit.read()?)?;
            explicit.finish()?;
            Ok(GeneralName::DirectoryName(name))
        }
        _ => Ok(GeneralName::Other(element)),
    }
}

fn ascii(bytes: &[u8]) -> Result<&str> {
    if bytes.is_ascii() {
        str::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidExtension))
    } else {
        Err(Error::new(ErrorKind::InvalidExtension))
    }
}

/// The signed portion of an X.509 certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TbsCertificate<'a> {
    encoded: &'a [u8],
    version: Version,
    serial_number: &'a [u8],
    signature: AlgorithmIdentifier<'a>,
    issuer: Name<'a>,
    validity: Validity,
    subject: Name<'a>,
    subject_public_key_info: SubjectPublicKeyInfo<'a>,
    extensions: Vec<Extension<'a>>,
}

impl<'a> TbsCertificate<'a> {
    /// Exact complete DER `TBSCertificate` bytes signed by the issuer.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Certificate version.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Unsigned serial-number magnitude.
    #[must_use]
    pub const fn serial_number(&self) -> &'a [u8] {
        self.serial_number
    }

    /// Signature identifier repeated inside `TBSCertificate`.
    #[must_use]
    pub const fn signature(&self) -> &AlgorithmIdentifier<'a> {
        &self.signature
    }

    /// Issuer distinguished name.
    #[must_use]
    pub const fn issuer(&self) -> &Name<'a> {
        &self.issuer
    }

    /// Inclusive validity interval.
    #[must_use]
    pub const fn validity(&self) -> Validity {
        self.validity
    }

    /// Subject distinguished name.
    #[must_use]
    pub const fn subject(&self) -> &Name<'a> {
        &self.subject
    }

    /// Subject public key.
    #[must_use]
    pub const fn subject_public_key_info(&self) -> &SubjectPublicKeyInfo<'a> {
        &self.subject_public_key_info
    }

    /// Extensions in certificate order.
    #[must_use]
    pub fn extensions(&self) -> &[Extension<'a>] {
        &self.extensions
    }

    /// Finds one extension by object identifier.
    #[must_use]
    pub fn extension(&self, oid: &[u64]) -> Option<&Extension<'a>> {
        self.extensions
            .iter()
            .find(|extension| extension.oid.is(oid))
    }
}

fn parse_tbs(element: Element<'_>) -> Result<TbsCertificate<'_>> {
    let encoded = element.encoded();
    let mut decoder = sequence(element)?;
    let fields = collect(&mut decoder)?;
    let mut index = 0;
    let version = if fields
        .first()
        .is_some_and(|field| field.tag() == Tag::context(0, true))
    {
        let field = fields[index];
        index += 1;
        match parse_explicit_integer(field)? {
            1 => Version::V2,
            2 => Version::V3,
            _ => return Err(Error::new(ErrorKind::InvalidVersion)),
        }
    } else {
        Version::V1
    };
    if fields.len().saturating_sub(index) < 6 {
        return structure();
    }
    let serial_number = fields[index].unsigned_bytes()?;
    index += 1;
    if serial_number.is_empty()
        || serial_number.len() > 20
        || serial_number.iter().all(|byte| *byte == 0)
    {
        return Err(Error::new(ErrorKind::InvalidSerialNumber));
    }
    let signature = parse_algorithm(fields[index])?;
    index += 1;
    let issuer = parse_name(fields[index])?;
    index += 1;
    if issuer.is_empty() {
        return Err(Error::new(ErrorKind::InvalidName));
    }
    let validity = parse_validity(fields[index])?;
    index += 1;
    let subject = parse_name(fields[index])?;
    index += 1;
    let subject_public_key_info = parse_spki(fields[index])?;
    index += 1;

    let mut saw_issuer_unique = false;
    let mut saw_subject_unique = false;
    let mut extensions = Vec::new();
    while index < fields.len() {
        let field = fields[index];
        index += 1;
        match (
            field.tag().class,
            field.tag().number,
            field.tag().constructed,
        ) {
            (Class::ContextSpecific, 1, false)
                if !saw_issuer_unique && !saw_subject_unique && extensions.is_empty() =>
            {
                saw_issuer_unique = true;
                validate_implicit_bit_string(field)?;
            }
            (Class::ContextSpecific, 2, false) if !saw_subject_unique && extensions.is_empty() => {
                saw_subject_unique = true;
                validate_implicit_bit_string(field)?;
            }
            (Class::ContextSpecific, 3, true) if extensions.is_empty() => {
                let mut explicit = field.children()?;
                let mut list = sequence(explicit.read()?)?;
                explicit.finish()?;
                while !list.is_finished() {
                    let extension = parse_extension(list.read()?)?;
                    if extensions
                        .iter()
                        .any(|known: &Extension<'_>| known.oid == extension.oid)
                    {
                        return Err(Error::new(ErrorKind::InvalidExtension));
                    }
                    extensions.push(extension);
                }
                if extensions.is_empty() {
                    return Err(Error::new(ErrorKind::InvalidExtension));
                }
            }
            _ => return structure(),
        }
    }
    if (!extensions.is_empty() && version != Version::V3)
        || ((saw_issuer_unique || saw_subject_unique) && version == Version::V1)
    {
        return Err(Error::new(ErrorKind::InvalidVersion));
    }
    Ok(TbsCertificate {
        encoded,
        version,
        serial_number,
        signature,
        issuer,
        validity,
        subject,
        subject_public_key_info,
        extensions,
    })
}

fn validate_implicit_bit_string(field: Element<'_>) -> Result<()> {
    let contents = field.contents();
    let Some((&unused, bytes)) = contents.split_first() else {
        return Err(Error::new(ErrorKind::InvalidStructure));
    };
    if unused > 7
        || bytes.is_empty() && unused != 0
        || bytes.last().is_some_and(|last| {
            let mask = if unused == 0 { 0 } else { (1_u8 << unused) - 1 };
            last & mask != 0
        })
    {
        return Err(Error::new(ErrorKind::InvalidStructure));
    }
    Ok(())
}

/// A complete borrowed X.509 certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Certificate<'a> {
    encoded: &'a [u8],
    body: TbsCertificate<'a>,
    signature_algorithm: AlgorithmIdentifier<'a>,
    signature_value: &'a [u8],
}

impl<'a> Certificate<'a> {
    /// Parses exactly one strict DER certificate.
    ///
    /// # Errors
    ///
    /// Malformed DER, invalid X.509 structure, or inconsistent signature identifiers.
    pub fn from_der(input: &'a [u8]) -> Result<Self> {
        let outer = rsl_asn1::decode_exact(input)?;
        let mut fields = sequence(outer)?;
        let body = parse_tbs(fields.read()?)?;
        let signature_algorithm = parse_algorithm(fields.read()?)?;
        let signature = fields.read()?.bit_string()?;
        fields.finish()?;
        if signature.unused_bits() != 0 || signature.bytes().is_empty() {
            return Err(Error::new(ErrorKind::InvalidSignature));
        }
        if body.signature.encoded != signature_algorithm.encoded {
            return Err(Error::new(ErrorKind::SignatureAlgorithmMismatch));
        }
        Ok(Self {
            encoded: input,
            body,
            signature_algorithm,
            signature_value: signature.bytes(),
        })
    }

    /// Exact complete DER certificate.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Returns an owned byte-for-byte DER encoding.
    #[must_use]
    pub fn to_der(&self) -> Vec<u8> {
        self.encoded.to_vec()
    }

    /// Signed certificate body.
    #[must_use]
    pub const fn tbs_certificate(&self) -> &TbsCertificate<'a> {
        &self.body
    }

    /// Outer signature algorithm.
    #[must_use]
    pub const fn signature_algorithm(&self) -> &AlgorithmIdentifier<'a> {
        &self.signature_algorithm
    }

    /// Signature payload, excluding the ASN.1 bit-string unused-bit octet.
    #[must_use]
    pub const fn signature_value(&self) -> &'a [u8] {
        self.signature_value
    }

    /// Parses a fixed-width `r || s` ECDSA signature from X.509's DER pair of integers.
    ///
    /// # Errors
    ///
    /// The signature is not a pair of positive integers fitting the requested width.
    pub fn ecdsa_signature(&self, component_len: usize) -> Result<Vec<u8>> {
        let element = rsl_asn1::decode_exact(self.signature_value)
            .map_err(|_| Error::new(ErrorKind::InvalidSignature))?;
        let mut components =
            sequence(element).map_err(|_| Error::new(ErrorKind::InvalidSignature))?;
        let r = components
            .read()
            .and_then(Element::unsigned_bytes)
            .map_err(|_| Error::new(ErrorKind::InvalidSignature))?;
        let s = components
            .read()
            .and_then(Element::unsigned_bytes)
            .map_err(|_| Error::new(ErrorKind::InvalidSignature))?;
        components
            .finish()
            .map_err(|_| Error::new(ErrorKind::InvalidSignature))?;
        if r.is_empty() || s.is_empty() || r.len() > component_len || s.len() > component_len {
            return Err(Error::new(ErrorKind::InvalidSignature));
        }
        let mut fixed = vec![0_u8; component_len * 2];
        fixed[component_len - r.len()..component_len].copy_from_slice(r);
        fixed[2 * component_len - s.len()..].copy_from_slice(s);
        Ok(fixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsl_asn1::Encoder;

    fn built(build: impl FnOnce(&mut Encoder) -> rsl_asn1::Result<()>) -> Vec<u8> {
        let mut encoder = Encoder::new();
        build(&mut encoder).unwrap();
        encoder.finish()
    }

    fn object_identifier(arcs: &[u64]) -> ObjectIdentifier {
        ObjectIdentifier::from_arcs(arcs).unwrap()
    }

    fn algorithm(arcs: &[u64]) -> Vec<u8> {
        built(|encoder| {
            encoder.sequence(|sequence| sequence.object_identifier(&object_identifier(arcs)))
        })
    }

    fn name(common_name: &str) -> Vec<u8> {
        let attribute = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&object_identifier(oid::COMMON_NAME))?;
                sequence.element(Tag::UTF8_STRING, common_name.as_bytes())
            })
        });
        let set = built(|encoder| encoder.element(Tag::SET, &attribute));
        built(|encoder| encoder.element(Tag::SEQUENCE, &set))
    }

    fn test_certificate() -> Vec<u8> {
        let signature_algorithm = algorithm(oid::ED25519);
        let issuer = name("Test Issuer");
        let subject = name("example.test");
        let validity = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::UTC_TIME, b"260101000000Z")?;
                sequence.element(Tag::UTC_TIME, b"270101000000Z")
            })
        });
        let spki = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.encoded(&algorithm(oid::ED25519))?;
                sequence.bit_string(0, &[7; 32])
            })
        });
        let san_value = built(|encoder| {
            encoder.sequence(|sequence| sequence.element(Tag::context(2, false), b"example.test"))
        });
        let extension = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&object_identifier(oid::SUBJECT_ALT_NAME))?;
                sequence.octet_string(&san_value)
            })
        });
        let extensions = built(|encoder| encoder.sequence(|sequence| sequence.encoded(&extension)));
        let version = built(|encoder| encoder.unsigned_integer(&[2]));
        let tbs = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::context(0, true), &version)?;
                sequence.unsigned_integer(&[1])?;
                sequence.encoded(&signature_algorithm)?;
                sequence.encoded(&issuer)?;
                sequence.encoded(&validity)?;
                sequence.encoded(&subject)?;
                sequence.encoded(&spki)?;
                sequence.element(Tag::context(3, true), &extensions)
            })
        });
        built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.encoded(&tbs)?;
                sequence.encoded(&signature_algorithm)?;
                sequence.bit_string(0, &[9; 64])
            })
        })
    }

    #[test]
    fn standard_derived_v3_certificate_preserves_tbs_and_parses_san() {
        let der = test_certificate();
        let certificate = Certificate::from_der(&der).unwrap();
        assert_eq!(certificate.to_der(), der);
        assert_eq!(certificate.tbs_certificate().version(), Version::V3);
        assert_eq!(
            certificate.tbs_certificate().subject().common_name(),
            Some("example.test")
        );
        assert!(certificate.tbs_certificate().encoded().starts_with(&[0x30]));
        assert_eq!(
            certificate
                .signature_algorithm()
                .signature_algorithm()
                .unwrap(),
            SignatureAlgorithm::Ed25519
        );
        let sans = certificate
            .tbs_certificate()
            .extension(oid::SUBJECT_ALT_NAME)
            .unwrap()
            .subject_alt_names()
            .unwrap()
            .unwrap();
        assert_eq!(sans, [GeneralName::DnsName("example.test")]);
    }

    #[test]
    fn negative_signature_algorithm_mismatch_is_rejected() {
        let mut der = test_certificate();
        let ed25519 = algorithm(oid::ED25519);
        let ed448 = algorithm(oid::ED448);
        let signature_offset = der
            .windows(ed25519.len())
            .rposition(|window| window == ed25519)
            .unwrap();
        der.splice(signature_offset..signature_offset + ed448.len(), ed448);
        assert_eq!(
            Certificate::from_der(&der).unwrap_err().kind,
            ErrorKind::SignatureAlgorithmMismatch
        );
    }

    #[test]
    fn negative_explicit_default_version_is_rejected() {
        let mut der = test_certificate();
        let version = [0xa0, 0x03, 0x02, 0x01, 0x02];
        let offset = der
            .windows(version.len())
            .position(|window| window == version)
            .unwrap();
        der[offset + version.len() - 1] = 0;
        assert_eq!(
            Certificate::from_der(&der).unwrap_err().kind,
            ErrorKind::InvalidVersion
        );
    }

    #[test]
    fn standard_derived_authority_key_identifier_is_typed() {
        let issuer = name("Authority");
        let directory_name = built(|encoder| encoder.element(Tag::context(4, true), &issuer));
        let value = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::context(0, false), &[1, 2, 3])?;
                sequence.element(Tag::context(1, true), &directory_name)?;
                sequence.element(Tag::context(2, false), &[7])
            })
        });
        let extension = Extension {
            oid: object_identifier(oid::AUTHORITY_KEY_IDENTIFIER),
            critical: false,
            value: &value,
        };
        let authority = extension.authority_key_identifier().unwrap().unwrap();
        assert_eq!(authority.key_identifier(), Some(&[1, 2, 3][..]));
        assert_eq!(authority.authority_cert_serial_number(), Some(&[7][..]));
        assert!(matches!(
            authority.authority_cert_issuer().unwrap(),
            [GeneralName::DirectoryName(name)] if name.common_name() == Some("Authority")
        ));

        let unpaired = built(|encoder| {
            encoder.sequence(|sequence| sequence.element(Tag::context(2, false), &[7]))
        });
        let extension = Extension {
            oid: object_identifier(oid::AUTHORITY_KEY_IDENTIFIER),
            critical: false,
            value: &unpaired,
        };
        assert_eq!(
            extension.authority_key_identifier().unwrap_err().kind,
            ErrorKind::InvalidExtension
        );
    }

    #[test]
    fn time_checks_calendar_and_inclusive_order() {
        assert!(Time::new(2024, 2, 29, 23, 59, 59).is_ok());
        assert_eq!(
            Time::new(2023, 2, 29, 0, 0, 0).unwrap_err().kind,
            ErrorKind::InvalidTime
        );
        assert_eq!(
            parse_time(rsl_asn1::decode_exact(b"\x18\x0f20260101000000Z").unwrap())
                .unwrap_err()
                .kind,
            ErrorKind::InvalidTime
        );
    }
}
