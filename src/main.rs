use std::cmp;
use std::env;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use image::GenericImageView;
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use once_cell::sync::OnceCell;
use serde::{self, Deserialize};
use tokio::{runtime, time::sleep};
use walkdir::WalkDir;

mod parse_pathbuf;
mod parse_thread_count;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(with = "parse_pathbuf")]
    input_path: PathBuf,
    #[serde(with = "parse_pathbuf")]
    output_path: PathBuf,
    read_delay_ms: u64,
    long_side_limit: u32,
    smoothing_factor: u8,
    quality: f32,
    #[serde(with = "parse_thread_count")]
    thread_count: usize,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

fn is_png(f: &Path) -> bool {
    match f.extension() {
        Some(ext) => ext == OsStr::new("png"),
        _ => false,
    }
}

fn get_into_path(from: &Path) -> PathBuf {
    let c = CONFIG.get().unwrap();

    c.output_path.join(
        PathBuf::from(from)
            .strip_prefix(&c.input_path)
            .expect("Failed to strip path")
            .with_extension("jpg"),
    )
}

fn scale_down(size: (u32, u32), long_side_limit: u32) -> (u32, u32) {
    if long_side_limit == 0 {
        return size;
    }

    let long_side = cmp::max(size.0, size.1) as f64;
    let scale = long_side_limit as f64 / long_side;

    if 1.0 <= scale {
        return size;
    }

    (
        (size.0 as f64 * scale) as u32,
        (size.1 as f64 * scale) as u32,
    )
}

fn process_image(from: &Path, into: &Path) {
    if !is_png(from) {
        return;
    }

    if into.exists() {
        return;
    }

    let im = image::open(&from).expect("Failed to open original file");

    std::fs::create_dir_all(
        PathBuf::from(into)
            .parent()
            .expect("Failed to get parent directory"),
    )
    .expect("Failed to create parent directory");

    let c = CONFIG.get().unwrap();

    println!(
        "Compress: {}",
        PathBuf::from(from)
            .strip_prefix(&c.input_path)
            .expect("Failed to get relative filepath")
            .to_str()
            .expect("Failed to convert filename to str")
    );

    let dimensions = scale_down(im.dimensions(), c.long_side_limit);
    let im = im.thumbnail(dimensions.0, dimensions.1);
    let im = im.into_rgb8();

    let jpeg_bytes = std::panic::catch_unwind(|| {
        let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        comp.set_smoothing_factor(c.smoothing_factor);
        comp.set_size(dimensions.0 as usize, dimensions.1 as usize);
        comp.set_quality(c.quality);
        comp.set_color_space(mozjpeg::ColorSpace::JCS_YCbCr);
        comp.set_mem_dest();
        comp.start_compress();
        assert!(comp.write_scanlines(&im.to_vec()));
        comp.finish_compress();
        comp.data_to_vec().unwrap()
    })
    .expect("MozJPEG Fail");

    let mut f = File::create(&into).expect("Failed to create output file");
    f.write_all(&jpeg_bytes).expect("Failed to write file");
    f.flush().expect("Failed to flush file");
}

#[tokio::main]
async fn main() {
    if env::args().len() != 2 {
        panic!("please set configuration yaml file");
    }

    let config_path = PathBuf::from(env::args().nth(1).unwrap());

    CONFIG
        .set(
            serde_yaml::from_reader(BufReader::new(
                File::open(config_path).expect("Failed to open config.yml"),
            ))
            .expect("Failed to parse CONFIG"),
        )
        .unwrap();

    let c = CONFIG.get().unwrap();

    if !c.output_path.is_dir() {
        if !c.output_path.parent().unwrap().is_dir() {
            panic!("Failed to lookup output directory");
        }

        std::fs::create_dir_all(&c.output_path).expect("Failed to create output directory");
    }

    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(c.thread_count)
        .enable_time()
        .build()
        .expect("Failed to create worker threads");

    WalkDir::new(&c.input_path)
        .into_iter()
        .filter_map(|f| f.ok())
        .filter(|f| is_png(f.path()))
        .for_each(|from| {
            let from = from.path().to_owned();
            let into = get_into_path(&from);

            rt.spawn(async move {
                process_image(&from, &into);
            });
        });

    let (tx, rx) = channel();

    let mut watcher = watcher(tx, Duration::from_secs(1)).expect("Failed to create watcher");

    watcher
        .watch(&c.input_path, RecursiveMode::Recursive)
        .expect("Failed to watch input directory");

    loop {
        match rx.recv() {
            Ok(event) => {
                if let DebouncedEvent::Create(event) = event {
                    let from = event.as_path().to_owned();
                    let into = get_into_path(&from);

                    rt.spawn(async move {
                        sleep(Duration::from_millis(c.read_delay_ms)).await;
                        process_image(&from, &into);
                    });
                }
            }
            Err(e) => panic!("Error: {:?}", e),
        }
    }
}
