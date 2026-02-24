//! Benchmark service adapted from legacy WTGBench logic (simplified).

use crate::Result;
use rand::{Rng, RngCore};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;

#[cfg(target_os = "windows")]
const FILE_FLAG_NO_BUFFERING: u32 = 0x20000000;
#[cfg(target_os = "windows")]
const FILE_FLAG_WRITE_THROUGH: u32 = 0x80000000;

const DATA_LENGTH: u64 = 1_073_741_824; // 1GB region
const BLOCK_SIZE: usize = 4096;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct BenchmarkResult {
    pub mode: String,
    pub write_seq: f64, // MB/s
    pub write_4k: f64,  // MB/s (single-thread)
    pub thread_results: Vec<ThreadResult>,
    pub full_seq_samples: Vec<Sample>,
    pub duration_ms: u64,
    pub full_written_gb: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ThreadResult {
    pub threads: u32,
    pub mb_s: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Sample {
    pub t_ms: u64,
    pub value: f64,
    pub x_gb: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct BenchmarkConfig {
    pub target_path: String,
    pub mode: String, // quick | multithread | full
}

fn open_benchmark_file(path: &str) -> Result<File> {
    #[cfg(target_os = "windows")]
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .share_mode(0) // exclusive
            .custom_flags((FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH))
            .open(path)?;
        return Ok(file);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?;
        Ok(file)
    }
}

fn generate_block() -> [u8; BLOCK_SIZE] {
    let mut buf = [0u8; BLOCK_SIZE];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

#[cfg(target_os = "windows")]
fn get_free_bytes(root: &str) -> u64 {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let mut path: Vec<u16> = OsStr::new(root).encode_wide().collect();
    if !path.ends_with(&[0]) {
        path.push(0);
    }
    unsafe {
        let mut free: u64 = 0;
        let mut total: u64 = 0;
        let mut avail: u64 = 0;
        if GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut free), Some(&mut total), Some(&mut avail)).is_ok() {
            free
        } else {
            0
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_free_bytes(_root: &str) -> u64 {
    0
}

fn sequential_bench(path: &str, duration: Duration) -> Result<(f64, Vec<Sample>)> {
    let mut file = open_benchmark_file(path)?;
    let mut offset: u64 = 0;
    let block = generate_block();
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut bytes_written: u64 = 0;
    let mut last_mark = Instant::now();

    while start.elapsed() < duration {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&block)?;
        file.flush()?;
        offset = (offset + BLOCK_SIZE as u64) % DATA_LENGTH;
        bytes_written += BLOCK_SIZE as u64;
        if last_mark.elapsed() >= Duration::from_millis(500) {
            let mbps = (bytes_written as f64 / 1024.0 / 1024.0) / start.elapsed().as_secs_f64().max(0.001);
            samples.push(Sample {
                t_ms: start.elapsed().as_millis() as u64,
                value: mbps,
                x_gb: bytes_written as f64 / 1_073_741_824.0, // GiB written
            });
            last_mark = Instant::now();
        }
    }

    let mbps_total = (bytes_written as f64 / 1024.0 / 1024.0) / start.elapsed().as_secs_f64().max(0.001);
    Ok((mbps_total, samples))
}

/// Full-disk style sequential write: write until target_bytes reached (cap) while sampling
fn sequential_full(path: &str, target_bytes: u64) -> Result<(f64, Vec<Sample>, u64)> {
    let mut file = open_benchmark_file(path)?;
    let block = vec![0u8; 1024 * 1024]; // 1MB chunk
    let mut offset: u64 = 0;
    let start = Instant::now();
    let mut bytes_written: u64 = 0;
    let mut samples = Vec::new();
    let mut last_mark = Instant::now();

    while bytes_written < target_bytes {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&block)?;
        file.flush()?;
        offset = (offset + block.len() as u64) % DATA_LENGTH;
        bytes_written += block.len() as u64;
        if last_mark.elapsed() >= Duration::from_millis(500) {
            let mbps = (bytes_written as f64 / 1024.0 / 1024.0) / start.elapsed().as_secs_f64().max(0.001);
            samples.push(Sample {
                t_ms: start.elapsed().as_millis() as u64,
                value: mbps,
                x_gb: bytes_written as f64 / 1_073_741_824.0,
            });
            last_mark = Instant::now();
        }
    }

    let mbps_total = (bytes_written as f64 / 1024.0 / 1024.0) / start.elapsed().as_secs_f64().max(0.001);
    Ok((mbps_total, samples, bytes_written))
}

fn random_4k_single(path: &str, duration: Duration) -> Result<f64> {
    let mut file = open_benchmark_file(path)?;
    let block = generate_block();
    let mut rng = rand::thread_rng();
    let start = Instant::now();
    let mut bytes_written: u64 = 0;
    while start.elapsed() < duration {
        let offset_block: u64 = rng.gen_range(0..(DATA_LENGTH / BLOCK_SIZE as u64 + 1));
        file.seek(SeekFrom::Start(offset_block * BLOCK_SIZE as u64))?;
        file.write_all(&block)?;
        file.flush()?;
        bytes_written += BLOCK_SIZE as u64;
    }
    let mbps = (bytes_written as f64 / 1024.0 / 1024.0) / start.elapsed().as_secs_f64().max(0.001);
    Ok(mbps)
}

fn random_4k_multi(path: &str, threads: u32, duration: Duration) -> Result<f64> {
    let block = generate_block();
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    let bytes_written = Arc::new(std::sync::atomic::AtomicU64::new(0));

    for _ in 0..threads {
        let stop_flag = stop.clone();
        let bw = bytes_written.clone();
        let temp_path = path.to_string();
        let block_clone = block.clone();
        handles.push(thread::spawn(move || -> Result<()> {
            let mut rng = rand::thread_rng();
            let mut file = open_benchmark_file(&temp_path)?;
            while !stop_flag.load(Ordering::Relaxed) {
                let offset_block: u64 = rng.gen_range(0..(DATA_LENGTH / BLOCK_SIZE as u64 + 1));
                file.seek(SeekFrom::Start(offset_block * BLOCK_SIZE as u64))?;
                file.write_all(&block_clone)?;
                file.flush()?;
                bw.fetch_add(BLOCK_SIZE as u64, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join().unwrap_or(Ok(()));
    }

    let total_bytes = bytes_written.load(Ordering::Relaxed);
    let mbps = (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64().max(0.001);
    Ok(mbps)
}

pub async fn run_benchmark(config: &BenchmarkConfig) -> Result<BenchmarkResult> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err(crate::AppError::Unsupported("Benchmark only implemented on Windows".into()));
    }

    #[cfg(target_os = "windows")]
    {
        let base = config.target_path.trim_end_matches(['\\', '/']);
        let temp_file = format!("{}\\wtg_bench.bin", base);

        let (write_seq, seq_samples, write_4k, thread_results, written_gb);
        let start = Instant::now();

        match config.mode.as_str() {
            "multithread" => {
                let seq = sequential_bench(&temp_file, Duration::from_secs(5))?;
                write_seq = seq.0;
                seq_samples = seq.1;
                written_gb = seq_samples.last().map(|s| s.x_gb).unwrap_or(0.0);
                write_4k = random_4k_single(&temp_file, Duration::from_secs(3))?;
                let mut threads_res = Vec::new();
                for t in [1u32, 2, 4, 8, 16, 32] {
                    let mbps = random_4k_multi(&temp_file, t, Duration::from_secs(3))?;
                    threads_res.push(ThreadResult { threads: t, mb_s: mbps });
                }
                thread_results = threads_res;
            }
            "full" => {
                let free = get_free_bytes(&format!("{}\\", base));
                // cap to 80% free space, max 128GB, min 4GB
                let target = free.saturating_mul(8).saturating_div(10).min(128 * 1024 * 1024 * 1024).max(4 * 1024 * 1024 * 1024);
                let seq = sequential_full(&temp_file, target)?;
                write_seq = seq.0;
                seq_samples = seq.1;
                written_gb = seq.2 as f64 / 1_073_741_824.0;
                write_4k = random_4k_single(&temp_file, Duration::from_secs(5))?;
                thread_results = vec![];
            }
            _ => {
                // quick
                let seq = sequential_bench(&temp_file, Duration::from_secs(5))?;
                write_seq = seq.0;
                seq_samples = seq.1;
                written_gb = seq_samples.last().map(|s| s.x_gb).unwrap_or(0.0);
                write_4k = random_4k_single(&temp_file, Duration::from_secs(3))?;
                thread_results = vec![];
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let _ = std::fs::remove_file(&temp_file);

        Ok(BenchmarkResult {
            mode: config.mode.clone(),
            write_seq: (write_seq * 10.0).round() / 10.0,
            write_4k: (write_4k * 10.0).round() / 10.0,
            thread_results,
            full_seq_samples: seq_samples,
            duration_ms,
            full_written_gb: written_gb,
        })
    }
}
