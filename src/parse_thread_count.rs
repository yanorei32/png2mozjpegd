use std::cmp;

use serde::{self, Deserialize, Deserializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let n = usize::deserialize(deserializer)?;
    Ok(if n == 0 {
        cmp::max(num_cpus::get(), 1)
    } else {
        n
    })
}
