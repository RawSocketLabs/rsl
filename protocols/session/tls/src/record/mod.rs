//! Record protection: turning a record's plaintext into its protected fragment and back.

mod aes128_gcm_protection;
mod error;
mod traffic_keys;

pub use aes128_gcm_protection::Aes128GcmProtection;
pub use error::RecordError;
pub use traffic_keys::TrafficKeys;
