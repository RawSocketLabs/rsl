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
    assert_eq!(range.lower.hz(), 1_000_000);
    assert_eq!(range.upper.hz(), 20_000_000);
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
            assert_eq!(ranges[0].lower.hz(), 1_000_000);
            assert_eq!(ranges[1].upper.hz(), 520_000_000);
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
            max: u32::MAX as u64,
            unit: "Hz",
        }
    );
    assert_eq!(
        "18446744073709551616hz".parse::<Hertz>().unwrap_err(),
        ParseUnitError::OutOfRange {
            value: u64::MAX,
            min: 0,
            max: u64::MAX,
            unit: "Hz",
        }
    );
    assert_eq!(
        "4294967296".parse::<SampleRateSps>().unwrap_err(),
        ParseUnitError::OutOfRange {
            value: 4_294_967_296,
            min: 0,
            max: u32::MAX as u64,
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
    assert!(parse_frequency_ranges("450m").is_err());
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
            assert_eq!(ranges[0].lower.hz(), 400_000_000);
            assert_eq!(ranges[0].upper.hz(), 520_000_000);
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
