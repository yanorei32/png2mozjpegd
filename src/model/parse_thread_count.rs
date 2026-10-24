use serde::{self, Deserialize, Deserializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let n = usize::deserialize(deserializer)?;
    Ok(if n == 0 {
        std::thread::available_parallelism().map(|v| v.into()).unwrap_or(1)
    } else {
        n
    })
}
