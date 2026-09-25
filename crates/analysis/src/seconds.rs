//! Serde for a [`Duration`] as seconds (`span = 4.0`), so saved settings
//! stay readable. Use with `#[serde(with = "dasmeter_analysis::seconds")]`.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(duration.as_secs_f64())
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    let seconds = f64::deserialize(deserializer)?;
    Duration::try_from_secs_f64(seconds).map_err(serde::de::Error::custom)
}
