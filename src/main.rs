#![warn(clippy::pedantic, clippy::nursery)]

use std::cmp;
use std::env;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use image::GenericImageView;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::OnceCell;
use tokio::{runtime, time::sleep};
use walkdir::WalkDir;

mod model;

static CONFIG: OnceCell<model::Config> = OnceCell::new();

fn is_png(f: &Path) -> bool {
    f.extension().map_or(false, |ext| ext == OsStr::new("png"))
}

fn newfname_from_origfname(from: &Path) -> PathBuf {
    let c = CONFIG.get().unwrap();

    let from_pathbuf = PathBuf::from(&from);

    c.output_path.join(
        if c.flatten {
            Path::new(from.file_name().expect("Failed to get filename"))
        } else {
            from_pathbuf
                .strip_prefix(&c.input_path)
                .expect("Failed to strip path")
        }
        .with_extension("jpg"),
    )
}

fn calc_new_dimensions(size: (u32, u32), long_side_limit: u32) -> (u32, u32) {
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_possible_truncation)]

    if long_side_limit == 0 {
        return size;
    }

    let long_side = f64::from(cmp::max(size.0, size.1));
    let scale = f64::from(long_side_limit) / long_side;

    if 1.0 <= scale {
        return size;
    }

    (
        (f64::from(size.0) * scale) as u32,
        (f64::from(size.1) * scale) as u32,
    )
}

fn process_image(from: &Path, into: &Path) {
    if into.exists() {
        return;
    }

    let c = CONFIG.get().unwrap();

    println!(
        "Compress: {}",
        PathBuf::from(from)
            .strip_prefix(&c.input_path)
            .expect("Failed to get relative filepath")
            .to_str()
            .unwrap()
    );

    let im = image::open(from).expect("Failed to open original file");

    std::fs::create_dir_all(
        PathBuf::from(into)
            .parent()
            .expect("Failed to get parent directory"),
    )
    .expect("Failed to create parent directory");

    let dimensions = calc_new_dimensions(im.dimensions(), c.long_side_limit);
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
        assert!(comp.write_scanlines(&im));
        comp.finish_compress();
        comp.data_to_vec().unwrap()
    })
    .expect("MozJPEG Fail");

    let mut f = File::create(into).expect("Failed to create output file");
    f.write_all(&jpeg_bytes).expect("Failed to write file");
    f.flush().expect("Failed to flush file");
}

#[tokio::main]
async fn main() {
    let config_path = env::args().nth(1).map_or_else(
        || {
            let mut buf = env::current_exe().unwrap();
            buf.pop();
            buf.push("config.yml");
            buf
        },
        PathBuf::from,
    );

    println!("{}", config_path.clone().into_os_string().to_string_lossy());

    CONFIG
        .set(
            serde_yaml::from_reader(BufReader::new(
                File::open(&config_path).expect("Failed to open config.yml"),
            ))
            .expect("Failed to parse CONFIG"),
        )
        .unwrap();

    let c = CONFIG.get().unwrap();

    if !c.output_path.is_dir() {
        std::fs::create_dir_all(&c.output_path).expect("Failed to create output directory");
    }

    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(c.thread_count)
        .enable_time()
        .build()
        .expect("Failed to create worker threads");

    WalkDir::new(&c.input_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|f| is_png(f.path()))
        .for_each(|from| {
            let from = from.path().to_owned();
            let into = newfname_from_origfname(&from);

            rt.spawn(async move {
                process_image(&from, &into);
            });
        });

    if c.mode == model::Mode::Oneshot {
        return;
    }

    let (tx, rx) = channel();

    let mut watcher =
        RecommendedWatcher::new(tx, notify::Config::default()).expect("Failed to create watcher");

    watcher
        .watch(&c.input_path, RecursiveMode::Recursive)
        .expect("Failed to watch input directory");

    println!("Wait for new file... (Press Ctrl+C to exit)");

    for res in rx {
        let event = match res {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Watch Error: {:?}", e);
                continue;
            }
        };

        let EventKind::Create(_) = event.kind else {
            continue
        };

        event
            .paths
            .into_iter()
            .filter(|v| is_png(v))
            .for_each(|from| {
                let into = newfname_from_origfname(&from);

                rt.spawn(async move {
                    sleep(Duration::from_millis(c.read_delay_ms)).await;
                    process_image(&from, &into);
                });
            });
    }
}
