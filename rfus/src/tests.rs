use super::*;

#[test]
fn parses_frequency_suffixes_case_insensitively() {
    assert_eq!("1mhz".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
    assert_eq!("1 MHz".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
    assert_eq!("1M".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
    assert_eq!("1.5MHz".parse::<FrequencyHz>().unwrap().hz(), 1_500_000);
    assert_eq!("450.5m".parse::<FrequencyHz>().unwrap().hz(), 450_500_000);
    assert_eq!("1_000_000".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
    assert_eq!("7g".parse::<FrequencyHz>().unwrap().hz(), 7_000_000_000);
    assert!("2msps".parse::<FrequencyHz>().is_err());
}

#[test]
fn parses_sample_rate_suffixes() {
    assert_eq!("2msps".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
    assert_eq!("2 MS/s".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
    assert_eq!("2mhz".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
    assert_eq!("500ksps".parse::<SampleRateSps>().unwrap().sps(), 500_000);
    assert_eq!(
        "1_000_000".parse::<SampleRateSps>().unwrap().sps(),
        1_000_000
    );
}

#[test]
fn parses_unbounded_hertz_for_bandwidths() {
    assert_eq!("12.5khz".parse::<Hertz>().unwrap().hz(), 12_500);
    assert_eq!(parse_hertz_u32("12500").unwrap(), 12_500);
}

#[test]
fn parses_frequency_ranges() {
    let range = "1mhz-20mhz".parse::<FrequencyRange>().unwrap();
    assert_eq!(range.lower().hz(), 1_000_000);
    assert_eq!(range.upper().hz(), 20_000_000);
    assert_eq!(
        "1m,2m".parse::<FrequencyRange>().unwrap().canonical(),
        "1000000,2000000"
    );
    assert_eq!(
        "400_000_000, 520_000_000"
            .parse::<FrequencyRange>()
            .unwrap()
            .canonical(),
        "400000000,520000000"
    );
}

#[test]
fn parses_scan_targets() {
    assert!(matches!(
        "450m".parse::<ScanTarget>().unwrap(),
        ScanTarget::Static(_)
    ));

    match "1m-2m; 400m-520m".parse::<ScanTarget>().unwrap() {
        ScanTarget::Ranges(ranges) => {
            assert_eq!(ranges.len(), 2);
            assert_eq!(ranges[0].lower().hz(), 1_000_000);
            assert_eq!(ranges[1].upper().hz(), 520_000_000);
        }
        ScanTarget::Static(_) => panic!("expected ranges"),
    }
}

#[test]
fn rejects_invalid_ranges() {
    assert_eq!(
        "2m-1m".parse::<FrequencyRange>().unwrap_err(),
        ParseUnitError::InvalidRange {
            lower: 2_000_000,
            upper: 1_000_000,
        }
    );
    assert!("1m".parse::<FrequencyRange>().is_err());
    assert_eq!(
        "1m-1m".parse::<FrequencyRange>().unwrap_err(),
        ParseUnitError::InvalidRange {
            lower: 1_000_000,
            upper: 1_000_000,
        }
    );
}

#[test]
fn reports_specific_scalar_errors() {
    assert_eq!(
        "2msps".parse::<FrequencyHz>().unwrap_err(),
        ParseUnitError::UnknownUnit("msps".to_string())
    );
    assert_eq!(
        "2widgets".parse::<FrequencyHz>().unwrap_err(),
        ParseUnitError::UnknownUnit("widgets".to_string())
    );
    assert_eq!(
        "1.0000001hz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::NonInteger("1.0000001".to_string())
    );
    assert_eq!(
        "_mhz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::InvalidNumber("_".to_string())
    );
}

#[test]
fn requires_semicolons_between_ranges() {
    assert!("1m-2m 3m-4m".parse::<ScanTarget>().is_err());

    match "1m-2m;".parse::<ScanTarget>().unwrap() {
        ScanTarget::Ranges(ranges) => assert_eq!(ranges.len(), 1),
        ScanTarget::Static(_) => panic!("expected ranges"),
    }
}

#[test]
fn accepts_whitespace_around_numbers_units_and_separators() {
    assert_eq!(
        "\t2 MS/s\n".parse::<SampleRateSps>().unwrap().sps(),
        2_000_000
    );
    assert_eq!(
        " 1 MHz , 2 MHz "
            .parse::<FrequencyRange>()
            .unwrap()
            .canonical(),
        "1000000,2000000"
    );
}

#[test]
fn validates_underscore_and_decimal_literals() {
    assert_eq!("1_000.5khz".parse::<Hertz>().unwrap().hz(), 1_000_500);
    assert_eq!("0.2MS/s".parse::<SampleRateSps>().unwrap().sps(), 200_000);
    assert_eq!(
        "1__000mhz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::InvalidNumber("1__000".to_string())
    );
    assert_eq!(
        "1_mhz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::InvalidNumber("1_".to_string())
    );
}

#[test]
fn reports_overflow_and_u32_bounds() {
    assert_eq!(
        parse_hertz_u32("4294967296hz").unwrap_err(),
        ParseUnitError::OutOfRange {
            value: 4_294_967_296,
            min: 0,
            max: u64::from(u32::MAX),
            unit: "Hz",
        }
    );
    assert_eq!(
        "18446744073709551616hz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::Overflow {
            value: "18446744073709551616".to_string(),
            unit: "Hz",
        }
    );
    assert_eq!(
        "18446744073709552k".parse::<FrequencyHz>().unwrap_err(),
        ParseUnitError::Overflow {
            value: "18446744073709552".to_string(),
            unit: "Hz",
        },
        "overflow must report the literal, not a fabricated value"
    );
    assert_eq!(
        "4294967296".parse::<SampleRateSps>().unwrap_err(),
        ParseUnitError::OutOfRange {
            value: 4_294_967_296,
            min: 0,
            max: u64::from(u32::MAX),
            unit: "S/s",
        }
    );
}

#[test]
fn public_parse_helpers_match_from_str() {
    assert_eq!(parse_frequency_hz("450m").unwrap(), 450_000_000);
    assert_eq!(parse_sample_rate_sps("2msps").unwrap(), 2_000_000);
    assert_eq!(
        parse_frequency_range("1m-2m").unwrap().canonical(),
        "1000000,2000000"
    );
    assert_eq!(parse_frequency_ranges("1m-2m;3m-4m").unwrap().len(), 2);
    assert_eq!(
        parse_frequency_ranges("450m").unwrap_err(),
        ParseUnitError::NotARange(450_000_000)
    );
}

#[cfg(feature = "serde")]
#[test]
fn serde_accepts_dense_and_explicit_scan_targets() {
    assert!(matches!(
        serde_json::from_str::<ScanTarget>(r#""450m""#).unwrap(),
        ScanTarget::Static(_)
    ));
    match serde_json::from_str::<ScanTarget>(
        r#"{ "ranges": [{ "lower": "400m", "upper": "520m" }] }"#,
    )
    .unwrap()
    {
        ScanTarget::Ranges(ranges) => {
            assert_eq!(ranges[0].lower().hz(), 400_000_000);
            assert_eq!(ranges[0].upper().hz(), 520_000_000);
        }
        ScanTarget::Static(_) => panic!("expected ranges"),
    }
}

#[cfg(feature = "serde")]
#[test]
fn serde_checks_numeric_boundaries() {
    assert_eq!(
        serde_json::from_str::<SampleRateSps>("200000")
            .unwrap()
            .sps(),
        200_000
    );
    assert!(serde_json::from_str::<SampleRateSps>("4294967296").is_err());
    assert!(
        serde_json::from_str::<FrequencyRange>(r#"{ "lower": 2000000, "upper": 1000000 }"#)
            .is_err()
    );
}

/// Serde behaviour that depends on the data format: TOML-style signed integers
/// and non-self-describing binary formats.
#[cfg(feature = "serde")]
mod serde_formats {
    use serde::de::value::{Error, I64Deserializer, SeqDeserializer};
    use serde::de::{self, IntoDeserializer, Visitor};
    use serde::{Deserialize, Deserializer, forward_to_deserialize_any};

    use super::*;

    #[test]
    fn accepts_signed_integers_from_toml_style_formats() {
        let frequency: I64Deserializer<Error> = 450_000_000_i64.into_deserializer();
        assert_eq!(
            FrequencyHz::deserialize(frequency).unwrap().hz(),
            450_000_000
        );

        let rate: I64Deserializer<Error> = 2_000_000_i64.into_deserializer();
        assert_eq!(SampleRateSps::deserialize(rate).unwrap().sps(), 2_000_000);

        let negative: I64Deserializer<Error> = (-1_i64).into_deserializer();
        assert!(
            FrequencyHz::deserialize(negative).is_err(),
            "negative integers must not wrap into a frequency"
        );
    }

    #[test]
    fn reads_scalars_and_ranges_from_binary_formats() {
        let frequency = FrequencyHz::deserialize(BinaryFormat(vec![450_000_000])).unwrap();
        assert_eq!(frequency.hz(), 450_000_000);

        let rate = SampleRateSps::deserialize(BinaryFormat(vec![2_000_000])).unwrap();
        assert_eq!(rate.sps(), 2_000_000);

        let range = FrequencyRange::deserialize(BinaryFormat(vec![1_000_000, 2_000_000])).unwrap();
        assert_eq!(
            (range.lower().hz(), range.upper().hz()),
            (1_000_000, 2_000_000)
        );

        assert!(
            FrequencyRange::deserialize(BinaryFormat(vec![2_000_000, 1_000_000])).is_err(),
            "binary input must still enforce lower < upper"
        );
    }

    /// Stand-in for a non-self-describing binary format such as bincode: it is
    /// not human-readable, rejects `deserialize_any`, and yields structs as
    /// sequences.
    struct BinaryFormat(Vec<u64>);

    impl<'de> Deserializer<'de> for BinaryFormat {
        type Error = Error;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
            Err(de::Error::custom(
                "binary formats do not support deserialize_any",
            ))
        }

        fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_u64(self.0[0])
        }

        fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_u32(u32::try_from(self.0[0]).map_err(de::Error::custom)?)
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            _: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Error> {
            let fields = self.0.into_iter().map(|value| BinaryFormat(vec![value]));
            visitor.visit_seq(SeqDeserializer::new(fields))
        }

        fn is_human_readable(&self) -> bool {
            false
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u128 f32 f64 char str string bytes byte_buf option
            unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
        }
    }

    impl IntoDeserializer<'_, Error> for BinaryFormat {
        type Deserializer = Self;

        fn into_deserializer(self) -> Self {
            self
        }
    }
}
