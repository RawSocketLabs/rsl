use serde::de::{self, DeserializeOwned, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use crate::types::{
    FrequencyHz, FrequencyRange, FrequencyRanges, Hertz, SampleRateSps, ScanTarget,
};

impl Serialize for Hertz {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.hz())
    }
}

impl<'de> Deserialize<'de> for Hertz {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_scalar(deserializer)
    }
}

impl Serialize for FrequencyHz {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.hz())
    }
}

impl<'de> Deserialize<'de> for FrequencyHz {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_scalar(deserializer)
    }
}

impl Serialize for FrequencyRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Range {
            lower: u64,
            upper: u64,
        }

        Range {
            lower: self.lower().hz(),
            upper: self.upper().hz(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FrequencyRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Explicit `{ lower, upper }` form; the only form binary formats carry.
        #[derive(Deserialize)]
        struct Endpoints {
            lower: FrequencyHz,
            upper: FrequencyHz,
        }

        /// Human-readable forms: a range string or explicit endpoints.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            String(String),
            Object(Endpoints),
        }

        let Endpoints { lower, upper } = if deserializer.is_human_readable() {
            match Repr::deserialize(deserializer)? {
                Repr::String(value) => {
                    return FrequencyRange::from_str(&value).map_err(de::Error::custom);
                }
                Repr::Object(endpoints) => endpoints,
            }
        } else {
            Endpoints::deserialize(deserializer)?
        };
        FrequencyRange::new(lower, upper).map_err(de::Error::custom)
    }
}

impl Serialize for FrequencyRanges {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FrequencyRanges {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ranges = Vec::<FrequencyRange>::deserialize(deserializer)?;
        FrequencyRanges::new(ranges).map_err(de::Error::custom)
    }
}

impl Serialize for SampleRateSps {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.sps())
    }
}

impl<'de> Deserialize<'de> for SampleRateSps {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_scalar(deserializer)
    }
}

/// Deserialize a scalar unit: the raw integer it serializes as in binary formats,
/// or an integer or human-readable string otherwise.
fn deserialize_scalar<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: NumberOrStringUnit,
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        deserializer.deserialize_any(NumberOrStringVisitor::<T>::new())
    } else {
        T::Raw::deserialize(deserializer).and_then(|raw| T::from_number(raw.into()))
    }
}

/// Scalar unit type that deserializes from either a raw integer or a human-readable string.
trait NumberOrStringUnit: Sized {
    /// Integer type the unit serializes as; read directly in binary formats.
    type Raw: DeserializeOwned + Into<u64>;

    /// `expecting` text shown when the input is neither an integer nor a string.
    const EXPECTING: &'static str;

    /// Build the value from a raw integer in the base unit.
    fn from_number<E>(value: u64) -> Result<Self, E>
    where
        E: de::Error;

    /// Build the value from a human-readable string.
    fn from_string<E>(value: &str) -> Result<Self, E>
    where
        E: de::Error;
}

/// Visitor that dispatches integers and strings to a [`NumberOrStringUnit`] type.
struct NumberOrStringVisitor<T>(PhantomData<T>);

impl<T> NumberOrStringVisitor<T> {
    /// Create the visitor for `T`.
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Visitor<'_> for NumberOrStringVisitor<T>
where
    T: NumberOrStringUnit,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(T::EXPECTING)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_number(value)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // Formats such as TOML carry every integer as i64.
        let value =
            u64::try_from(value).map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))?;
        T::from_number(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_string(value)
    }
}

impl NumberOrStringUnit for Hertz {
    type Raw = u64;

    const EXPECTING: &'static str = "a quantity as Hz number or human-readable string";

    fn from_number<E>(value: u64) -> Result<Self, E>
    where
        E: de::Error,
    {
        Ok(Hertz::new(value))
    }

    fn from_string<E>(value: &str) -> Result<Self, E>
    where
        E: de::Error,
    {
        Hertz::from_str(value).map_err(E::custom)
    }
}

impl NumberOrStringUnit for FrequencyHz {
    type Raw = u64;

    const EXPECTING: &'static str = "a frequency as Hz number or human-readable string";

    fn from_number<E>(value: u64) -> Result<Self, E>
    where
        E: de::Error,
    {
        Ok(FrequencyHz::new(value))
    }

    fn from_string<E>(value: &str) -> Result<Self, E>
    where
        E: de::Error,
    {
        FrequencyHz::from_str(value).map_err(E::custom)
    }
}

impl NumberOrStringUnit for SampleRateSps {
    type Raw = u32;

    const EXPECTING: &'static str = "a sample rate as S/s number or human-readable string";

    fn from_number<E>(value: u64) -> Result<Self, E>
    where
        E: de::Error,
    {
        let value = u32::try_from(value).map_err(E::custom)?;
        Ok(SampleRateSps::new(value))
    }

    fn from_string<E>(value: &str) -> Result<Self, E>
    where
        E: de::Error,
    {
        SampleRateSps::from_str(value).map_err(E::custom)
    }
}

impl Serialize for ScanTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ScanTarget::Static(freq) => {
                #[derive(Serialize)]
                struct Static {
                    static_frequency: u64,
                }
                Static {
                    static_frequency: freq.hz(),
                }
                .serialize(serializer)
            }
            ScanTarget::Ranges(ranges) => {
                #[derive(Serialize)]
                struct Ranges<'a> {
                    ranges: &'a [FrequencyRange],
                }
                Ranges {
                    ranges: ranges.as_slice(),
                }
                .serialize(serializer)
            }
        }
    }
}

// `ScanTarget` stays untagged in every format, so it needs a self-describing
// one (JSON, TOML, CBOR); bincode-style formats cannot carry it.
impl<'de> Deserialize<'de> for ScanTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            String(String),
            Static { static_frequency: FrequencyHz },
            Ranges { ranges: FrequencyRanges },
        }

        match Repr::deserialize(deserializer)? {
            Repr::String(value) => ScanTarget::from_str(&value).map_err(de::Error::custom),
            Repr::Static { static_frequency } => Ok(ScanTarget::Static(static_frequency)),
            Repr::Ranges { ranges } => Ok(ScanTarget::Ranges(ranges)),
        }
    }
}
