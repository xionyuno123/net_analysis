use core::config::load_config;
use core::subscription::{DnsTransaction};
use core::Runtime;
use core::rte_rdtsc;
use filtergen::filter;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use clap::Parser;

// Define command-line arguments.
#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    config: PathBuf,
    #[clap(short, long)]
    spin: u64,
}

#[filter("dns")]
fn main() {
    env_logger::init();
    let args = Args::parse();
    let config = load_config(&args.config);
    let cycle = args.spin;
    let callback = |conn: DnsTransaction| {
        spin(cycle);
    };
    let mut runtime = Runtime::new(config, filter, callback).unwrap();
    runtime.run();
}

#[inline]
fn spin(cycles: u64) {
    if cycles == 0 {
        return;
    }
    let start = unsafe { rte_rdtsc() };
    loop {
        let now = unsafe { rte_rdtsc() };
        if now - start > cycles {
            break;
        }
    }
}