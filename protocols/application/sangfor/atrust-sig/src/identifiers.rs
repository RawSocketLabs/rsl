//! The identifiers a client presents (spec §3.8).

// --- Workspace dependencies ---
use base64::{Engine as _, engine::general_purpose::STANDARD};
use crypto::{RandomSource, digest::sha2::sha256::Sha256};
use crypto_legacy::digest::md5::Md5;

// --- Internal modules ---
use crate::{Error, hex};

/// A new device ID: 16 random bytes as 32 lowercase hexadecimal digits. A client makes one
/// once and keeps it.
///
/// # Errors
///
/// [`Error::Crypto`] when `random` fails.
pub fn device_id(random: &mut impl RandomSource) -> Result<String, Error> {
    let mut bytes = [0; 16];
    random.fill_bytes(&mut bytes)?;
    Ok(hex::encode(&bytes, false))
}

/// A new MAC address to report: random, marked locally administered and unicast, as
/// lowercase `aa:bb:cc:dd:ee:ff`. A client makes one once and keeps it.
///
/// # Errors
///
/// [`Error::Crypto`] when `random` fails.
pub fn mac_address(random: &mut impl RandomSource) -> Result<String, Error> {
    let mut bytes = [0; 6];
    random.fill_bytes(&mut bytes)?;
    bytes[0] = bytes[0] & 0xfe | 0x02;
    let octets: Vec<String> = bytes
        .iter()
        .map(|byte| hex::encode(&[*byte], false))
        .collect();
    Ok(octets.join(":"))
}

/// A trace ID for one request: 4 random bytes as 8 lowercase hexadecimal digits. It only
/// labels the request in the server's logs.
///
/// # Errors
///
/// [`Error::Crypto`] when `random` fails.
pub fn trace_id(random: &mut impl RandomSource) -> Result<String, Error> {
    let mut bytes = [0; 4];
    random.fill_bytes(&mut bytes)?;
    Ok(hex::encode(&bytes, false))
}

/// The connection ID for a session set up at `unix_micros`: the MD5 of the device ID in
/// uppercase hexadecimal, a hyphen, and the time in microseconds.
///
/// # Errors
///
/// Only if `device_id` is too long for MD5 to hash.
pub fn connection_id(device_id: &str, unix_micros: u64) -> Result<String, Error> {
    let digest = Md5::digest(device_id)?;
    Ok(format!(
        "{}-{unix_micros}",
        hex::encode(digest.as_bytes(), true)
    ))
}

/// The value of the `x-sdp-rid` header: the base host, `host` or `host:port` exactly as used
/// in the base URL, in standard base64.
#[must_use]
pub fn rid(base_host: &str) -> String {
    STANDARD.encode(base_host)
}

/// The value of the `x-sdp-env` header: a one-member JSON object naming the device, in
/// standard base64.
#[must_use]
pub fn env_header(device_id: &str) -> String {
    let object = serde_json::json!({ "deviceId": device_id });
    STANDARD.encode(object.to_string())
}

/// The fingerprint a client reports for the process a tunnel is opened for: the SHA-256 of
/// its path, in uppercase hexadecimal.
///
/// # Errors
///
/// Only if `path` is too long for SHA-256 to hash.
pub fn proc_hash(path: &str) -> Result<String, Error> {
    Ok(hex::encode(Sha256::digest(path)?.as_bytes(), true))
}
