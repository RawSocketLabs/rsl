//! Canonical certificate construction with guided profiles and explicit raw extension points.
//!
//! Construction produces syntax, not trust. [`CertificateDer`](crate::builder::CertificateDer) is
//! an owned, parseable certificate; it is not a validation result. Common end-entity and CA
//! profiles install conservative baseline extensions, while
//! [`CertificateBuilder::custom`](crate::builder::CertificateBuilder::custom) and
//! [`ExtensionDer::from_parts`](crate::builder::ExtensionDer::from_parts) make unusual standards
//! work visible at the call site.

use alloc::{vec, vec::Vec};
use core::{fmt, marker::PhantomData};

use rsl_asn1::{Encoder, ObjectIdentifier, Tag};

use super::{Certificate, Error, ErrorKind, Time, oid, parse_algorithm, parse_name, parse_spki};

/// Owned canonical DER for an X.500 distinguished name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameDer {
    bytes: Vec<u8>,
    empty: bool,
}

impl NameDer {
    /// Constructs a name containing one UTF-8 `commonName` RDN.
    ///
    /// # Errors
    ///
    /// The value is empty or DER construction fails.
    pub fn common_name(value: &str) -> super::Result<Self> {
        NameBuilder::new().common_name(value)?.build()
    }

    /// Constructs the explicitly empty name used by end-entity profiles whose identity is solely
    /// in a critical subject-alternative-name extension.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            bytes: vec![0x30, 0x00],
            empty: true,
        }
    }

    /// Imports exactly one strict DER `Name` while preserving its encoding.
    ///
    /// This is the deliberate escape hatch for multi-valued RDNs and string forms not covered by
    /// [`NameBuilder`].
    ///
    /// # Errors
    ///
    /// The bytes are not a syntactically valid X.509 name.
    pub fn from_der(bytes: Vec<u8>) -> super::Result<Self> {
        let empty = {
            let element = rsl_asn1::decode_exact(&bytes)?;
            parse_name(element)?.is_empty()
        };
        Ok(Self { bytes, empty })
    }

    /// Borrows the complete DER encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the owner and returns the complete DER encoding.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Builder for a sequence of single-valued relative distinguished names.
///
/// Each call adds one RDN. Use [`NameDer::from_der`] when a test or external profile needs an
/// already encoded multi-valued RDN.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NameBuilder {
    attributes: Vec<Vec<u8>>,
}

impl NameBuilder {
    /// Starts an empty name.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attributes: Vec::new(),
        }
    }

    /// Adds one UTF-8 `commonName` RDN.
    ///
    /// # Errors
    ///
    /// The value is empty or DER construction fails.
    pub fn common_name(self, value: &str) -> super::Result<Self> {
        let oid = object_identifier(oid::COMMON_NAME)?;
        self.utf8_attribute(&oid, value)
    }

    /// Adds one caller-selected UTF-8 attribute as its own RDN.
    ///
    /// This is an explicit schema escape hatch: the caller owns whether the object identifier
    /// permits `UTF8String` and whether the value satisfies its size/profile rules.
    ///
    /// # Errors
    ///
    /// The value is empty or DER construction fails.
    pub fn utf8_attribute(mut self, oid: &ObjectIdentifier, value: &str) -> super::Result<Self> {
        if value.is_empty() {
            return Err(Error::new(ErrorKind::InvalidName));
        }
        self.attributes
            .push(attribute(oid, Tag::UTF8_STRING, value.as_bytes())?);
        Ok(self)
    }

    /// Adds one caller-selected `PrintableString` attribute as its own RDN.
    ///
    /// # Errors
    ///
    /// The value is empty, contains a character outside the ASN.1 `PrintableString` set, or DER
    /// construction fails.
    pub fn printable_attribute(
        mut self,
        oid: &ObjectIdentifier,
        value: &str,
    ) -> super::Result<Self> {
        if value.is_empty() || !value.bytes().all(is_printable_string_byte) {
            return Err(Error::new(ErrorKind::InvalidName));
        }
        self.attributes
            .push(attribute(oid, Tag::PRINTABLE_STRING, value.as_bytes())?);
        Ok(self)
    }

    /// Finishes the name. No attributes intentionally produces the empty name.
    ///
    /// # Errors
    ///
    /// DER construction fails.
    pub fn build(self) -> super::Result<NameDer> {
        let empty = self.attributes.is_empty();
        let bytes = built(|encoder| {
            encoder.sequence(|sequence| {
                for attribute in &self.attributes {
                    sequence.set_of(core::slice::from_ref(attribute))?;
                }
                Ok(())
            })
        })?;
        let name = NameDer::from_der(bytes)?;
        debug_assert_eq!(name.empty, empty);
        Ok(name)
    }
}

fn attribute(oid: &ObjectIdentifier, tag: Tag, value: &[u8]) -> super::Result<Vec<u8>> {
    built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.object_identifier(oid)?;
            sequence.element(tag, value)
        })
    })
}

/// Owned canonical DER for an `AlgorithmIdentifier` used to sign a certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureAlgorithmDer(Vec<u8>);

impl SignatureAlgorithmDer {
    /// Pure Ed25519 with absent parameters.
    ///
    /// # Errors
    ///
    /// DER construction fails.
    pub fn ed25519() -> super::Result<Self> {
        algorithm_identifier(oid::ED25519, None)
    }

    /// Pure Ed448 with absent parameters.
    ///
    /// # Errors
    ///
    /// DER construction fails.
    pub fn ed448() -> super::Result<Self> {
        algorithm_identifier(oid::ED448, None)
    }

    /// ECDSA P-256 with SHA-256 and absent parameters.
    ///
    /// # Errors
    ///
    /// DER construction fails.
    pub fn ecdsa_p256_sha256() -> super::Result<Self> {
        algorithm_identifier(oid::ECDSA_WITH_SHA256, None)
    }

    /// ECDSA P-384 with SHA-384 and absent parameters.
    ///
    /// # Errors
    ///
    /// DER construction fails.
    pub fn ecdsa_p384_sha384() -> super::Result<Self> {
        algorithm_identifier(oid::ECDSA_WITH_SHA384, None)
    }

    /// Imports exactly one strict DER algorithm identifier.
    ///
    /// The identifier need not be in the built-in validation profile. This is the explicit
    /// algorithm escape hatch for experiments and externally governed certificate formats.
    ///
    /// # Errors
    ///
    /// The bytes are not exactly one syntactically valid `AlgorithmIdentifier`.
    pub fn from_der(bytes: Vec<u8>) -> super::Result<Self> {
        {
            let element = rsl_asn1::decode_exact(&bytes)?;
            parse_algorithm(element)?;
        }
        Ok(Self(bytes))
    }

    /// Borrows the complete DER encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn algorithm_identifier(
    arcs: &[u64],
    parameters: Option<&[u8]>,
) -> super::Result<SignatureAlgorithmDer> {
    let oid = object_identifier(arcs)?;
    let bytes = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.object_identifier(&oid)?;
            if let Some(parameters) = parameters {
                sequence.encoded(parameters)?;
            }
            Ok(())
        })
    })?;
    SignatureAlgorithmDer::from_der(bytes)
}

/// Owned canonical DER for `SubjectPublicKeyInfo`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectPublicKeyInfoDer(Vec<u8>);

impl SubjectPublicKeyInfoDer {
    /// Constructs an Ed25519 subject public key.
    ///
    /// # Errors
    ///
    /// DER construction or structural validation fails.
    pub fn ed25519(public_key: &[u8; 32]) -> super::Result<Self> {
        spki(oid::ED25519, None, public_key)
    }

    /// Constructs an Ed448 subject public key.
    ///
    /// # Errors
    ///
    /// DER construction or structural validation fails.
    pub fn ed448(public_key: &[u8; 57]) -> super::Result<Self> {
        spki(oid::ED448, None, public_key)
    }

    /// Constructs an uncompressed P-256 subject public key.
    ///
    /// # Errors
    ///
    /// DER construction or structural validation fails.
    pub fn ecdsa_p256(public_key: &[u8; 65]) -> super::Result<Self> {
        if public_key[0] != 0x04 {
            return Err(Error::new(ErrorKind::InvalidPublicKey));
        }
        let curve = oid_der(oid::SECP256R1)?;
        spki(oid::EC_PUBLIC_KEY, Some(&curve), public_key)
    }

    /// Constructs an uncompressed P-384 subject public key.
    ///
    /// # Errors
    ///
    /// DER construction or structural validation fails.
    pub fn ecdsa_p384(public_key: &[u8; 97]) -> super::Result<Self> {
        if public_key[0] != 0x04 {
            return Err(Error::new(ErrorKind::InvalidPublicKey));
        }
        let curve = oid_der(oid::SECP384R1)?;
        spki(oid::EC_PUBLIC_KEY, Some(&curve), public_key)
    }

    /// Constructs an unrestricted `rsaEncryption` subject public key.
    ///
    /// This only encodes public components. Algorithm policy and minimum modulus strength remain
    /// verifier responsibilities.
    ///
    /// # Errors
    ///
    /// A component is zero/empty or DER construction fails.
    pub fn rsa_encryption(modulus: &[u8], exponent: &[u8]) -> super::Result<Self> {
        if magnitude_is_zero(modulus) || magnitude_is_zero(exponent) {
            return Err(Error::new(ErrorKind::InvalidPublicKey));
        }
        let public_key = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.unsigned_integer(modulus)?;
                sequence.unsigned_integer(exponent)
            })
        })?;
        let null = built(Encoder::null)?;
        spki(oid::RSA_ENCRYPTION, Some(&null), &public_key)
    }

    /// Imports exactly one strict DER `SubjectPublicKeyInfo`.
    ///
    /// Unsupported algorithms are preserved as syntax. Calling [`super::SubjectPublicKeyInfo::public_key`]
    /// on the resulting parsed certificate still applies the supported algorithm profile.
    ///
    /// # Errors
    ///
    /// The bytes are not syntactically valid `SubjectPublicKeyInfo`.
    pub fn from_der(bytes: Vec<u8>) -> super::Result<Self> {
        {
            let element = rsl_asn1::decode_exact(&bytes)?;
            parse_spki(element)?;
        }
        Ok(Self(bytes))
    }

    /// Borrows the complete DER encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn spki(
    algorithm_oid: &[u64],
    parameters: Option<&[u8]>,
    public_key: &[u8],
) -> super::Result<SubjectPublicKeyInfoDer> {
    let algorithm = algorithm_identifier(algorithm_oid, parameters)?;
    let bytes = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.encoded(algorithm.as_bytes())?;
            sequence.bit_string(0, public_key)
        })
    })?;
    SubjectPublicKeyInfoDer::from_der(bytes)
}

fn magnitude_is_zero(value: &[u8]) -> bool {
    value.is_empty() || value.iter().all(|byte| *byte == 0)
}

fn is_printable_string_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b' ' | b'\'' | b'(' | b')' | b'+' | b',' | b'-' | b'.' | b'/' | b':' | b'=' | b'?'
        )
}

/// RFC 5280 key-usage named bits used by the typed extension builder.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyUsages(u16);

impl KeyUsages {
    /// `digitalSignature`.
    pub const DIGITAL_SIGNATURE: Self = Self(1 << 0);
    /// `contentCommitment` / `nonRepudiation`.
    pub const CONTENT_COMMITMENT: Self = Self(1 << 1);
    /// `keyEncipherment`.
    pub const KEY_ENCIPHERMENT: Self = Self(1 << 2);
    /// `dataEncipherment`.
    pub const DATA_ENCIPHERMENT: Self = Self(1 << 3);
    /// `keyAgreement`.
    pub const KEY_AGREEMENT: Self = Self(1 << 4);
    /// `keyCertSign`.
    pub const KEY_CERT_SIGN: Self = Self(1 << 5);
    /// `cRLSign`.
    pub const CRL_SIGN: Self = Self(1 << 6);
    /// `encipherOnly`.
    pub const ENCIPHER_ONLY: Self = Self(1 << 7);
    /// `decipherOnly`.
    pub const DECIPHER_ONLY: Self = Self(1 << 8);

    /// Combines named usages.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether this set contains a named usage.
    #[must_use]
    pub const fn contains(self, usage: Self) -> bool {
        self.0 & usage.0 == usage.0
    }
}

/// A typed subject-alternative-name value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubjectAltName<'a> {
    /// Internet mail address.
    Rfc822Name(&'a str),
    /// DNS identity.
    DnsName(&'a str),
    /// URI identity.
    Uri(&'a str),
    /// Four-byte IPv4 or sixteen-byte IPv6 address.
    IpAddress(&'a [u8]),
}

/// An extended-key-usage purpose for the typed certificate profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtendedKeyPurpose {
    /// TLS server authentication.
    ServerAuth,
    /// TLS client authentication.
    ClientAuth,
    /// Any extended key usage.
    Any,
    /// Caller-selected purpose for an external or experimental profile.
    Other(ObjectIdentifier),
}

impl ExtendedKeyPurpose {
    fn object_identifier(&self) -> super::Result<ObjectIdentifier> {
        match self {
            Self::ServerAuth => object_identifier(oid::SERVER_AUTH),
            Self::ClientAuth => object_identifier(oid::CLIENT_AUTH),
            Self::Any => object_identifier(oid::ANY_EXTENDED_KEY_USAGE),
            Self::Other(oid) => Ok(oid.clone()),
        }
    }
}

/// One canonical extension ready for insertion in a certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionDer {
    oid: ObjectIdentifier,
    critical: bool,
    value: Vec<u8>,
}

impl ExtensionDer {
    /// Constructs an extension from an OID and the DER value carried inside `extnValue`.
    ///
    /// This is the explicit escape hatch for new or test-only extensions. It accepts syntax, not
    /// semantics; a critical unknown extension will still fail normal path validation closed.
    ///
    /// # Errors
    ///
    /// `value_der` is not exactly one strict DER element.
    pub fn from_parts(
        oid: ObjectIdentifier,
        critical: bool,
        value_der: Vec<u8>,
    ) -> super::Result<Self> {
        rsl_asn1::decode_exact(&value_der)?;
        Ok(Self {
            oid,
            critical,
            value: value_der,
        })
    }

    /// Constructs `basicConstraints`.
    ///
    /// # Errors
    ///
    /// A path length is supplied for a non-CA certificate or DER construction fails.
    pub fn basic_constraints(
        ca: bool,
        path_len: Option<u32>,
        critical: bool,
    ) -> super::Result<Self> {
        if !ca && path_len.is_some() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = built(|encoder| {
            encoder.sequence(|sequence| {
                if ca {
                    sequence.boolean(true)?;
                }
                if let Some(path_len) = path_len {
                    sequence.unsigned_integer(&path_len.to_be_bytes())?;
                }
                Ok(())
            })
        })?;
        Self::from_parts(object_identifier(oid::BASIC_CONSTRAINTS)?, critical, value)
    }

    /// Constructs a non-empty canonical `keyUsage` named-bit list.
    ///
    /// # Errors
    ///
    /// The set is empty or DER construction fails.
    pub fn key_usage(usages: KeyUsages, critical: bool) -> super::Result<Self> {
        let Some(highest) = (0..9).rev().find(|index| usages.0 & (1 << index) != 0) else {
            return Err(Error::new(ErrorKind::InvalidExtension));
        };
        let byte_len = highest / 8 + 1;
        let mut bytes = vec![0_u8; byte_len];
        for index in 0..=highest {
            if usages.0 & (1 << index) != 0 {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        let unused = u8::try_from(byte_len * 8 - highest - 1)
            .map_err(|_| Error::new(ErrorKind::InvalidExtension))?;
        let value = built(|encoder| encoder.bit_string(unused, &bytes))?;
        Self::from_parts(object_identifier(oid::KEY_USAGE)?, critical, value)
    }

    /// Constructs a non-empty subject-alternative-name sequence.
    ///
    /// # Errors
    ///
    /// A name is empty, non-ASCII, has an invalid IP length, or DER construction fails.
    pub fn subject_alt_names(names: &[SubjectAltName<'_>], critical: bool) -> super::Result<Self> {
        if names.is_empty()
            || names.iter().any(|name| match name {
                SubjectAltName::Rfc822Name(value)
                | SubjectAltName::DnsName(value)
                | SubjectAltName::Uri(value) => value.is_empty() || !value.is_ascii(),
                SubjectAltName::IpAddress(value) => !matches!(value.len(), 4 | 16),
            })
        {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = built(|encoder| {
            encoder.sequence(|sequence| {
                for name in names {
                    let (number, bytes) = match name {
                        SubjectAltName::Rfc822Name(value) => (1, value.as_bytes()),
                        SubjectAltName::DnsName(value) => (2, value.as_bytes()),
                        SubjectAltName::Uri(value) => (6, value.as_bytes()),
                        SubjectAltName::IpAddress(value) => (7, *value),
                    };
                    sequence.element(Tag::context(number, false), bytes)?;
                }
                Ok(())
            })
        })?;
        Self::from_parts(object_identifier(oid::SUBJECT_ALT_NAME)?, critical, value)
    }

    /// Constructs a non-empty, duplicate-free `extendedKeyUsage` sequence.
    ///
    /// # Errors
    ///
    /// The list is empty, contains duplicates, or DER construction fails.
    pub fn extended_key_usage(
        purposes: &[ExtendedKeyPurpose],
        critical: bool,
    ) -> super::Result<Self> {
        let purposes = purposes
            .iter()
            .map(ExtendedKeyPurpose::object_identifier)
            .collect::<super::Result<Vec<_>>>()?;
        if purposes.is_empty() || has_duplicates(&purposes) {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = built(|encoder| {
            encoder.sequence(|sequence| {
                for purpose in &purposes {
                    sequence.object_identifier(purpose)?;
                }
                Ok(())
            })
        })?;
        Self::from_parts(object_identifier(oid::EXTENDED_KEY_USAGE)?, critical, value)
    }

    /// Constructs a non-empty subject-key identifier.
    ///
    /// # Errors
    ///
    /// The identifier is empty or DER construction fails.
    pub fn subject_key_identifier(identifier: &[u8], critical: bool) -> super::Result<Self> {
        if identifier.is_empty() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = built(|encoder| encoder.octet_string(identifier))?;
        Self::from_parts(
            object_identifier(oid::SUBJECT_KEY_IDENTIFIER)?,
            critical,
            value,
        )
    }

    /// Constructs an authority-key identifier containing only a non-empty key identifier.
    ///
    /// # Errors
    ///
    /// The identifier is empty or DER construction fails.
    pub fn authority_key_identifier(identifier: &[u8], critical: bool) -> super::Result<Self> {
        if identifier.is_empty() {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        let value = built(|encoder| {
            encoder.sequence(|sequence| sequence.element(Tag::context(0, false), identifier))
        })?;
        Self::from_parts(
            object_identifier(oid::AUTHORITY_KEY_IDENTIFIER)?,
            critical,
            value,
        )
    }

    /// Extension object identifier.
    #[must_use]
    pub const fn oid(&self) -> &ObjectIdentifier {
        &self.oid
    }

    fn encoded(&self) -> super::Result<Vec<u8>> {
        built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&self.oid)?;
                if self.critical {
                    sequence.boolean(true)?;
                }
                sequence.octet_string(&self.value)
            })
        })
    }
}

/// Typestate indicating that certificate validity is still required.
#[derive(Debug)]
pub struct NeedsValidity;

/// Typestate indicating that a subject public key is still required.
#[derive(Debug)]
pub struct NeedsPublicKey;

/// Typestate indicating that all mandatory certificate fields are present.
#[derive(Debug)]
pub struct ReadyToSign;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    EndEntity,
    CertificateAuthority,
    Custom,
}

/// Typestate certificate builder.
///
/// Constructors collect identity and serial inputs, [`Self::validity`] advances to the public-key
/// stage, and [`CertificateBuilder<NeedsPublicKey>::subject_public_key_info`] produces a
/// [`ReadyToSign`] builder. Only that final state exposes extension and signing operations.
#[derive(Debug)]
pub struct CertificateBuilder<State> {
    profile: Profile,
    serial: Vec<u8>,
    issuer: NameDer,
    subject: NameDer,
    not_before: Option<Time>,
    not_after: Option<Time>,
    subject_public_key_info: Option<SubjectPublicKeyInfoDer>,
    extensions: Vec<ExtensionDer>,
    state: PhantomData<State>,
}

impl CertificateBuilder<NeedsValidity> {
    /// Starts a guided digital-signature end-entity profile.
    ///
    /// The profile begins with critical `basicConstraints(cA = FALSE)` and `keyUsage =
    /// digitalSignature`. [`CertificateBuilder::<ReadyToSign>::key_usage`] can select a different
    /// explicit usage set.
    ///
    /// # Errors
    ///
    /// The serial or issuer name violates the supported RFC 5280 profile.
    pub fn end_entity(serial: &[u8], issuer: NameDer, subject: NameDer) -> super::Result<Self> {
        Self::start(Profile::EndEntity, serial, issuer, subject, None)
    }

    /// Starts a guided certificate-authority profile.
    ///
    /// The profile begins with critical `basicConstraints(cA = TRUE)` and `keyUsage =
    /// keyCertSign`. A supplied path length is encoded into basic constraints.
    ///
    /// # Errors
    ///
    /// The serial, issuer, or subject name violates the supported RFC 5280 profile.
    pub fn certificate_authority(
        serial: &[u8],
        issuer: NameDer,
        subject: NameDer,
        path_len: Option<u32>,
    ) -> super::Result<Self> {
        Self::start(
            Profile::CertificateAuthority,
            serial,
            issuer,
            subject,
            path_len,
        )
    }

    /// Starts a V3 certificate with no profile-supplied extensions.
    ///
    /// This explicit escape hatch is for test certificates and externally governed profiles. It
    /// only guarantees canonical syntax; it makes no end-entity or CA issuance-policy claim.
    ///
    /// # Errors
    ///
    /// The serial or issuer name violates the supported RFC 5280 syntax profile.
    pub fn custom(serial: &[u8], issuer: NameDer, subject: NameDer) -> super::Result<Self> {
        Self::start(Profile::Custom, serial, issuer, subject, None)
    }

    fn start(
        profile: Profile,
        serial: &[u8],
        issuer: NameDer,
        subject: NameDer,
        path_len: Option<u32>,
    ) -> super::Result<Self> {
        if serial.is_empty()
            || serial.len() > 20
            || serial[0] == 0
            || serial.iter().all(|byte| *byte == 0)
            || issuer.empty
            || profile == Profile::CertificateAuthority && subject.empty
        {
            return Err(Error::new(
                if issuer.empty || profile == Profile::CertificateAuthority && subject.empty {
                    ErrorKind::InvalidName
                } else {
                    ErrorKind::InvalidSerialNumber
                },
            ));
        }
        let extensions = match profile {
            Profile::EndEntity => vec![
                ExtensionDer::basic_constraints(false, None, true)?,
                ExtensionDer::key_usage(KeyUsages::DIGITAL_SIGNATURE, true)?,
            ],
            Profile::CertificateAuthority => vec![
                ExtensionDer::basic_constraints(true, path_len, true)?,
                ExtensionDer::key_usage(KeyUsages::KEY_CERT_SIGN, true)?,
            ],
            Profile::Custom => Vec::new(),
        };
        Ok(Self {
            profile,
            serial: serial.to_vec(),
            issuer,
            subject,
            not_before: None,
            not_after: None,
            subject_public_key_info: None,
            extensions,
            state: PhantomData,
        })
    }

    /// Supplies the inclusive validity interval and advances the builder.
    ///
    /// # Errors
    ///
    /// The interval is reversed or a year lies outside the supported X.509 range 1950–9999.
    pub fn validity(
        self,
        not_before: Time,
        not_after: Time,
    ) -> super::Result<CertificateBuilder<NeedsPublicKey>> {
        if !not_before.is_valid()
            || !not_after.is_valid()
            || not_before > not_after
            || not_before.year < 1950
            || not_after.year < 1950
            || not_before.year > 9999
            || not_after.year > 9999
        {
            return Err(Error::new(ErrorKind::InvalidTime));
        }
        Ok(CertificateBuilder {
            profile: self.profile,
            serial: self.serial,
            issuer: self.issuer,
            subject: self.subject,
            not_before: Some(not_before),
            not_after: Some(not_after),
            subject_public_key_info: None,
            extensions: self.extensions,
            state: PhantomData,
        })
    }
}

impl CertificateBuilder<NeedsPublicKey> {
    /// Supplies the subject public key and advances to the signable state.
    #[must_use]
    pub fn subject_public_key_info(
        self,
        subject_public_key_info: SubjectPublicKeyInfoDer,
    ) -> CertificateBuilder<ReadyToSign> {
        CertificateBuilder {
            profile: self.profile,
            serial: self.serial,
            issuer: self.issuer,
            subject: self.subject,
            not_before: self.not_before,
            not_after: self.not_after,
            subject_public_key_info: Some(subject_public_key_info),
            extensions: self.extensions,
            state: PhantomData,
        }
    }
}

impl CertificateBuilder<ReadyToSign> {
    /// Replaces the guided profile's key-usage extension, or adds it to a custom profile.
    ///
    /// # Errors
    ///
    /// The set is empty, contradicts the selected CA/end-entity profile, or DER construction
    /// fails.
    pub fn key_usage(mut self, usages: KeyUsages) -> super::Result<Self> {
        if (self.profile == Profile::CertificateAuthority
            && !usages.contains(KeyUsages::KEY_CERT_SIGN))
            || (self.profile == Profile::EndEntity && usages.contains(KeyUsages::KEY_CERT_SIGN))
        {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        self.replace_extension(ExtensionDer::key_usage(usages, true)?);
        Ok(self)
    }

    /// Adds a typed subject-alternative-name extension.
    ///
    /// The extension becomes critical automatically when the subject name is empty, as required
    /// by RFC 5280.
    ///
    /// # Errors
    ///
    /// A value is malformed or the extension is already present.
    pub fn subject_alt_names(mut self, names: &[SubjectAltName<'_>]) -> super::Result<Self> {
        let extension = ExtensionDer::subject_alt_names(names, self.subject.empty)?;
        self.add_extension(extension)?;
        Ok(self)
    }

    /// Adds a typed extended-key-usage extension.
    ///
    /// # Errors
    ///
    /// The purpose list is empty/duplicated or the extension is already present.
    pub fn extended_key_usage(mut self, purposes: &[ExtendedKeyPurpose]) -> super::Result<Self> {
        self.add_extension(ExtensionDer::extended_key_usage(purposes, false)?)?;
        Ok(self)
    }

    /// Adds a subject-key identifier.
    ///
    /// # Errors
    ///
    /// The identifier is empty or the extension is already present.
    pub fn subject_key_identifier(mut self, identifier: &[u8]) -> super::Result<Self> {
        self.add_extension(ExtensionDer::subject_key_identifier(identifier, false)?)?;
        Ok(self)
    }

    /// Adds an authority-key identifier containing the issuer key identifier.
    ///
    /// # Errors
    ///
    /// The identifier is empty or the extension is already present.
    pub fn authority_key_identifier(mut self, identifier: &[u8]) -> super::Result<Self> {
        self.add_extension(ExtensionDer::authority_key_identifier(identifier, false)?)?;
        Ok(self)
    }

    /// Adds a caller-constructed extension without replacing an existing OID.
    ///
    /// This is the explicit extension escape hatch. Critical unknown extensions remain
    /// fail-closed in `rsl-pki` validation.
    ///
    /// # Errors
    ///
    /// The object identifier is already present.
    pub fn raw_extension(mut self, extension: ExtensionDer) -> super::Result<Self> {
        self.add_extension(extension)?;
        Ok(self)
    }

    fn add_extension(&mut self, extension: ExtensionDer) -> super::Result<()> {
        if self
            .extensions
            .iter()
            .any(|known| known.oid == extension.oid)
        {
            return Err(Error::new(ErrorKind::InvalidExtension));
        }
        self.extensions.push(extension);
        Ok(())
    }

    fn replace_extension(&mut self, extension: ExtensionDer) {
        if let Some(existing) = self
            .extensions
            .iter_mut()
            .find(|known| known.oid == extension.oid)
        {
            *existing = extension;
        } else {
            self.extensions.push(extension);
        }
    }

    /// Builds the exact `TBSCertificate` bytes for a caller-selected algorithm.
    ///
    /// This staged form supports hardware, remote, and experimental signers without letting them
    /// rewrite the already constructed signed fields.
    ///
    /// # Errors
    ///
    /// A guided end-entity profile has an empty subject without subject alternatives, or DER
    /// construction fails.
    pub fn build_tbs(
        self,
        signature_algorithm: SignatureAlgorithmDer,
    ) -> super::Result<TbsCertificateDer> {
        if self.profile == Profile::EndEntity
            && self.subject.empty
            && !self
                .extensions
                .iter()
                .any(|extension| extension.oid.is(oid::SUBJECT_ALT_NAME) && extension.critical)
        {
            return Err(Error::new(ErrorKind::InvalidName));
        }
        let not_before = self
            .not_before
            .ok_or_else(|| Error::new(ErrorKind::InvalidTime))?;
        let not_after = self
            .not_after
            .ok_or_else(|| Error::new(ErrorKind::InvalidTime))?;
        let spki = self
            .subject_public_key_info
            .ok_or_else(|| Error::new(ErrorKind::InvalidPublicKey))?;
        let version = built(|encoder| encoder.unsigned_integer(&[2]))?;
        let validity = built(|encoder| {
            encoder.sequence(|sequence| {
                encode_time(sequence, not_before)?;
                encode_time(sequence, not_after)
            })
        })?;
        let extensions = if self.extensions.is_empty() {
            None
        } else {
            let encoded_extensions = self
                .extensions
                .iter()
                .map(ExtensionDer::encoded)
                .collect::<super::Result<Vec<_>>>()?;
            Some(built(|encoder| {
                encoder.sequence(|sequence| {
                    for extension in &encoded_extensions {
                        sequence.encoded(extension)?;
                    }
                    Ok(())
                })
            })?)
        };
        let bytes = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.element(Tag::context(0, true), &version)?;
                sequence.unsigned_integer(&self.serial)?;
                sequence.encoded(signature_algorithm.as_bytes())?;
                sequence.encoded(self.issuer.as_bytes())?;
                sequence.encoded(&validity)?;
                sequence.encoded(self.subject.as_bytes())?;
                sequence.encoded(spki.as_bytes())?;
                if let Some(extensions) = &extensions {
                    sequence.element(Tag::context(3, true), extensions)?;
                }
                Ok(())
            })
        })?;
        Ok(TbsCertificateDer {
            bytes,
            signature_algorithm,
        })
    }

    /// Builds, signs, and assembles one certificate with a caller-selected signer implementation.
    ///
    /// # Errors
    ///
    /// Construction errors are distinguished from the signer's own error type.
    pub fn sign<S: CertificateSigner>(
        self,
        signer: &S,
    ) -> core::result::Result<CertificateDer, CertificateSignError<S::Error>> {
        let algorithm = signer
            .signature_algorithm()
            .map_err(CertificateSignError::Build)?;
        let tbs = self
            .build_tbs(algorithm)
            .map_err(CertificateSignError::Build)?;
        let signature = signer
            .sign(tbs.as_bytes())
            .map_err(CertificateSignError::Signer)?;
        tbs.finish(&signature).map_err(CertificateSignError::Build)
    }
}

/// Generic certificate-signing contract.
///
/// Implementations choose an exact signature algorithm identifier and sign the immutable
/// `TBSCertificate` bytes supplied by the builder. `rsl-pki::issuance` provides adapters for the
/// signing keys implemented by `rsl-crypto`; external and experimental signers can implement this
/// trait without being treated as validation-policy defaults.
pub trait CertificateSigner {
    /// Signer-specific failure.
    type Error;

    /// Exact algorithm identifier to repeat inside and outside `TBSCertificate`.
    ///
    /// # Errors
    ///
    /// The signer cannot construct a canonical identifier.
    fn signature_algorithm(&self) -> super::Result<SignatureAlgorithmDer>;

    /// Signs the exact complete DER `TBSCertificate` element.
    ///
    /// # Errors
    ///
    /// The signer refuses or fails the operation.
    fn sign(&self, tbs_certificate: &[u8]) -> core::result::Result<Vec<u8>, Self::Error>;
}

/// Error from the combined build-and-sign convenience.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CertificateSignError<E> {
    /// Canonical certificate construction failed.
    Build(Error),
    /// The caller-selected signer failed.
    Signer(E),
}

impl<E: fmt::Display> fmt::Display for CertificateSignError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => write!(formatter, "certificate construction failed: {error}"),
            Self::Signer(error) => write!(formatter, "certificate signer failed: {error}"),
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for CertificateSignError<E> {}

/// Owned exact `TBSCertificate` bytes awaiting an externally produced signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TbsCertificateDer {
    bytes: Vec<u8>,
    signature_algorithm: SignatureAlgorithmDer,
}

impl TbsCertificateDer {
    /// Exact DER bytes to sign.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Assembles the signature and repeated algorithm identifier into a complete certificate.
    ///
    /// # Errors
    ///
    /// The signature is empty or final DER validation fails.
    pub fn finish(self, signature: &[u8]) -> super::Result<CertificateDer> {
        if signature.is_empty() {
            return Err(Error::new(ErrorKind::InvalidSignature));
        }
        let bytes = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.encoded(&self.bytes)?;
                sequence.encoded(self.signature_algorithm.as_bytes())?;
                sequence.bit_string(0, signature)
            })
        })?;
        CertificateDer::from_der(bytes)
    }
}

/// Owned exact DER for a constructed certificate.
///
/// This type proves canonical certificate syntax only. Trust still requires `rsl-pki` path
/// validation with external anchors, time, and policy inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateDer(Vec<u8>);

impl CertificateDer {
    /// Imports and owns exactly one strict DER certificate.
    ///
    /// # Errors
    ///
    /// Parsing or X.509 structural validation fails.
    pub fn from_der(bytes: Vec<u8>) -> super::Result<Self> {
        Certificate::from_der(&bytes)?;
        Ok(Self(bytes))
    }

    /// Borrows the exact complete certificate encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Parses a borrowed view of the constructed certificate.
    ///
    /// # Errors
    ///
    /// The owned invariant was violated by an internal defect.
    pub fn certificate(&self) -> super::Result<Certificate<'_>> {
        Certificate::from_der(&self.0)
    }

    /// Consumes the owner and returns the exact complete certificate encoding.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// Encodes fixed-width `r || s` as X.509's DER `ECDSA-Sig-Value`.
///
/// # Errors
///
/// The input is empty, has odd length, or DER construction fails.
pub fn ecdsa_signature_value(fixed: &[u8]) -> super::Result<Vec<u8>> {
    if fixed.is_empty() || fixed.len() % 2 != 0 {
        return Err(Error::new(ErrorKind::InvalidSignature));
    }
    let width = fixed.len() / 2;
    built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.unsigned_integer(&fixed[..width])?;
            sequence.unsigned_integer(&fixed[width..])
        })
    })
}

fn encode_time(encoder: &mut Encoder, time: Time) -> rsl_asn1::Result<()> {
    let mut bytes = [b'0'; 15];
    let (tag, start, year) = if time.year <= 2049 {
        (Tag::UTC_TIME, 2, time.year % 100)
    } else {
        (Tag::GENERALIZED_TIME, 0, time.year)
    };
    write_decimal(
        &mut bytes[start..start + if start == 0 { 4 } else { 2 }],
        year,
    );
    let mut offset = 4;
    for value in [
        u16::from(time.month),
        u16::from(time.day),
        u16::from(time.hour),
        u16::from(time.minute),
        u16::from(time.second),
    ] {
        write_decimal(&mut bytes[offset..offset + 2], value);
        offset += 2;
    }
    bytes[14] = b'Z';
    encoder.element(tag, &bytes[start..])
}

fn write_decimal(output: &mut [u8], mut value: u16) {
    for byte in output.iter_mut().rev() {
        *byte = b'0' + u8::try_from(value % 10).expect("a decimal digit fits in u8");
        value /= 10;
    }
}

fn object_identifier(arcs: &[u64]) -> super::Result<ObjectIdentifier> {
    ObjectIdentifier::from_arcs(arcs).map_err(Error::from)
}

fn has_duplicates<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

fn oid_der(arcs: &[u64]) -> super::Result<Vec<u8>> {
    let oid = object_identifier(arcs)?;
    built(|encoder| encoder.object_identifier(&oid))
}

fn built(build: impl FnOnce(&mut Encoder) -> rsl_asn1::Result<()>) -> super::Result<Vec<u8>> {
    let mut encoder = Encoder::new();
    build(&mut encoder)?;
    Ok(encoder.finish())
}
