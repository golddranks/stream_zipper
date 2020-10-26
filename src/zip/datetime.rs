use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::ZipError;

use nom::bits::{bits, streaming::take};
use nom::error::ErrorKind;
use nom::number::streaming::le_u16;
use nom::sequence::{pair, tuple};
use nom::IResult;
use utils::{NomErrorExt, NomErrorExt2};

const SEC: Duration = Duration::from_secs(1);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

// Days in month for a non-leap year
const DAYS_IN_MONTH: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

// The accumulated days in a year, at month granularity
const DAY_OF_YEAR: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

// Difference between the UNIX and MS-DOS epochs
const EPOCH_DIFF: Duration = Duration::from_secs(3652 * 24 * 60 * 60);

/// Returns full days since the MS-DOS epoch (1980-01-01T00:00:00Z).
/// Doesn't account for leap seconds, so a full day is always 86400 seconds.
/// The parameter for year is zero-based and for month and day, one-based:
/// msdos_year: 0 - 127 (years 1980 - 2107)
/// month: 1 - 12
/// day: days 1 - days in month
fn days_since_msdos_epoch(msdos_year: u16, month: u16, day: u16) -> Result<u16, ()> {
    // Checking the inputs
    if msdos_year >= 128 {
        return Err(());
    }
    if month == 0 || month > 12 {
        return Err(());
    }
    if day == 0 {
        return Err(());
    }

    // Convert to zero-based indices and gregorian year
    let month = month as usize - 1;
    let day = day - 1;
    let year = 1980 + msdos_year;

    // As a base rule, the leap day happens on 29th of February, every 4th year.
    let is_leap_day = (year % 4) == 0 && month == 1 && day == 28;

    // Every 100th year is exceptionally a non-leap year,
    // but year 2000, even more exceptionally, IS a leap year.
    // (the 400th year rule)
    // This means that the representable years contain only one
    // exceptionally skipped year: 2100.
    // The 29th of February 2100 is not a leap day, it's an invalid date
    if is_leap_day && year == 2100 {
        return Err(());
    }

    // If the current day happens to be leap day,
    // add one to the days in month
    // to account for the 29th of February
    let leap_month_correction = if is_leap_day { 1 } else { 0 };

    // Sanity checking the day number
    if day >= DAYS_IN_MONTH[month] + leap_month_correction {
        return Err(());
    }

    // Performing date calculation

    // Check if the leap has already happened
    // during the current 4 year cycle.
    let is_after_leap = (year % 4) > 0 || month > 1;
    let after_leap_correction = if is_after_leap { 1 } else { 0 };

    // Check if the year 2100 skip has already happened
    let is_after_skip = year > 2100 || (year == 2100 && is_after_leap);
    let skip_leap_correction = if is_after_skip { 1 } else { 0 };

    // Calculating the number of leap days since epoch:
    // past full 4 year cycles
    // plus the possible leap day in the current cycle
    // minus the possible skipped leap day on year 2100.
    let leap_days_since_epoch = (msdos_year / 4) + after_leap_correction - skip_leap_correction;

    // Calculating days since epoch as if leap days didn't exist
    let non_leap_days_since_epoch = msdos_year * 365 + DAY_OF_YEAR[month] + day;

    Ok(non_leap_days_since_epoch + leap_days_since_epoch)
}

/// Loops through and tests every date MS-DOS time stamps support
#[test]
fn test_days_since_msdos_epoch() {
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let days_in_month_leap = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap_cycle = [
        days_in_month_leap,
        days_in_month,
        days_in_month,
        days_in_month,
    ];
    let mut year_accu = 0;
    let mut day_accu = 0;

    // Covers the entire span of dates: 1980 - 2107 as 32 "leap cycles"
    for _ in 0..32 {
        for mut year in leap_cycle.iter() {
            if year_accu == 120 {
                // The year 2100 is exceptionally not a leap year!
                year = &days_in_month;
            }
            for (month, month_days) in year.iter().enumerate() {
                let month = month as u16 + 1;
                for day in 1..=*month_days {
                    assert_eq!(days_since_msdos_epoch(year_accu, month, day), Ok(day_accu));
                    day_accu += 1;
                }
                // Test errors: days over the monthly number of days
                for day in (*month_days + 1)..32 {
                    assert_eq!(days_since_msdos_epoch(year_accu, month, day), Err(()));
                }
            }
            // Test errors: months over December
            for month in 13..16 {
                for day in 0..32 {
                    assert_eq!(days_since_msdos_epoch(year_accu, month, day), Err(()));
                }
            }
            year_accu += 1;
        }
    }
}
/// TODO test this
pub fn parse_msdos_date(i: &[u8]) -> IResult<&[u8], u16, ZipError> {
    let (i, (days, months, years)) = bits::<_, _, ((&[u8], usize), ErrorKind), _, _>(tuple((
        take(5_usize),
        take(4_usize),
        take(7_usize),
    )))(i)
    .map_nom_err(|_: (&[u8], ErrorKind)| ZipError::InvalidDateOrTime)?;
    let epoch_days =
        days_since_msdos_epoch(years, months, days).nom_fail(|_| ZipError::InvalidDateOrTime)?;
    Ok((i, epoch_days))
}

/// Parses a date out of a MS-DOS date format.
/// The output is a 3-tuple of:
/// (years since epoch, months, days)
/// The format allows representing a date between 1980-01-01 - 2107-12-31.
/// Format explanation:
/// The data is a 16-bit word, where bit number 0 is the least significant bit.
/// Bits 0 .. 4: day of the month (valid range: 1..31 inclusive)
/// Bits 5 .. 8: month of the year (valid range: 1..12 inclusive)
/// Bits 9 .. 15: count of years from 1980 (valid range: 0..127 inclusive)
pub fn parse_msdos_date_bits(i: u16) -> (u16, u16, u16) {
    let days = 0b00000000_00011111 & i;
    let months = (0b00000001_11100000 & i) >> 5;
    let years = (0b11111110_00000000 & i) >> 9;
    (years, months, days)
}

#[test]
fn test_parse_msdos_date_bits() {
    fn test(input: u16, years: u16, months: u16, days: u16) {
        assert_eq!(parse_msdos_date_bits(input), (years, months, days));
    }
    test(0b00000000_00000000_u16, 0, 0, 0); // Min representable invalid date
    test(0b00000000_00000001_u16, 0, 0, 1); // Invalid
    test(0b00000000_00100000_u16, 0, 1, 0); // Invalid
    test(0b00000000_00100001_u16, 0, 1, 1); // Min valid date
    test(0b00000000_00111110_u16, 0, 1, 30);
    test(0b00000000_00111111_u16, 0, 1, 31);
    test(0b00000001_10000001_u16, 0, 12, 1);
    test(0b00000010_00000000_u16, 1, 0, 0); // Invalid
    test(0b00000010_00100001_u16, 1, 1, 1);
    test(0b10000001_00010000_u16, 64, 8, 16);
    test(0b11111110_00000000_u16, 127, 0, 0); // Invalid
    test(0b11111110_00100001_u16, 127, 1, 1);
    test(0b11111111_10011111_u16, 127, 12, 31); // Max valid date
    test(0b11111111_11111111_u16, 127, 15, 31); // Max representable invalid date
}

fn seconds_since_midnight(hours: u16, minutes: u16, seconds: u16) -> Result<u32, ()> {
    if hours >= 24 {
        return Err(());
    }
    if minutes >= 60 {
        return Err(());
    }
    if seconds >= 60 {
        return Err(());
    }
    let total_minutes = minutes as u32 + hours as u32 * 60;
    let total_seconds = seconds as u32 + total_minutes * 60;
    Ok(total_seconds)
}

/// Loop all the times of day
#[test]
fn test_seconds_since_midnight() {
    let mut seconds_accu = 0;
    for hour in 0..24 {
        for minute in 0..60 {
            for sec in 0..60 {
                assert_eq!(seconds_since_midnight(hour, minute, sec), Ok(seconds_accu));
                seconds_accu += 1;
            }
            for sec in 60..64 {
                assert_eq!(seconds_since_midnight(hour, minute, sec), Err(()));
            }
        }
        for minute in 60..64 {
            for sec in 60..64 {
                assert_eq!(seconds_since_midnight(hour, minute, sec), Err(()));
            }
        }
    }
    for hour in 24..32 {
        for minute in 0..64 {
            for sec in 0..64 {
                assert_eq!(seconds_since_midnight(hour, minute, sec), Err(()));
            }
        }
    }
}

pub fn parse_msdos_time(i: &[u8]) -> IResult<&[u8], u32, ZipError> {
    // TODO: send a PR to nom that implemenths ErrorConvert to () to make this bearable
    // TODO: the type inference breaks because of ErrorConvert provides too many degrees of freedom - what to do?
    let (i, (seconds, minutes, hours)) = bits::<_, _, ((&[u8], usize), ErrorKind), _, _>(tuple((
        take(5_usize),
        take(6_usize),
        take(5_usize),
    )))(i)
    .map_nom_err(|_: (&[u8], ErrorKind)| ZipError::InvalidDateOrTime)?;
    let seconds = seconds_since_midnight(seconds, minutes, hours)
        .nom_fail(|_| ZipError::InvalidDateOrTime)?;
    Ok((i, seconds))
}

pub fn parse_msdos_time_bits(i: u16) -> (u16, u16, u16) {
    let seconds = 0b00000000_00011111 & i;
    let minutes = (0b00000111_11100000 & i) >> 5;
    let hours = (0b11111000_00000000 & i) >> 11;
    (hours, minutes, seconds * 2)
}

#[test]
fn test_parse_msdos_time_bits() {
    fn test(input: u16, hours: u16, minutes: u16, seconds: u16) {
        assert_eq!(parse_msdos_time_bits(input), (hours, minutes, seconds));
    }
    test(0b00000000_00000000_u16, 0, 0, 0); // Min valid time
    test(0b00000000_00000001_u16, 0, 0, 2);
    test(0b00000000_00010000_u16, 0, 0, 32);
    test(0b00000000_00011101_u16, 0, 0, 58);
    test(0b00000000_00011110_u16, 0, 0, 60);
    test(0b00000000_00011111_u16, 0, 0, 62);
    test(0b00000000_00100000_u16, 0, 1, 0);
    test(0b00000000_01000000_u16, 0, 2, 0);
    test(0b00000100_00000000_u16, 0, 32, 0);
    test(0b00000111_01100000_u16, 0, 59, 0);
    test(0b00000111_10000000_u16, 0, 60, 0);
    test(0b00000111_11100000_u16, 0, 63, 0);
    test(0b00001000_00000000_u16, 1, 0, 0);
    test(0b10000000_00000000_u16, 16, 0, 0);
    test(0b10100000_00000000_u16, 20, 0, 0);
    test(0b10111000_00000000_u16, 23, 0, 0);
    test(0b11000000_00000000_u16, 24, 0, 0);
    test(0b00001000_00100001_u16, 1, 1, 2);
    test(0b10111111_01111101_u16, 23, 59, 58); // Max valid time
    test(0b11111111_11111111_u16, 31, 63, 62); // Max representable invalid time
}

pub fn parse_msdos_datetime(i: &[u8]) -> IResult<&[u8], SystemTime, ZipError> {
    let (i, (msdos_time, msdos_date)) =
        pair(le_u16, le_u16)(i).map_nom_err(|()| ZipError::InvalidDateOrTime)?;
    let (hours, minutes, seconds) = parse_msdos_time_bits(msdos_time);
    let (years, months, days) = parse_msdos_date_bits(msdos_date);
    let seconds = seconds_since_midnight(hours, minutes, seconds)
        .nom_fail(|_| ZipError::InvalidDateOrTime)?;
    let epoch_days =
        days_since_msdos_epoch(years, months, days).nom_fail(|_| ZipError::InvalidDateOrTime)?;
    Ok((
        i,
        (UNIX_EPOCH + EPOCH_DIFF + DAY * epoch_days as u32 + SEC * seconds),
    ))
}

#[test]
fn test_parse_msdos_datetime() {
    assert_eq!(
        parse_msdos_datetime(b""),
        Err(nom::Err::Incomplete(nom::Needed::Size(2)))
    );
    assert_eq!(
        parse_msdos_datetime(b"\x10"),
        Err(nom::Err::Incomplete(nom::Needed::Size(2)))
    );
    assert_eq!(
        parse_msdos_datetime(b"\x10\x10"),
        Err(nom::Err::Incomplete(nom::Needed::Size(2)))
    );
    assert_eq!(
        parse_msdos_datetime(b"\x10\x10\x10"),
        Err(nom::Err::Incomplete(nom::Needed::Size(2)))
    );
    assert_eq!(
        parse_msdos_datetime(b"\x00\x00\x21\x00"),
        Ok((
            &[][..],
            SystemTime::UNIX_EPOCH + Duration::from_secs(315532800)
        ))
    );
}
