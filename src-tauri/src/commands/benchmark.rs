use crate::Result;
use crate::services::benchmark;
pub use crate::services::benchmark::BenchmarkResult;

/// Run disk benchmark on target path (e.g., "E:\\")
#[tauri::command]
pub async fn run_benchmark(target_path: String, mode: Option<String>) -> Result<BenchmarkResult> {
    let config = benchmark::BenchmarkConfig {
        target_path,
        mode: mode.unwrap_or_else(|| "quick".to_string()),
    };
    benchmark::run_benchmark(&config).await
}

#[tauri::command]
pub fn cancel_benchmark() -> Result<()> {
    benchmark::request_cancel();
    Ok(())
}
