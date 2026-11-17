//! How the members of an answer are read: what a gateway may leave out, and what it may not
//! substitute.

// --- Standard library ---
use std::fmt;
use std::marker::PhantomData;

// --- Workspace dependencies ---
use serde::de::{DeserializeOwned, Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

/// The most entries a list in an answer may have. Real lists hold a handful; the bound keeps
/// an answer of many empty entries from costing far more memory than its text.
pub(super) const MAX_ENTRIES: usize = 256;

/// Read `json` as a `T`, refusing anything but a JSON object. A derived reader would
/// otherwise take an array for a struct, member by position.
pub(super) fn from_object<'a, T: Deserialize<'a>>(json: &'a str) -> Result<T, serde_json::Error> {
    if !json.trim_start().starts_with('{') {
        return Err(serde_json::Error::custom("expected a JSON object"));
    }
    serde_json::from_str(json)
}

/// A scalar member: null reads as the type's default, like an absent member.
pub(super) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

/// An object member: null reads as the type's default, and anything but an object is an
/// error.
pub(super) fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    match Option::<Box<RawValue>>::deserialize(deserializer)? {
        Some(raw) => from_object(raw.get()).map_err(D::Error::custom),
        None => Ok(T::default()),
    }
}

/// A member that is a list of objects: null reads as no entries, an entry that is not an
/// object is an error, and so is a list of more than [`MAX_ENTRIES`].
pub(super) fn objects<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    /// Reads the list entry by entry, so that the bound applies before the entries are held.
    struct Entries<T>(PhantomData<T>);

    impl<'de, T: DeserializeOwned> Visitor<'de> for Entries<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("null or a list of objects")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut list: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(raw) = list.next_element::<Box<RawValue>>()? {
                if entries.len() == MAX_ENTRIES {
                    return Err(A::Error::custom("the list has too many entries"));
                }
                entries.push(from_object(raw.get()).map_err(A::Error::custom)?);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_any(Entries(PhantomData))
}
