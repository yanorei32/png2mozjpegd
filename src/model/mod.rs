use serde::{self, Deserialize};
use std::path::PathBuf;

mod parse_pathbuf;
mod parse_thread_count;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum Mode {
    Oneshot,
    Daemon,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(with = "parse_pathbuf")]
    pub input_path: PathBuf,
    #[serde(with = "parse_pathbuf")]
    pub output_path: PathBuf,
    pub flatten: bool,
    pub read_delay_ms: u64,
    pub long_side_limit: u32,
    pub smoothing_factor: u8,
    pub quality: f32,
    pub mode: Mode,
    #[serde(with = "parse_thread_count")]
    pub thread_count: usize,
}
