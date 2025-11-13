mod bot;
mod executor;
mod test_spec;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "flintmc")]
#[command(about = "Minecraft server testing framework", long_about = None)]
struct Args {
    /// Path to test file or directory
    #[arg(value_name = "PATH")]
    path: PathBuf,

    /// Server address (e.g., localhost:25565)
    #[arg(short, long)]
    server: String,

    /// Recursively search directories for test files
    #[arg(short, long)]
    recursive: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    println!("{}", "FlintMC - Minecraft Testing Framework".green().bold());
    println!();

    // Collect test files
    let test_files = collect_test_files(&args.path, args.recursive)?;

    if test_files.is_empty() {
        eprintln!("{} No test files found at: {}", "Error:".red().bold(), args.path.display());
        std::process::exit(1);
    }

    println!("Found {} test file(s)\n", test_files.len());

    // Connect to server
    let mut executor = executor::TestExecutor::new();
    println!("{} Connecting to {}...", "→".blue(), args.server);
    executor.connect(&args.server).await?;
    println!("{} Connected successfully\n", "✓".green());

    // Load all tests and calculate offsets
    let total_tests = test_files.len();
    let mut tests_with_offsets = Vec::new();

    for (test_index, test_file) in test_files.iter().enumerate() {
        match test_spec::TestSpec::from_file(test_file) {
            Ok(test) => {
                let offset = calculate_test_offset(test_index, total_tests);
                println!("  {} Grid position: {} (offset: [{}, {}, {}])",
                    "→".blue(),
                    format!("[{}/{}]", test_index + 1, total_tests).dimmed(),
                    offset[0], offset[1], offset[2]
                );
                tests_with_offsets.push((test, offset));
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to load test {}: {}",
                    "Error:".red().bold(),
                    test_file.display(),
                    e
                );
                std::process::exit(1);
            }
        }
    }

    println!();

    // Run all tests in parallel using merged timeline
    let results = executor.run_tests_parallel(&tests_with_offsets).await?;

    // Print summary
    println!("\n{}", "═".repeat(60).dimmed());
    println!("{}", "Test Summary".cyan().bold());
    println!("{}", "═".repeat(60).dimmed());

    let total_passed = results.iter().filter(|r| r.success).count();
    let total_failed = results.len() - total_passed;

    for result in &results {
        let status = if result.success {
            "PASS".green().bold()
        } else {
            "FAIL".red().bold()
        };
        println!("  [{}] {}", status, result.test_name);
    }

    println!("\n{} tests run: {} passed, {} failed\n",
        results.len(),
        total_passed.to_string().green(),
        total_failed.to_string().red()
    );

    if total_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn collect_test_files(path: &PathBuf, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut test_files = Vec::new();

    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            test_files.push(path.clone());
        }
    } else if path.is_dir() {
        if recursive {
            collect_json_files_recursive(path, &mut test_files)?;
        } else {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    test_files.push(path);
                }
            }
        }
    }

    test_files.sort();
    Ok(test_files)
}

fn collect_json_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files_recursive(&path, files)?;
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(())
}

/// Calculate grid offset for test at given index
/// Tests are arranged in a square grid centered at (0, 0)
/// Each cell is 16x16 blocks (15 test area + 1 spacing)
fn calculate_test_offset(test_index: usize, total_tests: usize) -> [i32; 3] {
    const CELL_SIZE: i32 = 16; // 15 blocks + 1 spacing

    // Calculate grid size (ceil(sqrt(N)))
    let grid_size = (total_tests as f64).sqrt().ceil() as i32;

    // Calculate position in grid
    let grid_x = (test_index as i32) % grid_size;
    let grid_z = (test_index as i32) / grid_size;

    // Calculate base offset to center the grid at (0, 0)
    let base_offset = -(grid_size * CELL_SIZE) / 2;

    // Calculate world offset for this test
    [
        base_offset + grid_x * CELL_SIZE,
        0,
        base_offset + grid_z * CELL_SIZE,
    ]
}
