//! Which authentication step comes next.

// --- Internal modules ---
use crate::api::NextStep;

/// The services whose kind a `auth/token` step is narrowed to by its list entry.
const TOKEN_KINDS: [&str; 3] = ["auth/totp", "auth/radius", "auth/challenge"];

/// The next step of the secondary chain, resolved from an answer's next-step fields
/// (spec §5.5).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    /// The step's service, such as `auth/sms`.
    pub service: String,
    /// The id the step's requests carry. Empty when the answer gave none.
    pub auth_id: String,
}

impl Step {
    /// Resolve an answer's next-step fields. `None` when the chain is complete.
    #[must_use]
    pub fn resolve(fields: &NextStep) -> Option<Self> {
        let named = fields.next_service.as_str();
        let entry = fields
            .next_service_list
            .iter()
            .find(|entry| entry.auth_type == named)
            .or(fields.next_service_list.first());
        let (auth_type, sub_type, auth_id) = entry.map_or(("", "", ""), |entry| {
            (
                entry.auth_type.as_str(),
                entry.sub_type.as_str(),
                entry.auth_id.as_str(),
            )
        });
        let service = match named {
            "" if auth_type.is_empty() && auth_id.is_empty() => return None,
            // The answer shape of older gateways: an id and no type.
            "" if auth_type.is_empty() => "auth/sms",
            "" => auth_type,
            "auth/token" if TOKEN_KINDS.contains(&auth_type) => auth_type,
            "auth/token" if auth_type == "auth/token" && sub_type == "totp" => "auth/totp",
            named => named,
        };
        let service = if service == "auth/sendSms" {
            "auth/sms"
        } else {
            service
        };
        Some(Self {
            service: service.to_owned(),
            auth_id: auth_id.to_owned(),
        })
    }
}
