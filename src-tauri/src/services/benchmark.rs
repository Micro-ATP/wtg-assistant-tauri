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
const MB: u64 = 1024 * 1024;
const FULL_IO_BYTES: u64 = 64 * MB;
const FULL_STEP_BYTES: u64 = 1024 * MB;
const FULL_RESERVED_BYTES: u64 = 100 * MB;

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
    pub mode: String, // quick | multithread | fullwrite | full
}

fn open_benchmark_file(path: &str) -> Result<File> {
    #[cfg(target_os = "windows")]
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .share_mode(0) // exclusive
            .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH)
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

/// WTGB-style full sequential test:
/// - 64MB IO size
/// - 1GB per measurement step
/// - writes through the whole target area (no ring buffer wrap)
fn sequential_full(path: &str, target_bytes: u64) -> Result<(f64, Vec<Sample>, u64)> {
    if target_bytes < FULL_IO_BYTES {
        return Err(crate::AppError::InvalidParameter(
            "Not enough free space for full benchmark".to_string(),
        ));
    }

    let mut file = open_benchmark_file(path)?;
    let mut block = vec![0u8; FULL_IO_BYTES as usize];
    rand::thread_rng().fill_bytes(&mut block);

    let total_steps = (target_bytes / FULL_STEP_BYTES).max(1);
    let chunks_per_step = FULL_STEP_BYTES / FULL_IO_BYTES;
    let global_start = Instant::now();
    let mut samples = Vec::new();
    let mut bytes_written: u64 = 0;
    let mut speed_sum = 0.0;
    let mut speed_cnt: u64 = 0;

    for step in 0..total_steps {
        let step_start = Instant::now();
        let step_begin = bytes_written;
        for _ in 0..chunks_per_step {
            if bytes_written + FULL_IO_BYTES > target_bytes {
                break;
            }
            file.seek(SeekFrom::Start(bytes_written))?;
            file.write_all(&block)?;
            bytes_written += FULL_IO_BYTES;
            if bytes_written >= target_bytes {
                break;
            }
        }
        file.flush()?;

        let step_written = bytes_written.saturating_sub(step_begin);
        if step_written == 0 {
            break;
        }
        let elapsed = step_start.elapsed().as_secs_f64().max(0.001);
        let cur_speed = (step_written as f64 / 1024.0 / 1024.0) / elapsed;

        // Match WTGB behavior: skip first data point for chart stabilization.
        if step > 0 {
            samples.push(Sample {
                t_ms: global_start.elapsed().as_millis() as u64,
                value: cur_speed,
                x_gb: (bytes_written as f64 / 1_073_741_824.0).max(0.0),
            });
            speed_sum += cur_speed;
            speed_cnt += 1;
        }

        if bytes_written >= target_bytes {
            break;
        }
    }

    if speed_cnt == 0 {
        // For very small targets where only one step is written.
        let elapsed = global_start.elapsed().as_secs_f64().max(0.001);
        let mbps_total = (bytes_written as f64 / 1024.0 / 1024.0) / elapsed;
        return Ok((mbps_total, samples, bytes_written));
    }

    let mbps_total = speed_sum / speed_cnt as f64;
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
    let mut thread_paths = Vec::new();

    for idx in 0..threads {
        let stop_flag = stop.clone();
        let bw = bytes_written.clone();
        // Use one file per worker thread to avoid lock contention and sharing errors.
        let temp_path = format!("{}.mt{}", path, idx);
        thread_paths.push(temp_path.clone());
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

    let mut worker_error: Option<crate::AppError> = None;
    for h in handles {
        match h.join() {
            Ok(result) => {
                if let Err(e) = result {
                    worker_error = Some(e);
                }
            }
            Err(_) => {
                worker_error = Some(crate::AppError::SystemError(
                    "Benchmark worker thread panicked".to_string(),
                ));
            }
        }
    }

    for p in thread_paths {
        let _ = std::fs::remove_file(p);
    }

    if let Some(e) = worker_error {
        return Err(e);
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
            "fullwrite" => {
                let free = get_free_bytes(&format!("{}\\", base));
                // WTGB full-seq is a near-full-disk write. Keep a small reserve to avoid hard full.
                let target = free.saturating_sub(FULL_RESERVED_BYTES);
                let aligned_target = (target / FULL_IO_BYTES) * FULL_IO_BYTES;
                if aligned_target < FULL_IO_BYTES {
                    return Err(crate::AppError::InvalidParameter(
                        "Not enough free space for full benchmark (needs at least ~64MB)".into(),
                    ));
                }
                let seq = sequential_full(&temp_file, aligned_target)?;
                write_seq = seq.0;
                seq_samples = seq.1;
                written_gb = seq.2 as f64 / 1_073_741_824.0;
                write_4k = 0.0;
                thread_results = vec![];
            }
            "full" => {
                // Extreme mode: full-disk sequential + random 4K tail.
                let free = get_free_bytes(&format!("{}\\", base));
                let target = free.saturating_sub(FULL_RESERVED_BYTES);
                let aligned_target = (target / FULL_IO_BYTES) * FULL_IO_BYTES;
                if aligned_target < FULL_IO_BYTES {
                    return Err(crate::AppError::InvalidParameter(
                        "Not enough free space for full benchmark (needs at least ~64MB)".into(),
                    ));
                }
                let seq = sequential_full(&temp_file, aligned_target)?;
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
