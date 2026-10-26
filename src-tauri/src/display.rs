//! 画面に出す文字列の整形。

use chrono::{DateTime, Utc};

/// 「3時間前」のような経過時間の表記。
pub fn age_label(published: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let elapsed = now - published;
    if elapsed.num_minutes() < 1 {
        "たった今".to_string()
    } else if elapsed.num_minutes() < 60 {
        format!("{}分前", elapsed.num_minutes())
    } else if elapsed.num_hours() < 24 {
        format!("{}時間前", elapsed.num_hours())
    } else if elapsed.num_hours() < 48 {
        "昨日".to_string()
    } else {
        format!("{}日前", elapsed.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
    }

    #[test]
    fn age_labels() {
        let label = |d: Duration| age_label(now() - d, now());
        assert_eq!(label(Duration::seconds(30)), "たった今");
        assert_eq!(label(Duration::minutes(5)), "5分前");
        assert_eq!(label(Duration::hours(3)), "3時間前");
        assert_eq!(label(Duration::hours(30)), "昨日");
        assert_eq!(label(Duration::days(3)), "3日前");
    }

    #[test]
    fn future_dates_are_now() {
        assert_eq!(age_label(now() + Duration::hours(2), now()), "たった今");
    }
}
