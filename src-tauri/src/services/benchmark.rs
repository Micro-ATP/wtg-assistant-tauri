//! Benchmark service aligned with WTGBench methodology.

use crate::Result;
use rand::{Rng, RngCore};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

#[cfg(target_os = "windows")]
const FILE_FLAG_NO_BUFFERING: u32 = 0x20000000;
#[cfg(target_os = "windows")]
const FILE_FLAG_WRITE_THROUGH: u32 = 0x80000000;

const MB: u64 = 1024 * 1024;
const GIB: f64 = 1_073_741_824.0;
const BLOCK_SIZE: usize = 4096;
const RANDOM_REGION_BYTES: u64 = 512 * MB;
const MAX_IO_BYTES: usize = 16 * 1024 * 1024;

// WTGBench-like sequential benchmark defaults.
const WTGB_SEQ_CHUNK_BYTES: u64 = 64 * MB;
const WTGB_SEQ_RING_BYTES: u64 = 10 * 1024 * MB;
const WTGB_SEQ_DURATION: Duration = Duration::from_secs(10);
const WTGB_QUICK_SEQ_RING_BYTES: u64 = 2 * 1024 * MB;
const WTGB_QUICK_SEQ_DURATION: Duration = Duration::from_secs(5);
const WTGB_EXTREME_DURATION: Duration = Duration::from_secs(15 * 60);

// WTGBench-like random 4K defaults.
const WTGB_4K_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
const WTGB_4K_POINTS: usize = 30;
const WTGB_QUICK_4K_POINTS: usize = 12;

// WTGBench multi-thread curve defaults.
const WTGB_MT_LEVEL_DURATION: Duration = Duration::from_secs(30);
const WTGB_MT_LEVEL_PAUSE: Duration = Duration::from_secs(20);
const WTGB_MT_LEVELS: [u32; 6] = [1, 2, 4, 8, 16, 32];

// WTGB full sequential defaults.
const FULL_IO_BYTES: u64 = 64 * MB;
const FULL_STEP_BYTES: u64 = 1024 * MB;
const FULL_RESERVED_BYTES: u64 = 100 * MB;

// WTGB scenario defaults.
const SCENARIO_LINE_DURATION: Duration = Duration::from_secs(5);
static BENCHMARK_CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

pub fn request_cancel() {
    BENCHMARK_CANCEL_FLAG.store(true, Ordering::Relaxed);
}

fn clear_cancel_flag() {
    BENCHMARK_CANCEL_FLAG.store(false, Ordering::Relaxed);
}

fn is_cancelled() -> bool {
    BENCHMARK_CANCEL_FLAG.load(Ordering::Relaxed)
}

fn ensure_not_cancelled() -> Result<()> {
    if is_cancelled() {
        Err(crate::AppError::SystemError(
            "Benchmark cancelled by user".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct BenchmarkResult {
    pub mode: String,
    pub write_seq: f64, // MB/s
    pub write_4k: f64,  // MB/s (adjusted)
    pub write_4k_raw: Option<f64>,
    pub write_4k_adjusted: Option<f64>,
    pub write_4k_samples: Vec<TrendPoint>,
    pub thread_results: Vec<ThreadResult>,
    pub full_seq_samples: Vec<Sample>,
    pub scenario_samples: Vec<TrendPoint>,
    pub scenario_total_io: Option<u64>,
    pub scenario_score: Option<f64>,
    pub score: Option<f64>,
    pub grade: Option<String>,
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
pub struct TrendPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct BenchmarkConfig {
    pub target_path: String,
    pub mode: String, // quick | multithread | fullwrite | full | scenario
}

#[derive(Debug, Clone)]
struct ScenarioLine {
    io_sizes: [usize; 10],
    write_proportion: f64,
    seqness: f64,
    threads: usize,
}

fn open_benchmark_file(path: &str) -> Result<File> {
    #[cfg(target_os = "windows")]
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .share_mode(0)
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

fn fill_random(buf: &mut [u8]) {
    rand::thread_rng().fill_bytes(buf);
}

fn ensure_file_region(file: &mut File, region_bytes: u64) -> Result<()> {
    let mut tail = [0u8; BLOCK_SIZE];
    fill_random(&mut tail);
    let end = region_bytes.saturating_sub(BLOCK_SIZE as u64);
    file.seek(SeekFrom::Start(end))?;
    file.write_all(&tail)?;
    file.flush()?;
    Ok(())
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn compute_wtgb_score(seq_mbps: f64, adj_4k_mbps: f64) -> (f64, String) {
    // WTGB score formula.
    let score = adj_4k_mbps + (1.0 + (seq_mbps / 1000.0)).ln();
    let grade = if score > 30.0 {
        "Platinum"
    } else if score > 10.0 {
        "Gold"
    } else if score > 0.8 {
        "Silver"
    } else if score > 0.3 {
        "Bronze"
    } else {
        "Steel"
    };
    (score, grade.to_string())
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
        if GetDiskFreeSpaceExW(
            PCWSTR(path.as_ptr()),
            Some(&mut free),
            Some(&mut total),
            Some(&mut avail),
        )
        .is_ok()
        {
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

fn sequential_wtgb_with_ring(
    path: &str,
    duration: Duration,
    ring_bytes: u64,
) -> Result<(f64, Vec<Sample>, u64)> {
    let _ = std::fs::remove_file(path);
    let mut file = open_benchmark_file(path)?;

    let mut chunk = vec![0u8; WTGB_SEQ_CHUNK_BYTES as usize];
    fill_random(&mut chunk);
    let safe_ring = ring_bytes.max(WTGB_SEQ_CHUNK_BYTES);
    ensure_file_region(&mut file, safe_ring)?;

    let start = Instant::now();
    let mut offset: u64 = 0;
    let mut bytes_written: u64 = 0;
    let mut speeds = Vec::new();
    let mut samples = Vec::new();

    while start.elapsed() < duration {
        ensure_not_cancelled()?;
        let chunk_start = Instant::now();
        let pos = offset % safe_ring;
        file.seek(SeekFrom::Start(pos))?;
        file.write_all(&chunk)?;
        file.flush()?;

        let sec = chunk_start.elapsed().as_secs_f64().max(0.001);
        let mbps = (WTGB_SEQ_CHUNK_BYTES as f64 / 1024.0 / 1024.0) / sec;
        bytes_written += WTGB_SEQ_CHUNK_BYTES;
        offset += WTGB_SEQ_CHUNK_BYTES;
        speeds.push(mbps);
        samples.push(Sample {
            t_ms: start.elapsed().as_millis() as u64,
            value: mbps,
            x_gb: bytes_written as f64 / GIB,
        });
    }

    Ok((mean(&speeds), samples, bytes_written))
}

fn sequential_wtgb(path: &str, duration: Duration) -> Result<(f64, Vec<Sample>, u64)> {
    sequential_wtgb_with_ring(path, duration, WTGB_SEQ_RING_BYTES)
}

fn random_4k_single_wtgb_with_points(
    path: &str,
    point_count: usize,
) -> Result<(f64, f64, Vec<TrendPoint>)> {
    let mut file = open_benchmark_file(path)?;
    ensure_file_region(&mut file, RANDOM_REGION_BYTES)?;
    let points_target = point_count.max(1);

    let mut rng = rand::thread_rng();
    let mut block = [0u8; BLOCK_SIZE];
    fill_random(&mut block);

    let mut points = Vec::with_capacity(points_target);
    let mut trend = Vec::with_capacity(points_target);
    let mut window_ops: u64 = 0;
    let mut window_start = Instant::now();
    let mut elapsed_sec = 0.0;

    while points.len() < points_target {
        ensure_not_cancelled()?;
        let idx = rng.gen_range(0..(RANDOM_REGION_BYTES / BLOCK_SIZE as u64));
        let pos = idx * BLOCK_SIZE as u64;
        file.seek(SeekFrom::Start(pos))?;
        file.write_all(&block)?;
        file.flush()?;
        window_ops += 1;

        if window_start.elapsed() >= WTGB_4K_SAMPLE_INTERVAL {
            let sec = window_start.elapsed().as_secs_f64().max(0.001);
            let mbps = ((window_ops * BLOCK_SIZE as u64) as f64 / 1024.0 / 1024.0) / sec;
            points.push(mbps);
            elapsed_sec += sec;
            trend.push(TrendPoint {
                x: elapsed_sec,
                y: mbps,
            });
            window_ops = 0;
            window_start = Instant::now();
        }
    }

    let avg = mean(&points);
    let mut adjusted = points.clone();
    let half = points.len() / 2;
    adjusted.extend_from_slice(&points[half..]);
    for p in &points {
        if *p < avg * 0.5 {
            adjusted.push(*p);
        }
    }

    Ok((avg, mean(&adjusted), trend))
}

fn random_4k_single_wtgb(path: &str) -> Result<(f64, f64, Vec<TrendPoint>)> {
    random_4k_single_wtgb_with_points(path, WTGB_4K_POINTS)
}

fn random_4k_multi_once(path: &str, threads: u32, duration: Duration) -> Result<f64> {
    let start_flag = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    let mut tmp_paths = Vec::new();

    for idx in 0..threads {
        let f_start = start_flag.clone();
        let f_stop = stop_flag.clone();
        let f_total = total_bytes.clone();
        let file_path = format!("{}.mt{}", path, idx);
        tmp_paths.push(file_path.clone());

        handles.push(thread::spawn(move || -> Result<()> {
            let mut rng = rand::thread_rng();
            let mut file = open_benchmark_file(&file_path)?;
            ensure_file_region(&mut file, RANDOM_REGION_BYTES)?;

            let mut block = [0u8; BLOCK_SIZE];
            fill_random(&mut block);

            while !f_start.load(Ordering::Acquire) {
                if is_cancelled() {
                    return Err(crate::AppError::SystemError(
                        "Benchmark cancelled by user".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }

            while !f_stop.load(Ordering::Relaxed) {
                if is_cancelled() {
                    break;
                }
                let rnd = rng.gen_range(0..(RANDOM_REGION_BYTES / BLOCK_SIZE as u64));
                file.seek(SeekFrom::Start(rnd * BLOCK_SIZE as u64))?;
                file.write_all(&block)?;
                file.flush()?;
                f_total.fetch_add(BLOCK_SIZE as u64, Ordering::Relaxed);
            }

            Ok(())
        }));
    }

    start_flag.store(true, Ordering::Release);
    let wait_start = Instant::now();
    while wait_start.elapsed() < duration {
        if is_cancelled() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    stop_flag.store(true, Ordering::Relaxed);

    let mut worker_error: Option<crate::AppError> = None;
    for h in handles {
        match h.join() {
            Ok(inner) => {
                if let Err(e) = inner {
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

    for p in tmp_paths {
        let _ = std::fs::remove_file(p);
    }

    if let Some(e) = worker_error {
        return Err(e);
    }
    ensure_not_cancelled()?;

    let bytes = total_bytes.load(Ordering::Relaxed);
    Ok((bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64().max(0.001))
}

fn random_4k_multithread_curve(path: &str) -> Result<Vec<ThreadResult>> {
    let mut out = Vec::with_capacity(WTGB_MT_LEVELS.len());
    for (idx, t) in WTGB_MT_LEVELS.iter().enumerate() {
        ensure_not_cancelled()?;
        let mbps = random_4k_multi_once(path, *t, WTGB_MT_LEVEL_DURATION)?;
        out.push(ThreadResult {
            threads: *t,
            mb_s: mbps,
        });

        if idx + 1 < WTGB_MT_LEVELS.len() {
            let pause_start = Instant::now();
            while pause_start.elapsed() < WTGB_MT_LEVEL_PAUSE {
                ensure_not_cancelled()?;
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(out)
}

/// WTGB-style full sequential test:
/// - 64MB IO size
/// - 1GB per measurement step
/// - writes through almost all free space
fn sequential_full(path: &str, target_bytes: u64) -> Result<(f64, Vec<Sample>, u64)> {
    if target_bytes < FULL_IO_BYTES {
        return Err(crate::AppError::InvalidParameter(
            "Not enough free space for full benchmark".to_string(),
        ));
    }

    let _ = std::fs::remove_file(path);
    let mut file = open_benchmark_file(path)?;
    let mut block = vec![0u8; FULL_IO_BYTES as usize];
    fill_random(&mut block);

    let total_steps = (target_bytes / FULL_STEP_BYTES).max(1);
    let chunks_per_step = FULL_STEP_BYTES / FULL_IO_BYTES;
    let global_start = Instant::now();
    let mut samples = Vec::new();
    let mut bytes_written: u64 = 0;
    let mut speeds = Vec::new();

    for step in 0..total_steps {
        ensure_not_cancelled()?;
        let step_start = Instant::now();
        let step_begin = bytes_written;
        for _ in 0..chunks_per_step {
            ensure_not_cancelled()?;
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

        let sec = step_start.elapsed().as_secs_f64().max(0.001);
        let speed = (step_written as f64 / 1024.0 / 1024.0) / sec;

        // Same as WTGBench: skip the first step on trend graph.
        if step > 0 {
            samples.push(Sample {
                t_ms: global_start.elapsed().as_millis() as u64,
                value: speed,
                x_gb: bytes_written as f64 / GIB,
            });
            speeds.push(speed);
        }

        if bytes_written >= target_bytes {
            break;
        }
    }

    if speeds.is_empty() {
        let sec = global_start.elapsed().as_secs_f64().max(0.001);
        let total = (bytes_written as f64 / 1024.0 / 1024.0) / sec;
        return Ok((total, samples, bytes_written));
    }

    Ok((mean(&speeds), samples, bytes_written))
}

fn scenario_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("Scenarios").join("normal_web.csv"));
        paths.push(
            cwd.join("WTGBench")
                .join("Scenarios")
                .join("normal_web.csv"),
        );
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("Scenarios").join("normal_web.csv"));
            paths.push(
                dir.join("resources")
                    .join("Scenarios")
                    .join("normal_web.csv"),
            );
            paths.push(
                dir.join("..")
                    .join("resources")
                    .join("Scenarios")
                    .join("normal_web.csv"),
            );
        }
    }

    let mut dedup = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|p| dedup.insert(p.to_string_lossy().to_string()))
        .collect()
}

fn parse_scenario_csv(path: &Path) -> Result<Vec<ScenarioLine>> {
    let content = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 13 {
            continue;
        }

        let mut io_sizes = [BLOCK_SIZE; 10];
        for i in 0..10 {
            let parsed = parts[i].parse::<usize>().unwrap_or(BLOCK_SIZE);
            let clamped = parsed.clamp(BLOCK_SIZE, MAX_IO_BYTES);
            io_sizes[i] = clamped - (clamped % BLOCK_SIZE);
        }

        let write_proportion = parts[10].parse::<f64>().unwrap_or(0.5).clamp(0.0, 1.0);
        let seqness = parts[11].parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0);
        let threads = parts[12].parse::<usize>().unwrap_or(1).clamp(1, 32);

        out.push(ScenarioLine {
            io_sizes,
            write_proportion,
            seqness,
            threads,
        });
    }
    Ok(out)
}

fn built_in_normal_web_scenario() -> Vec<ScenarioLine> {
    // Fallback profile approximating mixed desktop/web workloads (15 minutes total).
    let base = vec![
        ScenarioLine {
            io_sizes: [
                4096, 4096, 4096, 8192, 8192, 16384, 4096, 4096, 32768, 65536,
            ],
            write_proportion: 0.55,
            seqness: 0.10,
            threads: 4,
        },
        ScenarioLine {
            io_sizes: [
                4096, 4096, 8192, 8192, 16384, 16384, 32768, 65536, 131072, 262144,
            ],
            write_proportion: 0.40,
            seqness: 0.18,
            threads: 6,
        },
        ScenarioLine {
            io_sizes: [
                4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 262144, 131072,
            ],
            write_proportion: 0.35,
            seqness: 0.30,
            threads: 8,
        },
        ScenarioLine {
            io_sizes: [
                4096, 4096, 4096, 4096, 8192, 8192, 16384, 16384, 32768, 32768,
            ],
            write_proportion: 0.65,
            seqness: 0.05,
            threads: 3,
        },
        ScenarioLine {
            io_sizes: [
                65536, 131072, 262144, 524288, 1048576, 262144, 131072, 65536, 32768, 16384,
            ],
            write_proportion: 0.28,
            seqness: 0.55,
            threads: 5,
        },
        ScenarioLine {
            io_sizes: [
                4096, 8192, 12288, 16384, 24576, 32768, 65536, 131072, 8192, 4096,
            ],
            write_proportion: 0.50,
            seqness: 0.22,
            threads: 7,
        },
        ScenarioLine {
            io_sizes: [
                4096, 4096, 8192, 8192, 12288, 16384, 24576, 32768, 65536, 131072,
            ],
            write_proportion: 0.58,
            seqness: 0.12,
            threads: 10,
        },
        ScenarioLine {
            io_sizes: [
                16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 1048576, 524288,
            ],
            write_proportion: 0.20,
            seqness: 0.72,
            threads: 4,
        },
        ScenarioLine {
            io_sizes: [
                4096, 4096, 4096, 8192, 8192, 16384, 32768, 65536, 131072, 262144,
            ],
            write_proportion: 0.48,
            seqness: 0.20,
            threads: 12,
        },
    ];

    let mut lines = Vec::new();
    for _ in 0..20 {
        lines.extend(base.iter().cloned());
    }
    lines
}

fn load_scenario_lines() -> Vec<ScenarioLine> {
    for candidate in scenario_candidates() {
        if candidate.exists() {
            if let Ok(lines) = parse_scenario_csv(&candidate) {
                if !lines.is_empty() {
                    return lines;
                }
            }
        }
    }
    built_in_normal_web_scenario()
}

fn scenario_line_worker(path: String, line: ScenarioLine) -> Result<u64> {
    let mut file = open_benchmark_file(&path)?;
    ensure_file_region(&mut file, RANDOM_REGION_BYTES)?;

    let mut rng = rand::thread_rng();
    let max_io = *line
        .io_sizes
        .iter()
        .max()
        .unwrap_or(&BLOCK_SIZE)
        .max(&BLOCK_SIZE);
    let mut write_buf = vec![0u8; max_io];
    fill_random(&mut write_buf);
    let mut read_buf = vec![0u8; max_io];
    let mut prev_pos: u64 = 0;
    let mut ops: u64 = 0;

    let start = Instant::now();
    while start.elapsed() < SCENARIO_LINE_DURATION {
        ensure_not_cancelled()?;
        let mut len = line.io_sizes[rng.gen_range(0..line.io_sizes.len())];
        if len < BLOCK_SIZE {
            len = BLOCK_SIZE;
        }
        len -= len % BLOCK_SIZE;
        if len == 0 {
            continue;
        }

        let max_slots = RANDOM_REGION_BYTES / len as u64;
        let rand_slot = rng.gen_range(0..=max_slots);
        let rand_pos = rand_slot * len as u64;
        let pos = if rng.gen_bool(line.seqness) {
            prev_pos
        } else {
            rand_pos
        };

        let safe_pos = if pos + len as u64 > RANDOM_REGION_BYTES {
            0
        } else {
            pos
        };
        file.seek(SeekFrom::Start(safe_pos))?;

        if rng.gen_bool(line.write_proportion) {
            file.write_all(&write_buf[..len])?;
            file.flush()?;
        } else {
            let _ = file.read(&mut read_buf[..len])?;
        }

        prev_pos = (safe_pos + len as u64) % RANDOM_REGION_BYTES;
        ops += 1;
    }

    Ok(ops)
}

fn scenario_benchmark(path_prefix: &str) -> Result<(u64, Vec<TrendPoint>)> {
    let lines = load_scenario_lines();
    let worker_paths: Vec<String> = (0..32)
        .map(|i| format!("{}.sce{}", path_prefix, i))
        .collect();
    for p in &worker_paths {
        let _ = std::fs::remove_file(p);
        let mut f = open_benchmark_file(p)?;
        ensure_file_region(&mut f, RANDOM_REGION_BYTES)?;
    }

    let mut total_io: u64 = 0;
    let mut trend = Vec::new();
    let global = Instant::now();

    for line in lines {
        ensure_not_cancelled()?;
        let thread_count = line.threads.min(32).max(1);
        let mut handles = Vec::with_capacity(thread_count);
        for path in worker_paths.iter().take(thread_count) {
            let p = path.clone();
            let l = line.clone();
            handles.push(thread::spawn(move || scenario_line_worker(p, l)));
        }

        let mut line_total = 0_u64;
        for h in handles {
            match h.join() {
                Ok(inner) => {
                    line_total += inner?;
                }
                Err(_) => {
                    return Err(crate::AppError::SystemError(
                        "Scenario worker thread panicked".to_string(),
                    ));
                }
            }
        }
        total_io += line_total;

        ensure_not_cancelled()?;
        let line_secs = SCENARIO_LINE_DURATION.as_secs_f64().max(0.001);
        trend.push(TrendPoint {
            x: global.elapsed().as_secs_f64(),
            y: line_total as f64 / line_secs,
        });
    }

    for p in worker_paths {
        let _ = std::fs::remove_file(p);
    }

    Ok((total_io, trend))
}

pub async fn run_benchmark(config: &BenchmarkConfig) -> Result<BenchmarkResult> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err(crate::AppError::Unsupported(
            "Benchmark only implemented on Windows".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        clear_cancel_flag();
        let base = config.target_path.trim_end_matches(['\\', '/']);
        let temp_file = format!("{}\\wtg_bench.bin", base);
        let start = Instant::now();

        let mut result = BenchmarkResult {
            mode: config.mode.clone(),
            write_seq: 0.0,
            write_4k: 0.0,
            write_4k_raw: None,
            write_4k_adjusted: None,
            write_4k_samples: vec![],
            thread_results: vec![],
            full_seq_samples: vec![],
            scenario_samples: vec![],
            scenario_total_io: None,
            scenario_score: None,
            score: None,
            grade: None,
            duration_ms: 0,
            full_written_gb: 0.0,
        };

        let run_outcome: Result<()> = (|| {
            match config.mode.as_str() {
                "multithread" => {
                    let seq = sequential_wtgb(&temp_file, WTGB_SEQ_DURATION)?;
                    let r4k = random_4k_single_wtgb(&temp_file)?;
                    let mt = random_4k_multithread_curve(&temp_file)?;
                    let (score, grade) = compute_wtgb_score(seq.0, r4k.1);

                    result.write_seq = seq.0;
                    result.full_seq_samples = seq.1;
                    result.full_written_gb = seq.2 as f64 / GIB;
                    result.write_4k = r4k.1;
                    result.write_4k_raw = Some(r4k.0);
                    result.write_4k_adjusted = Some(r4k.1);
                    result.write_4k_samples = r4k.2;
                    result.thread_results = mt;
                    result.score = Some(score);
                    result.grade = Some(grade);
                }
                "fullwrite" => {
                    let free = get_free_bytes(&format!("{}\\", base));
                    let target = free.saturating_sub(FULL_RESERVED_BYTES);
                    let aligned = (target / FULL_IO_BYTES) * FULL_IO_BYTES;
                    if aligned < FULL_IO_BYTES {
                        return Err(crate::AppError::InvalidParameter(
                            "Not enough free space for full benchmark (needs at least ~64MB)"
                                .into(),
                        ));
                    }
                    let seq = sequential_full(&temp_file, aligned)?;
                    result.write_seq = seq.0;
                    result.full_seq_samples = seq.1;
                    result.full_written_gb = seq.2 as f64 / GIB;
                }
                "full" => {
                    let seq = sequential_wtgb(&temp_file, WTGB_EXTREME_DURATION)?;
                    let r4k = random_4k_single_wtgb(&temp_file)?;
                    let (score, grade) = compute_wtgb_score(seq.0, r4k.1);

                    result.write_seq = seq.0;
                    result.full_seq_samples = seq.1;
                    result.full_written_gb = seq.2 as f64 / GIB;
                    result.write_4k = r4k.1;
                    result.write_4k_raw = Some(r4k.0);
                    result.write_4k_adjusted = Some(r4k.1);
                    result.write_4k_samples = r4k.2;
                    result.score = Some(score);
                    result.grade = Some(grade);
                }
                "scenario" => {
                    let (total_io, trend) = scenario_benchmark(&temp_file)?;
                    result.scenario_total_io = Some(total_io);
                    result.scenario_score = Some(total_io as f64 / 1000.0);
                    result.scenario_samples = trend;
                }
                _ => {
                    // quick
                    let seq = sequential_wtgb_with_ring(
                        &temp_file,
                        WTGB_QUICK_SEQ_DURATION,
                        WTGB_QUICK_SEQ_RING_BYTES,
                    )?;
                    let r4k = random_4k_single_wtgb_with_points(&temp_file, WTGB_QUICK_4K_POINTS)?;
                    let (score, grade) = compute_wtgb_score(seq.0, r4k.1);

                    result.write_seq = seq.0;
                    result.full_seq_samples = seq.1;
                    result.full_written_gb = seq.2 as f64 / GIB;
                    result.write_4k = r4k.1;
                    result.write_4k_raw = Some(r4k.0);
                    result.write_4k_adjusted = Some(r4k.1);
                    result.write_4k_samples = r4k.2;
                    result.score = Some(score);
                    result.grade = Some(grade);
                }
            }
            ensure_not_cancelled()?;
            Ok(())
        })();

        if let Err(e) = run_outcome {
            let _ = std::fs::remove_file(&temp_file);
            clear_cancel_flag();
            return Err(e);
        }

        result.write_seq = round1(result.write_seq);
        result.write_4k = round1(result.write_4k);
        result.write_4k_raw = result.write_4k_raw.map(round1);
        result.write_4k_adjusted = result.write_4k_adjusted.map(round1);
        result.full_written_gb = round1(result.full_written_gb);
        result.score = result.score.map(round1);
        result.scenario_score = result.scenario_score.map(round1);
        result.thread_results = result
            .thread_results
            .into_iter()
            .map(|mut x| {
                x.mb_s = round1(x.mb_s);
                x
            })
            .collect();
        result.duration_ms = start.elapsed().as_millis() as u64;

        let _ = std::fs::remove_file(&temp_file);
        clear_cancel_flag();
        Ok(result)
    }
}
