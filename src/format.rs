//! Text for values that appear in several places of the interface.

use rust_i18n::t;
use std::time::Duration;

/// Human readable length of a span of time.
///
/// Below a second milliseconds, below a minute seconds with one decimal, above
/// it minutes and seconds.
///
/// A span shorter than a second is written in the unit it happened in, because
/// the one decimal of a second is not a reading of it: the wait between a line
/// being written to and the first byte back is tens of milliseconds on a
/// console and a fifth of a second on a slow line, and both of them come out as
/// nothing at all when the unit is a tenth of a second.
/// The unit is in the locale, because it is a word and not a sign: a second is
/// `s` in one language and `с` in another, and a number the interface writes in
/// the language of the interface with a unit it does not is a number written
/// half in each.
///
/// Minutes and seconds carry none: `10:05` is that everywhere, and a colon is
/// the unit.
pub fn duration(span: Duration) -> String {
    let seconds = span.as_secs_f64();
    if seconds < 1.0 {
        format!("{}{}", span.as_millis(), t!("format.milliseconds"))
    } else if seconds < 60.0 {
        format!("{seconds:.1}{}", t!("format.seconds"))
    } else {
        let minutes = span.as_secs() / 60;
        let rest = span.as_secs() % 60;
        format!("{minutes}:{rest:02}")
    }
}

/// A moment as the interface writes it: the day and the minute, in the time
/// zone of this machine.
///
/// Seconds are left out everywhere it is shown — the last connection of a
/// device, the last run of a command — because what they answer is "when",
/// which a minute already says.
pub fn time(at: jiff::Timestamp) -> String {
    let zoned = at.to_zoned(jiff::tz::TimeZone::system());
    jiff::fmt::strtime::format("%Y-%m-%d %H:%M", &zoned).unwrap_or_else(|_| at.to_string())
}

/// A moment as the status bar writes it: the second of the day, in the time
/// zone of this machine.
///
/// Seconds are in it where they are left out everywhere else, because what it
/// answers is how long ago a line last said anything — a question a minute is
/// too coarse for.
pub fn clock(at: jiff::Timestamp) -> String {
    let zoned = at.to_zoned(jiff::tz::TimeZone::system());
    jiff::fmt::strtime::format("%H:%M:%S", &zoned).unwrap_or_else(|_| at.to_string())
}

/// A moment with its day, for a plate that has room for the whole of it.
///
/// The day is left off when it is today. A window is looked at on the day it is
/// open, so the date of everything in it is the same date over and over, and a
/// number that never changes is a number the eye has to step over to reach the
/// one that does. A moment of another day keeps it, because then it is the
/// thing worth seeing.
pub fn moment(at: jiff::Timestamp) -> String {
    let zone = jiff::tz::TimeZone::system();
    let zoned = at.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date() == zoned.date();
    let shape = if today {
        "%H:%M:%S"
    } else {
        "%Y-%m-%d %H:%M:%S"
    };
    jiff::fmt::strtime::format(shape, &zoned).unwrap_or_else(|_| at.to_string())
}

/// The units a size is written in, smallest first, as the locale says them.
///
/// They are words like the units of a span: a byte is `B` in one language and
/// `Б` in another, and the multiples are read as the word they are built from.
pub const SIZE_UNITS: [&str; 5] = [
    "format.bytes",
    "format.kibibytes",
    "format.mebibytes",
    "format.gibibytes",
    "format.tebibytes",
];

/// Human readable size of a file, in the units a desktop uses.
pub fn size(bytes: u64) -> String {
    const STEP: f64 = 1024.0;

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= STEP && unit + 1 < SIZE_UNITS.len() {
        value /= STEP;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes}{}", t!(SIZE_UNITS[0]))
    } else {
        format!("{value:.1}{}", t!(SIZE_UNITS[unit]))
    }
}

/// Bytes that crossed a line, written so that the number is the one somebody
/// came to read.
///
/// Bytes up to [`VOLUME_STEP`] and kibibytes above it. A short answer is counted
/// in bytes because that is what it is — thirty-seven bytes are thirty-seven
/// bytes and not nought point nought kibibytes — and past a hundred kibibytes the
/// last three digits are noise.
///
/// It steps once and no further, so a session that carried a mebibyte reads as
/// the kibibytes it was. That is the rule this is asked for; [`size`] is the one
/// that steps all the way for the numbers that need it.
pub fn volume(bytes: u64) -> String {
    if bytes < VOLUME_STEP {
        return format!("{bytes}{}", t!(SIZE_UNITS[0]));
    }
    format!("{:.1}{}", bytes as f64 / 1024.0, t!(SIZE_UNITS[1]))
}

/// Where [`volume`] stops counting in bytes.
pub const VOLUME_STEP: u64 = 100 * 1024;

/// The unit a speed is named in: kibibytes a second, built from the words of
/// the size and of the span so that neither is written twice.
pub fn rate_unit() -> String {
    format!("{}{}", t!(SIZE_UNITS[1]), t!("format.per_second"))
}

/// How fast a source is talking, in the unit the settings name a speed in.
///
/// It stays kibibytes a second whatever the number: the ladder of the settings
/// is written in that unit, and a speed that changed its unit as it rose would
/// be a number nobody could read against the row it belongs to.
pub fn rate(kibibytes_per_second: f32) -> String {
    format!("{kibibytes_per_second:.1}{}", rate_unit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The day of a moment of today is the day the window is being looked at,
    /// so it says nothing and is left off; the day of any other moment is the
    /// thing worth seeing and stays.
    #[test]
    fn a_moment_of_today_is_written_without_its_day() {
        let now = moment(jiff::Timestamp::now());
        assert_eq!(now.matches(':').count(), 2, "the time of day: {now}");
        assert!(!now.contains('-'), "and nothing of the date: {now}");

        let yesterday = moment(jiff::Timestamp::now() - jiff::SignedDuration::from_hours(30));
        assert!(
            yesterday.contains('-') && yesterday.len() > now.len(),
            "another day carries its date: {yesterday}"
        );
    }

    /// The unit is a word of the locale, so what the test holds is the number
    /// and the unit it climbed to.
    fn sized(text: &str, unit: usize) -> String {
        format!("{text}{}", t!(SIZE_UNITS[unit]))
    }

    #[test]
    fn sizes_climb_through_the_units() {
        assert_eq!(size(0), sized("0", 0));
        assert_eq!(size(999), sized("999", 0));
        assert_eq!(size(1024), sized("1.0", 1));
        assert_eq!(size(1024 * 1024), sized("1.0", 2));
        assert_eq!(
            size(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024),
            sized("3.5", 3)
        );
    }

    /// Every unit a size climbs to is a key the locale carries, in both
    /// languages, because a size that reached one nobody translated would show
    /// the key to the reader.
    #[test]
    fn every_unit_of_a_size_is_said_in_both_languages() {
        for key in SIZE_UNITS {
            let english = t!(key, locale = "en");
            let russian = t!(key, locale = "ru");
            assert_ne!(english, key, "{key} is said in English");
            assert_ne!(russian, key, "{key} is said in Russian");
        }
    }

    /// The unit of a span is a word of the locale and the number is not, so
    /// what the tests hold is the number and the threshold it belongs to.
    fn milliseconds(count: u128) -> String {
        format!("{count}{}", t!("format.milliseconds"))
    }

    fn seconds(text: &str) -> String {
        format!("{text}{}", t!("format.seconds"))
    }

    /// A span of less than a second is written in milliseconds: a tenth of a
    /// second is not a unit a wait of forty milliseconds has a reading in.
    #[test]
    fn spans_below_a_second_are_written_in_milliseconds() {
        assert_eq!(duration(Duration::from_millis(0)), milliseconds(0));
        assert_eq!(duration(Duration::from_millis(42)), milliseconds(42));
        assert_eq!(duration(Duration::from_millis(999)), milliseconds(999));
    }

    #[test]
    fn short_spans_keep_one_decimal() {
        assert_eq!(duration(Duration::from_millis(1000)), seconds("1.0"));
        assert_eq!(duration(Duration::from_millis(1234)), seconds("1.2"));
        assert_eq!(duration(Duration::from_secs(59)), seconds("59.0"));
    }

    /// The unit follows the language of the interface, because a number
    /// written in one language with a unit of another is written half in each.
    #[test]
    fn the_unit_is_the_word_of_the_locale() {
        let english = t!("format.milliseconds", locale = "en");
        let russian = t!("format.milliseconds", locale = "ru");

        assert_ne!(english, russian, "the two languages say it their own way");
        assert!(
            duration(Duration::from_millis(5)).ends_with(t!("format.milliseconds").as_ref()),
            "and the one the interface stands in is the one written"
        );
    }

    #[test]
    fn long_spans_use_minutes() {
        assert_eq!(duration(Duration::from_secs(60)), "1:00");
        assert_eq!(duration(Duration::from_secs(605)), "10:05");
    }
}
