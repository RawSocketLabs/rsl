//! Record protection: turning a record's plaintext into its protected fragment and back.

mod aes128_gcm_protection;
mod error;
mod read_state;
mod tls_plaintext;
mod traffic_keys;
mod write_state;

pub use aes128_gcm_protection::Aes128GcmProtection;
pub use error::RecordError;
pub use read_state::ReadState;
pub use tls_plaintext::TlsPlaintext;
pub use traffic_keys::TrafficKeys;
pub use write_state::WriteState;
