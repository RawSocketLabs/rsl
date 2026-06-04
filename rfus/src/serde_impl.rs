// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use crate::types::{FrequencyHz, FrequencyRange, SampleRateSps, ScanTarget};

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
        deserializer.deserialize_any(NumberOrStringVisitor::<FrequencyHz>::new())
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
            lower: self.lower.hz(),
            upper: self.upper.hz(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FrequencyRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            String(String),
            Object {
                lower: FrequencyHz,
                upper: FrequencyHz,
            },
        }

        match Repr::deserialize(deserializer)? {
            Repr::String(value) => FrequencyRange::from_str(&value).map_err(de::Error::custom),
            Repr::Object { lower, upper } => {
                FrequencyRange::new(lower, upper).map_err(de::Error::custom)
            }
        }
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
        deserializer.deserialize_any(NumberOrStringVisitor::<SampleRateSps>::new())
    }
}

trait NumberOrStringUnit: Sized {
    const EXPECTING: &'static str;

    fn from_number<E>(value: u64) -> Result<Self, E>
    where
        E: de::Error;

    fn from_string<E>(value: &str) -> Result<Self, E>
    where
        E: de::Error;
}

struct NumberOrStringVisitor<T>(PhantomData<T>);

impl<T> NumberOrStringVisitor<T> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, T> Visitor<'de> for NumberOrStringVisitor<T>
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

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_string(value)
    }
}

impl NumberOrStringUnit for FrequencyHz {
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
                Ranges { ranges }.serialize(serializer)
            }
        }
    }
}

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
            Ranges { ranges: Vec<FrequencyRange> },
        }

        match Repr::deserialize(deserializer)? {
            Repr::String(value) => ScanTarget::from_str(&value).map_err(de::Error::custom),
            Repr::Static { static_frequency } => Ok(ScanTarget::Static(static_frequency)),
            Repr::Ranges { ranges } => Ok(ScanTarget::Ranges(ranges)),
        }
    }
}
