use jiff::{Timestamp as Instant, ToSpan, Zoned, civil::Date, tz::TimeZone};

use crate::model::Timestamp;

/// How many days back a weekday name is still unambiguous.
const WEEKDAY_WINDOW: i32 = 6;
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Writes moments relative to one "today" in one time zone, so labels in a list agree and
/// tests can pin both.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeLabels {
    today: Date,
    zone: TimeZone,
}

impl TimeLabels {
    /// Today in the system's time zone.
    #[must_use]
    pub fn now() -> Self {
        let now = Zoned::now();
        Self {
            today: now.date(),
            zone: now.time_zone().clone(),
        }
    }

    /// Today as of `now` in `zone`.
    #[must_use]
    pub fn at(now: Timestamp, zone: TimeZone) -> Self {
        let today =
            local(now, &zone).map_or_else(|| Date::constant(1970, 1, 1), |zoned| zoned.date());
        Self { today, zone }
    }

    /// For a conversation list: "14:05", "Yesterday", "Tue", "12 Mar", "12 Mar 2024".
    #[must_use]
    pub fn short(&self, moment: Timestamp) -> String {
        let Some(zoned) = local(moment, &self.zone) else {
            return String::new();
        };
        match self.days_ago(zoned.date()) {
            Some(0) => clock(&zoned),
            Some(1) => "Yesterday".to_owned(),
            Some(days) if days <= WEEKDAY_WINDOW => abbreviated(weekday(zoned.date())).to_owned(),
            _ if zoned.year() == self.today.year() => {
                format!("{} {}", zoned.day(), abbreviated(month(zoned.date())))
            }
            _ => format!(
                "{} {} {}",
                zoned.day(),
                abbreviated(month(zoned.date())),
                zoned.year()
            ),
        }
    }

    /// The time of day, "09:41".
    #[must_use]
    pub fn clock(&self, moment: Timestamp) -> String {
        local(moment, &self.zone).map_or_else(String::new, |zoned| clock(&zoned))
    }

    /// A day heading in a conversation: "Today", "Yesterday", "Monday", "Wednesday 12 March",
    /// "12 March 2024".
    #[must_use]
    pub fn day(&self, moment: Timestamp) -> String {
        let Some(zoned) = local(moment, &self.zone) else {
            return String::new();
        };
        let date = zoned.date();
        match self.days_ago(date) {
            Some(0) => "Today".to_owned(),
            Some(1) => "Yesterday".to_owned(),
            Some(days) if days <= WEEKDAY_WINDOW => weekday(date).to_owned(),
            _ if date.year() == self.today.year() => {
                format!("{} {} {}", weekday(date), date.day(), month(date))
            }
            _ => format!("{} {} {}", date.day(), month(date), date.year()),
        }
    }

    /// The local calendar day of `moment`, for grouping.
    #[must_use]
    pub fn date(&self, moment: Timestamp) -> Option<Date> {
        local(moment, &self.zone).map(|zoned| zoned.date())
    }

    fn days_ago(&self, date: Date) -> Option<i32> {
        if date >= self.today {
            return Some(0);
        }
        date.until(self.today).ok().map(|span| span.get_days())
    }
}

fn local(moment: Timestamp, zone: &TimeZone) -> Option<Zoned> {
    let millis = i64::try_from(moment.unix_ms()).ok()?;
    let instant = Instant::from_millisecond(millis).ok()?;
    Some(instant.to_zoned(zone.clone()))
}

fn clock(zoned: &Zoned) -> String {
    format!("{:02}:{:02}", zoned.hour(), zoned.minute())
}

fn weekday(date: Date) -> &'static str {
    let index = usize::try_from(date.weekday().to_monday_zero_offset()).unwrap_or_default();
    WEEKDAYS.get(index).copied().unwrap_or_default()
}

fn month(date: Date) -> &'static str {
    let index = usize::try_from(date.month() - 1).unwrap_or_default();
    MONTHS.get(index).copied().unwrap_or_default()
}

fn abbreviated(name: &str) -> &str {
    name.get(..3).unwrap_or(name)
}

/// One day later than `date`, for tests and callers stepping through days.
#[must_use]
pub fn next_day(date: Date) -> Option<Date> {
    date.checked_add(1.day()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Friday 6 November 2026, 12:00 UTC.
    const NOW: u64 = 1_793_966_400_000;
    const HOUR: u64 = 3_600_000;
    const DAY: u64 = 24 * HOUR;

    fn labels() -> TimeLabels {
        TimeLabels::at(Timestamp(NOW), TimeZone::UTC)
    }

    #[test]
    fn short_labels_shrink_with_distance() {
        let labels = labels();
        assert_eq!(labels.short(Timestamp(NOW - 3 * HOUR)), "09:00");
        assert_eq!(labels.short(Timestamp(NOW - DAY)), "Yesterday");
        assert_eq!(labels.short(Timestamp(NOW - 3 * DAY)), "Tue");
        assert_eq!(labels.short(Timestamp(NOW - 30 * DAY)), "7 Oct");
        assert_eq!(labels.short(Timestamp(NOW - 400 * DAY)), "2 Oct 2025");
    }

    #[test]
    fn day_headings_name_recent_days() {
        let labels = labels();
        assert_eq!(labels.day(Timestamp(NOW)), "Today");
        assert_eq!(labels.day(Timestamp(NOW - DAY)), "Yesterday");
        assert_eq!(labels.day(Timestamp(NOW - 2 * DAY)), "Wednesday");
        assert_eq!(labels.day(Timestamp(NOW - 30 * DAY)), "Wednesday 7 October");
        assert_eq!(labels.day(Timestamp(NOW - 400 * DAY)), "2 October 2025");
    }

    #[test]
    fn the_zone_moves_the_day_boundary() {
        let tokyo = TimeZone::get("Asia/Tokyo").unwrap();
        let labels = TimeLabels::at(Timestamp(NOW), tokyo);
        assert_eq!(labels.clock(Timestamp(NOW)), "21:00");
        assert_eq!(labels.short(Timestamp(NOW - 12 * HOUR)), "09:00");
        assert_eq!(labels.short(Timestamp(NOW - 22 * HOUR)), "Yesterday");
    }

    #[test]
    fn future_moments_count_as_today() {
        assert_eq!(labels().day(Timestamp(NOW + 2 * DAY)), "Today");
        assert_eq!(
            next_day(Date::constant(2026, 12, 31)),
            Some(Date::constant(2027, 1, 1))
        );
    }
}
