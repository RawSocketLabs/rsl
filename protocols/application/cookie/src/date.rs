//! Cookie dates.

/// Whether `byte` separates the tokens of a cookie date (RFC 6265 §5.1.1, `delimiter`).
const fn is_delimiter(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x20..=0x2f | 0x3b..=0x40 | 0x5b..=0x60 | 0x7b..=0x7e)
}

/// Reads between `min` and `max` leading digits of `token` as a number. What follows them
/// must be the end of the token or a non-digit.
fn leading_number(token: &[u8], min: usize, max: usize) -> Option<(u32, &[u8])> {
    let digits = token
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits < min || digits > max {
        return None;
    }
    let value = token[..digits]
        .iter()
        .fold(0, |value, digit| value * 10 + u32::from(digit - b'0'));
    Some((value, &token[digits..]))
}

/// Reads a token as `hours:minutes:seconds`.
fn time(token: &[u8]) -> Option<(u32, u32, u32)> {
    let (hours, rest) = leading_number(token, 1, 2)?;
    let (minutes, rest) = leading_number(rest.strip_prefix(b":")?, 1, 2)?;
    let (seconds, _) = leading_number(rest.strip_prefix(b":")?, 1, 2)?;
    Some((hours, minutes, seconds))
}

/// Reads a token as a month name, by its first three letters.
fn month(token: &[u8]) -> Option<u32> {
    const NAMES: [&[u8; 3]; 12] = [
        b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov",
        b"dec",
    ];
    let prefix = token.get(..3)?;
    let at = NAMES
        .iter()
        .position(|name| prefix.eq_ignore_ascii_case(*name))?;
    u32::try_from(at + 1).ok()
}

/// The number of days in `month` of `year`.
const fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Days from 1970-01-01 to the given date in the proleptic Gregorian calendar.
const fn days_from_epoch(year: i64, month: u32, day: u32) -> i64 {
    // Count from 1 March, so that the leap day is the last day of the counting year.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_from_march = (month as i64 + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Parses a cookie date into seconds since the Unix epoch, by the algorithm of RFC 6265
/// §5.1.1: the tokens may come in any order, and the first that fits each of time, day of
/// month, month and year is used. Returns `None` when a part is missing or out of range, or
/// the date does not exist.
///
/// The RFC's grammar writes the text after a number as required; like every browser, this
/// takes it as optional, without which the RFC's own examples would not parse.
//~ deviates rfc6265#5.1.1 reason="the trailing ( non-digit *OCTET ) of time, day-of-month and year is treated as optional; as written, the RFC's own example dates would not parse"
pub(crate) fn parse(date: &[u8]) -> Option<i64> {
    let (mut found_time, mut found_day, mut found_month, mut found_year) = (None, None, None, None);
    for token in date
        .split(|&byte| is_delimiter(byte))
        .filter(|token| !token.is_empty())
    {
        if found_time.is_none() && time(token).is_some() {
            found_time = time(token);
        } else if found_day.is_none() && leading_number(token, 1, 2).is_some() {
            found_day = leading_number(token, 1, 2).map(|(day, _)| day);
        } else if found_month.is_none() && month(token).is_some() {
            found_month = month(token);
        } else if found_year.is_none() && leading_number(token, 2, 4).is_some() {
            found_year = leading_number(token, 2, 4).map(|(year, _)| year);
        }
    }
    let ((hours, minutes, seconds), day, month) = (found_time?, found_day?, found_month?);
    let year = match i64::from(found_year?) {
        year @ 70..=99 => year + 1900,
        year @ 0..=69 => year + 2000,
        year => year,
    };
    let in_range = year >= 1601 && hours <= 23 && minutes <= 59 && seconds <= 59;
    if !in_range || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    let seconds_of_day = i64::from(hours * 3600 + minutes * 60 + seconds);
    Some(days_from_epoch(year, month, day) * 86_400 + seconds_of_day)
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Expected values from Python 3.15.2: `calendar.timegm(email.utils.parsedate(date))`.
    #[test]
    fn dates_match_an_independent_calendar() {
        let cases: [(&[u8], i64); 5] = [
            (b"Wed, 09 Jun 2021 10:18:14 GMT", 1_623_233_894),
            (b"Sun, 06 Nov 1994 08:49:37 GMT", 784_111_777),
            (b"Thu, 01 Jan 1970 00:00:00 GMT", 0),
            (b"Tue, 29 Feb 2000 23:59:59 GMT", 951_868_799),
            (b"Sat, 31 Dec 2022 23:59:59 GMT", 1_672_531_199),
        ];
        for (date, expected) in cases {
            assert_eq!(parse(date), Some(expected), "{}", date.escape_ascii());
        }
    }

    #[test]
    fn older_formats_and_token_orders_are_read_the_same_way() {
        let expected = Some(784_111_777);
        assert_eq!(
            parse(b"Sunday, 06-Nov-94 08:49:37 GMT"),
            expected,
            "RFC 850 style"
        );
        assert_eq!(
            parse(b"Sun Nov  6 08:49:37 1994"),
            expected,
            "asctime style"
        );
        assert_eq!(parse(b"08:49:37 1994 nov 6"), expected, "any order");
        assert_eq!(
            parse(b"6 NOVEMBER 1994 8:49:37"),
            expected,
            "long month, short hour"
        );
    }

    #[test]
    fn two_digit_years_pivot_at_seventy() {
        assert_eq!(parse(b"01 Jan 70 00:00:00"), Some(0));
        assert_eq!(parse(b"01 Jan 69 00:00:00"), parse(b"01 Jan 2069 00:00:00"));
    }

    #[test]
    fn dates_before_the_epoch_are_negative() {
        assert_eq!(parse(b"31 Dec 1969 23:59:59"), Some(-1));
    }

    #[test]
    fn every_delimiter_separates_tokens_and_nothing_else_does() {
        // RFC 6265 §5.1.1: tab, 0x20-0x2F, 0x3B-0x40, 0x5B-0x60, 0x7B-0x7E.
        for byte in 0..=u8::MAX {
            let listed =
                matches!(byte, 0x09 | 0x20..=0x2f | 0x3b..=0x40 | 0x5b..=0x60 | 0x7b..=0x7e);
            let date = [
                &b"06"[..],
                &[byte],
                b"Nov",
                &[byte],
                b"1994",
                &[byte],
                b"08:49:37",
            ]
            .concat();
            let parsed = parse(&date) == Some(784_111_777);
            // A digit or a colon would extend a neighbouring token rather than fail to split.
            if !byte.is_ascii_digit() && byte != b':' {
                assert_eq!(parsed, listed, "byte {byte:#04x}");
            }
        }
    }

    #[test]
    fn first_token_that_fits_each_part_is_the_one_used() {
        // Two times, two months, two years: the later ones are left over and ignored.
        let date = b"06 Nov 1994 08:49:37 23:59:59 Dec 2099";
        assert_eq!(parse(date), Some(784_111_777));
        // A second small number is not a day once one is found; it is read as the year.
        assert_eq!(parse(b"06 Nov 94 08:49:37"), Some(784_111_777));
    }

    #[test]
    fn range_limits_admit_their_last_valid_values() {
        assert!(
            parse(b"01 Jan 1601 00:00:00").is_some(),
            "the earliest year"
        );
        assert!(
            parse(b"01 Jan 2021 23:59:59").is_some(),
            "the last second of a day"
        );
        assert_eq!(parse(b"01 Jan 2021 00:00:60"), None, "no leap seconds");
        assert_eq!(parse(b"31 Nov 2021 00:00:00"), None, "November has 30 days");
        assert_eq!(
            parse(b"01 Jan 2021 000:00:00"),
            None,
            "at most two digits a field"
        );
        assert_eq!(parse(b"01 Jan 2021 00:00:000"), None);
    }

    #[test]
    fn missing_impossible_or_out_of_range_dates_do_not_parse() {
        let cases: [&[u8]; 10] = [
            b"",
            b"Wed, 09 Jun 2021",
            b"Wed, 09 2021 10:18:14 GMT",
            b"30 Feb 2021 00:00:00",
            b"29 Feb 2100 00:00:00",
            b"31 Apr 2021 00:00:00",
            b"00 Jan 2021 00:00:00",
            b"01 Jan 2021 24:00:00",
            b"01 Jan 2021 00:60:00",
            b"01 Jan 1600 00:00:00",
        ];
        for date in cases {
            assert_eq!(parse(date), None, "{}", date.escape_ascii());
        }
    }
}
