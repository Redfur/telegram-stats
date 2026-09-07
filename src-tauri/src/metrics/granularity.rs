#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Granularity {
    Hour,
    Day,
    Week,
    Month,
    Year,
}

const THREE_DAYS: i64 = 3 * 86_400;
const NINETY_DAYS: i64 = 90 * 86_400;
const TWO_YEARS: i64 = 2 * 365 * 86_400;
const TEN_YEARS: i64 = 10 * 365 * 86_400;

/// Гранулярность графика — по длине периода, а не зашита на «по годам»
/// (docs/SPEC.md §7). Чистая функция: любая будущая time-series метрика
/// переиспользует её так же, как эта.
pub fn pick_granularity(first_ts: i64, last_ts: i64) -> Granularity {
    let span = (last_ts - first_ts).max(0);
    if span <= THREE_DAYS {
        Granularity::Hour
    } else if span <= NINETY_DAYS {
        Granularity::Day
    } else if span <= TWO_YEARS {
        Granularity::Week
    } else if span <= TEN_YEARS {
        Granularity::Month
    } else {
        Granularity::Year
    }
}

/// `strftime`-выражение для группировки по бакету. Всегда UTC — часовой
/// пояс экспортёра восстановить нельзя (SPEC §8.4), фронт обязан честно
/// это пометить, а не подделывать локальное время.
pub fn strftime_bucket_expr(g: Granularity) -> &'static str {
    match g {
        Granularity::Hour => "%Y-%m-%dT%H:00:00",
        Granularity::Day => "%Y-%m-%d",
        Granularity::Week => "%Y-%W",
        Granularity::Month => "%Y-%m",
        Granularity::Year => "%Y",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_span_is_hourly() {
        assert_eq!(pick_granularity(0, THREE_DAYS - 1), Granularity::Hour);
        assert_eq!(pick_granularity(0, THREE_DAYS), Granularity::Hour);
    }

    #[test]
    fn boundaries_pick_the_coarser_side() {
        assert_eq!(pick_granularity(0, THREE_DAYS + 1), Granularity::Day);
        assert_eq!(pick_granularity(0, NINETY_DAYS), Granularity::Day);
        assert_eq!(pick_granularity(0, NINETY_DAYS + 1), Granularity::Week);
        assert_eq!(pick_granularity(0, TWO_YEARS), Granularity::Week);
        assert_eq!(pick_granularity(0, TWO_YEARS + 1), Granularity::Month);
        assert_eq!(pick_granularity(0, TEN_YEARS), Granularity::Month);
        assert_eq!(pick_granularity(0, TEN_YEARS + 1), Granularity::Year);
    }

    #[test]
    fn identical_timestamps_are_hourly_not_a_panic() {
        assert_eq!(pick_granularity(1000, 1000), Granularity::Hour);
    }

    #[test]
    fn out_of_order_timestamps_do_not_panic() {
        // защита от отрицательного диапазона, если вызовут не в том порядке
        assert_eq!(pick_granularity(1000, 500), Granularity::Hour);
    }
}
