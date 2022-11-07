use serde::{self, Deserialize, Deserializer};
use std::path::PathBuf;

pub fn deserialize<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let path = PathBuf::from(&String::deserialize(deserializer)?);

    Ok(if path.has_root() {
        path
    } else {
        dirs::home_dir().expect("Failed to get home_dir").join(path)
    })
}
