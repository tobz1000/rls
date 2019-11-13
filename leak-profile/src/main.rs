#![feature(proc_macro_hygiene)]

mod run_command;

use command_macros::command;
use failure::{Error, Fail};
use log::{error, info, LevelFilter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Duration;

use crate::run_command::{run, run_with_timeout};

const RLS_DIR: &str = "/home/toby/sources/rls";
const TEST_PROJ_DIR: &str = "/home/toby/sources/rls/test-proj";

const GOOD_TOOLCHAIN: &str = "leaktest-prev"; // Rust #757d6cc91a
const BAD_TOOLCHAIN: &str = "leaktest"; // Rust #b6e8f9dbdc Remove the `alloc_jemalloc` crate

const RELEASE: bool = true;
const RUN_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Fail)]
#[fail(display = "Maxi")]
struct MaxHeapNotFound {
    toolchain: &'static str
}

fn main() {
    pretty_env_logger::formatted_builder().filter_level(LevelFilter::Debug).init();
    match main_inner() {
        Ok(()) => {}
        Err(e) => error!("{}", e),
    }
}

fn main_inner() -> Result<(), Error> {
    run(command!(cargo clean --manifest-path=(TEST_PROJ_DIR)/Cargo.toml))?;

    build(GOOD_TOOLCHAIN)?;
    build(BAD_TOOLCHAIN)?;
    profile(GOOD_TOOLCHAIN)?;
    profile(BAD_TOOLCHAIN)?;
    summary(GOOD_TOOLCHAIN)?;
    summary(BAD_TOOLCHAIN)?;

    Ok(())
}

fn build(toolchain: &'static str) -> Result<(), Error> {
    info!("Building {}", toolchain);

    let mut build_cmd = command!(
        rustup run (toolchain)
        cargo build --target-dir=(RLS_DIR)/target/(toolchain)
        --no-default-features
        if RELEASE { --release }
    );
    build_cmd.current_dir(RLS_DIR);

    run(build_cmd)?;

    Ok(())
}

fn profile(toolchain: &'static str) -> Result<(), Error> {
    info!("Profiling {}", toolchain);

    let toolchain_path = run(command!(rustup run (toolchain) rustc --print sysroot))?;

    let mut profile_cmd = command!(
        rustup run (toolchain)
        valgrind --tool=massif --massif-out-file=(RLS_DIR)/(toolchain).massif
        (RLS_DIR)/target/(toolchain)/(if RELEASE { "release" } else { "debug" })/rls --cli
    );
    profile_cmd.env("LD_LIBRARY_PATH", format!("{}/lib", toolchain_path.trim()));
    profile_cmd.current_dir(TEST_PROJ_DIR);

    run_with_timeout(profile_cmd, Duration::from_secs(RUN_TIMEOUT_SECS))?;

    Ok(())
}

fn summary(toolchain: &'static str) -> Result<(), Error> {
    info!("Reading output for {}", toolchain);

    let max_heap_bytes = get_massif_peak_heap_bytes(toolchain)?;
    let max_heap_mib = max_heap_bytes as f64 / (1024 * 1024) as f64;

    let message = format!("{} max heap: {:.1}MiB", toolchain, max_heap_mib);

    info!("{}", message);
    println!("{}", message);

    Ok(())
}

fn get_massif_peak_heap_bytes(toolchain: &'static str) -> Result<u64, Error> {
    // Expects each snapshot in output to have byte count line before the (if any) "is peak heap"
    // line
    info!("Reading output for {}", toolchain);

    const BYTES_LINE_START: &str = "mem_heap_B=";
    const HEAP_TREE_PEAK_LINE: &str = "heap_tree=peak";

    let out_file = File::open(format!("{}/{}.massif", RLS_DIR, toolchain))?;
    let reader = BufReader::new(out_file);

    let mut mem_heap_val: Option<u64> = None;

    for line in reader.lines() {
        let line = line?;

        if line.starts_with(BYTES_LINE_START) {
            mem_heap_val = Some(line[BYTES_LINE_START.len()..].parse()?);
        } else if line.trim() == HEAP_TREE_PEAK_LINE {
            if let Some(mem_heap_val) = mem_heap_val {
                return Ok(mem_heap_val);
            } else {
                // Found "is peak heap" specifier but never found a byte count
                return Err(MaxHeapNotFound { toolchain })?;
            }
        }
    }

    // Never found "is peak heap" specifier
    return Err(MaxHeapNotFound { toolchain })?;
}