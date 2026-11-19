//! 自动刷新节奏。
//!
//! 至少一个市场处于常规交易时段时用 [`SESSION_POLL_INTERVAL`]，否则用
//! [`CLOSED_POLL_INTERVAL`]。菜单里的「立即刷新」不读这个间隔。
//!
//! 用户界面的钟点固定为 [`DISPLAY_TIMEZONE`]（上海，无夏令时），不读机器本地时区。
//! A 股、港股的常规时段就用这个钟。美股常规时段用纽约时间，含 2007 年起的夏令时规则，
//! 再和上海时间对齐到同一个 UTC 瞬间。没有交易所节假日表：工作日都按开市处理。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Timelike, Utc, Weekday};

/// 界面钟点。A 股、港股的常规时段也按这个时区。
pub const DISPLAY_TIMEZONE: &str = "Asia/Shanghai";

/// 常规交易时段内的自动刷新间隔。
pub const SESSION_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// 三个市场都休市时的自动刷新间隔。
pub const CLOSED_POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);

const SHANGHAI_OFFSET_SECS: i32 = 8 * 3600;
const US_REGULAR: (u32, u32) = (9 * 60 + 30, 16 * 60);
const CN_REGULAR: [(u32, u32); 2] = [(9 * 60 + 30, 11 * 60 + 30), (13 * 60, 15 * 60)];
const HK_REGULAR: [(u32, u32); 2] = [(9 * 60 + 30, 12 * 60), (13 * 60, 16 * 60)];

/// 现在到下一次自动刷新的等待时间。已经处在常规时段时为 60 秒；休市时最多 15 分钟，
/// 若下一档开市更近，则提前到开市，避免开盘后还按休市节奏空等。
pub fn automatic_poll_delay(now: DateTime<Utc>) -> Duration {
    if markets_in_regular_session(now) {
        return SESSION_POLL_INTERVAL;
    }
    let slow_minutes = (CLOSED_POLL_INTERVAL.as_secs() / 60) as i64;
    for minute in 1..=slow_minutes {
        let Some(next) = now.checked_add_signed(ChronoDuration::minutes(minute)) else {
            return CLOSED_POLL_INTERVAL;
        };
        if markets_in_regular_session(next) {
            return Duration::from_secs((minute as u64) * 60);
        }
    }
    CLOSED_POLL_INTERVAL
}

/// [`automatic_poll_delay`] 在当前 UTC 时刻的值。不读本机时区。
pub fn automatic_poll_delay_now() -> Duration {
    automatic_poll_delay(utc_now())
}

/// A 股、港股、美股里是否至少有一个处在常规交易时段。集合竞价、盘前盘后和午休都不算。
pub fn markets_in_regular_session(now: DateTime<Utc>) -> bool {
    let shanghai = now.with_timezone(&shanghai_offset());
    if weekday_session(
        shanghai.date_naive(),
        shanghai.hour(),
        shanghai.minute(),
        &CN_REGULAR,
    ) || weekday_session(
        shanghai.date_naive(),
        shanghai.hour(),
        shanghai.minute(),
        &HK_REGULAR,
    ) {
        return true;
    }
    let new_york = new_york_time(now);
    weekday_session(
        new_york.date_naive(),
        new_york.hour(),
        new_york.minute(),
        &[US_REGULAR],
    )
}

/// 把 UTC 瞬间格式化成上海时间的 `HH:MM`。不使用机器本地时区。
pub fn format_display_time(now: DateTime<Utc>) -> String {
    let local = now.with_timezone(&shanghai_offset());
    format!("{:02}:{:02}", local.hour(), local.minute())
}

fn utc_now() -> DateTime<Utc> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::<Utc>::from_timestamp(elapsed.as_secs() as i64, elapsed.subsec_nanos())
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

fn shanghai_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(SHANGHAI_OFFSET_SECS).expect("shanghai offset")
}

fn weekday_session(date: NaiveDate, hour: u32, minute: u32, windows: &[(u32, u32)]) -> bool {
    if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let minute_of_day = hour * 60 + minute;
    windows
        .iter()
        .any(|(open, close)| minute_of_day >= *open && minute_of_day < *close)
}

/// 纽约民用时间。交易时段不落在凌晨两点的切换窗口里，所以按日期选择 EST/EDT 即可。
fn new_york_time(now: DateTime<Utc>) -> DateTime<chrono::FixedOffset> {
    let edt = chrono::FixedOffset::west_opt(4 * 3600).expect("edt");
    let est = chrono::FixedOffset::west_opt(5 * 3600).expect("est");
    let as_edt = now.with_timezone(&edt);
    if us_eastern_dst(as_edt.date_naive()) {
        as_edt
    } else {
        now.with_timezone(&est)
    }
}

/// 2007 年起的美东夏令时：三月第二个星期日之后，到十一月第一个星期日之前。
/// 切换日本身是周末，常规交易时段用不到当天的模糊小时。
fn us_eastern_dst(date: NaiveDate) -> bool {
    let start = nth_sunday(date.year(), 3, 2);
    let end = nth_sunday(date.year(), 11, 1);
    date > start && date < end
}

fn nth_sunday(year: i32, month: u32, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("date");
    let days_until_sunday = (7 - first.weekday().num_days_from_sunday()) % 7;
    first + ChronoDuration::days(i64::from(days_until_sunday + (n - 1) * 7))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
    }

    fn assert_weekday(year: i32, month: u32, day: u32, weekday: Weekday) {
        assert_eq!(
            NaiveDate::from_ymd_opt(year, month, day).unwrap().weekday(),
            weekday
        );
    }

    #[test]
    fn display_clock_is_shanghai_not_utc() {
        assert_eq!(DISPLAY_TIMEZONE, "Asia/Shanghai");
        assert_eq!(format_display_time(utc(2026, 11, 18, 7, 4)), "15:04");
        assert_eq!(format_display_time(utc(2026, 11, 18, 16, 0)), "00:00");
        let source = include_str!("schedule.rs");
        assert!(!source.contains(&["chrono::", "Local"].concat()));
        assert!(!source.contains(&["Local::", "now"].concat()));
    }

    #[test]
    fn manual_refresh_is_not_a_pace_of_this_clock() {
        assert_eq!(SESSION_POLL_INTERVAL.as_secs(), 60);
        assert_eq!(CLOSED_POLL_INTERVAL.as_secs(), 15 * 60);
        assert!(SESSION_POLL_INTERVAL < CLOSED_POLL_INTERVAL);
    }

    #[test]
    fn shanghai_weekday_sessions_follow_regular_hours_only() {
        assert_weekday(2026, 11, 18, Weekday::Wed);
        // 09:30–11:30、13:00–15:00 沪深；09:30–12:00、13:00–16:00 港股。时刻是 UTC。
        assert!(markets_in_regular_session(utc(2026, 11, 18, 1, 30))); // 09:30
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 1, 29))); // 09:29
        assert!(markets_in_regular_session(utc(2026, 11, 18, 2, 0))); // 10:00 沪深和港股
        assert!(markets_in_regular_session(utc(2026, 11, 18, 3, 30))); // 11:30 沪深上午收，港股仍开
        assert!(markets_in_regular_session(utc(2026, 11, 18, 3, 45))); // 11:45 只有港股
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 4, 0))); // 12:00 两边午休
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 4, 30))); // 12:30
        assert!(markets_in_regular_session(utc(2026, 11, 18, 5, 0))); // 13:00
        assert!(markets_in_regular_session(utc(2026, 11, 18, 7, 0))); // 15:00 沪深收，港股仍开
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 8, 0))); // 16:00 港股收
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 9, 0))); // 17:00
    }

    #[test]
    fn us_regular_session_uses_new_york_dst() {
        assert_weekday(2026, 11, 18, Weekday::Wed);
        assert_weekday(2026, 7, 15, Weekday::Wed);
        assert_weekday(2026, 3, 6, Weekday::Fri);
        assert_weekday(2026, 3, 8, Weekday::Sun);
        assert_weekday(2026, 3, 9, Weekday::Mon);
        assert_weekday(2026, 10, 30, Weekday::Fri);
        assert_weekday(2026, 11, 1, Weekday::Sun);
        assert_weekday(2026, 11, 2, Weekday::Mon);

        assert!(!us_eastern_dst(
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()
        ));
        assert!(us_eastern_dst(
            NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()
        ));
        assert!(!us_eastern_dst(
            NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()
        ));
        assert!(us_eastern_dst(NaiveDate::from_ymd_opt(2026, 3, 9).unwrap()));
        assert!(us_eastern_dst(
            NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()
        ));
        assert!(!us_eastern_dst(
            NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
        ));

        // 冬令时：09:30–16:00 EST = 14:30–21:00 UTC = 22:30–次日 05:00 上海。
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 14, 29)));
        assert!(markets_in_regular_session(utc(2026, 11, 18, 14, 30)));
        assert!(markets_in_regular_session(utc(2026, 11, 18, 20, 59)));
        assert!(!markets_in_regular_session(utc(2026, 11, 18, 21, 0)));

        // 夏令时：09:30–16:00 EDT = 13:30–20:00 UTC = 21:30–次日 04:00 上海。
        assert!(!markets_in_regular_session(utc(2026, 7, 15, 13, 29)));
        assert!(markets_in_regular_session(utc(2026, 7, 15, 13, 30)));
        assert!(markets_in_regular_session(utc(2026, 7, 15, 19, 59)));
        assert!(!markets_in_regular_session(utc(2026, 7, 15, 20, 0)));

        // 切换周两边的周一开盘，确认偏移已经换过。
        assert!(markets_in_regular_session(utc(2026, 3, 6, 14, 30))); // 切换前周五 09:30 EST
        assert!(!markets_in_regular_session(utc(2026, 3, 9, 13, 29)));
        assert!(markets_in_regular_session(utc(2026, 3, 9, 13, 30))); // 切换后周一 09:30 EDT
        assert!(markets_in_regular_session(utc(2026, 10, 30, 13, 30))); // 切回前周五 09:30 EDT
        assert!(!markets_in_regular_session(utc(2026, 11, 2, 14, 29)));
        assert!(markets_in_regular_session(utc(2026, 11, 2, 14, 30))); // 切回后周一 09:30 EST
    }

    #[test]
    fn friday_us_session_continues_into_saturday_shanghai() {
        assert_weekday(2026, 11, 20, Weekday::Fri);
        // 周五 15:59 EST = 20:59 UTC = 周六 04:59 上海，美股仍在常规时段。
        assert!(markets_in_regular_session(utc(2026, 11, 20, 20, 59)));
        assert!(!markets_in_regular_session(utc(2026, 11, 20, 21, 0)));
        assert_weekday(2026, 7, 17, Weekday::Fri);
        assert!(markets_in_regular_session(utc(2026, 7, 17, 19, 59)));
        assert!(!markets_in_regular_session(utc(2026, 7, 17, 20, 0)));
    }

    #[test]
    fn weekends_and_missing_holidays_stay_on_the_weekday_calendar() {
        assert_weekday(2026, 11, 15, Weekday::Sun);
        assert_weekday(2026, 11, 16, Weekday::Mon);
        assert!(!markets_in_regular_session(utc(2026, 11, 15, 2, 0))); // 周日 10:00 上海
        assert!(markets_in_regular_session(utc(2026, 11, 16, 1, 30))); // 周一 09:30 上海

        // 2026-02-17 是春节，也是星期二。客户端没有节假日表，仍按常规时段。
        assert_weekday(2026, 2, 17, Weekday::Tue);
        assert!(markets_in_regular_session(utc(2026, 2, 17, 2, 0))); // 10:00 上海
    }

    #[test]
    fn closed_delay_wakes_for_the_next_open_but_never_past_fifteen_minutes() {
        assert_eq!(automatic_poll_delay(utc(2026, 11, 18, 2, 0)).as_secs(), 60);
        assert!(markets_in_regular_session(utc(2026, 11, 18, 2, 0)));

        let before_open = utc(2026, 11, 18, 1, 29);
        assert!(!markets_in_regular_session(before_open));
        assert_eq!(automatic_poll_delay(before_open).as_secs(), 60);

        let late_lunch = utc(2026, 11, 18, 4, 50); // 12:50 上海，13:00 开市
        assert_eq!(automatic_poll_delay(late_lunch).as_secs(), 10 * 60);

        let mid_lunch = utc(2026, 11, 18, 4, 30); // 12:30，离 13:00 还有 30 分钟
        assert_eq!(automatic_poll_delay(mid_lunch).as_secs(), 15 * 60);

        let evening = utc(2026, 11, 18, 9, 0); // 17:00 上海
        assert_eq!(automatic_poll_delay(evening).as_secs(), 15 * 60);

        let sunday = utc(2026, 11, 15, 4, 0);
        assert_eq!(automatic_poll_delay(sunday).as_secs(), 15 * 60);

        for instant in [
            before_open,
            late_lunch,
            mid_lunch,
            evening,
            sunday,
            utc(2026, 11, 18, 14, 30),
            utc(2026, 7, 15, 13, 30),
        ] {
            let delay = automatic_poll_delay(instant).as_secs();
            assert!((60..=15 * 60).contains(&delay), "{instant} -> {delay}");
        }
    }
}
