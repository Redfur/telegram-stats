use serde::de::{self, Deserializer};
use serde::Deserialize;

/// `date_unixtime`/`edited_unixtime`/`duration_seconds` в реальных
/// экспортах Telegram Desktop разных версий встречаются то числом, то
/// строкой с числом. `id` в это не заворачиваем — в разведке реального
/// экспорта он всегда был числом.
pub fn opt_i64_from_num_or_str<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
    }

    match Option::<NumOrStr>::deserialize(deserializer)? {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => s
            .parse::<i64>()
            .map(Some)
            .map_err(|_| de::Error::custom(format!("expected an integer, got string {s:?}"))),
    }
}
