//! The text the anti-MITM signature covers.

// --- Standard library ---
use std::collections::BTreeMap;

// --- Workspace dependencies ---
use serde_json::value::RawValue;

// --- Internal modules ---
use crate::Error;

/// The member whose value is the signature itself, and so is not signed.
const SIGNATURE_MEMBER: &str = "mitmSig";

/// How deeply the object may nest. Real payloads have one level; the bound keeps a hostile
/// one from exhausting the stack or, since each level is parsed again, the clock.
const MAX_DEPTH: usize = 16;

/// Builds the canonical string of an `antiMITMAttackData` object, from its JSON text exactly
/// as received (spec §3.3).
///
/// Every member whose value is not an object or array becomes one `name:value` item, at any
/// depth; members named `mitmSig` are left out. A string contributes its decoded content, and
/// a number, boolean or null its text as received, so `1` stays `1`. Items are ordered by
/// name, bytewise, and joined with `&`.
///
/// Cases the specification leaves open are settled here in the simplest way. Real payloads
/// are flat, so none arises in practice; each is a question for the specification, not a
/// claim about the vendor's client:
///
/// - A scalar inside an array takes the name of the member holding the array.
/// - When a name occurs more than once, at any depth or in an array, the last occurrence in
///   document order is the one kept. So `"a":[1,2,3]` contributes only `a:3`.
/// - A member named `mitmSig` is left out whole, whatever it holds.
/// - The name `mitmSig` is matched exactly, letter case included.
/// - A string that is not valid JSON text, such as one with an unpaired surrogate escape,
///   makes the whole object malformed.
///
/// # Errors
///
/// [`Error::Malformed`] when `json` is not a JSON object, or nests deeper than
/// 16 levels: the object comes from a server that has not been
/// authenticated yet, and this is the function that helps decide whether to trust it.
pub fn canonical_string(json: &str) -> Result<String, Error> {
    let malformed = || Error::Malformed("anti-MITM data");
    let root: &RawValue = serde_json::from_str(json).map_err(|_| malformed())?;
    if !root.get().starts_with('{') {
        return Err(malformed());
    }
    let mut items = BTreeMap::new();
    collect(root, "", MAX_DEPTH, &mut items).ok_or_else(malformed)?;
    let items: Vec<String> = items
        .into_iter()
        .map(|(name, value)| format!("{name}:{value}"))
        .collect();
    Ok(items.join("&"))
}

/// Adds the scalars at and beneath `value` to `items`, under `name` where `value` is itself a
/// scalar or an array element. Returns `None` when the JSON is invalid or nests more than
/// `depth` levels below here.
fn collect(
    value: &RawValue,
    name: &str,
    depth: usize,
    items: &mut BTreeMap<String, String>,
) -> Option<()> {
    let text = value.get();
    let first = text.as_bytes().first();
    let deeper = match first {
        Some(b'{' | b'[') => depth.checked_sub(1)?,
        _ => depth,
    };
    match first {
        Some(b'{') => {
            // A vector of pairs keeps document order, so that a repeated name resolves the
            // same way every time.
            let members: Members<'_> = serde_json::from_str(text).ok()?;
            for (member, value) in &members.0 {
                if member != SIGNATURE_MEMBER {
                    collect(value, member, deeper, items)?;
                }
            }
        }
        Some(b'[') => {
            let elements: Vec<&RawValue> = serde_json::from_str(text).ok()?;
            for element in elements {
                collect(element, name, deeper, items)?;
            }
        }
        Some(b'"') => {
            items.insert(name.to_owned(), serde_json::from_str(text).ok()?);
        }
        _ => {
            items.insert(name.to_owned(), text.to_owned());
        }
    }
    Some(())
}

/// The members of a JSON object in document order, values unparsed.
struct Members<'a>(Vec<(String, &'a RawValue)>);

impl<'de: 'a, 'a> serde::Deserialize<'de> for Members<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<'a>(core::marker::PhantomData<&'a ()>);

        impl<'de: 'a, 'a> serde::de::Visitor<'de> for Visitor<'a> {
            type Value = Members<'a>;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry::<String, &'a RawValue>()? {
                    members.push(member);
                }
                Ok(Members(members))
            }
        }

        deserializer.deserialize_map(Visitor(core::marker::PhantomData))
    }
}
