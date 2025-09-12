use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use tokio::sync::mpsc;
use risc0_zkvm::{ExecutorEnv, ExecutorImpl, NullSegmentRef, Segment};

const V2_ELF_MAGIC: &[u8] = b"R0BF";

#[derive(Parser, Debug)]
#[command(name = "r0-execute")]
#[command(about = "Execute RISC-V ELF programs locally using zkVM")]
struct Args {
    /// Path to the ELF file to execute
    #[arg(short = 'e', long, default_value = "./elf")]
    elf_path: String,
    
    /// Path to the input file
    #[arg(short = 'i', long, default_value = "./input")]
    input_path: String,
    
    /// Output directory for results
    #[arg(short = 'd', long, default_value = "./output")]
    output_dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocalExecutionResult {
    status: String,
    output: Vec<u8>,
    user_cycles: u64,
    total_cycles: u64,
    segment_count: usize,
    execution_time_ms: u128,
    error: Option<String>,
}

struct ExecutionResult {
    user_cycles: u64,
    total_cycles: u64,
    segment_count: usize,
    output: Vec<u8>,
}

struct LocalExecutor;

impl LocalExecutor {
    fn new() -> LocalExecutor {
        LocalExecutor
    }

    fn read_elf_file(&self, elf_path: &str) -> Result<Vec<u8>> {
        println!("Reading ELF file: {}", elf_path);
        
        let elf_data = std::fs::read(elf_path)
            .with_context(|| format!("Failed to read ELF file from: {}", elf_path))?;
            
        // Validate that it's actually an ELF file
        if elf_data.len() < 4 || &elf_data[0..4] != V2_ELF_MAGIC {
            return Err(anyhow::anyhow!("File {} is not a valid R0 ELF file", elf_path));
        }
        
        println!("Successfully read ELF file: {} bytes", elf_data.len());
        Ok(elf_data)
    }
    
    fn read_input_file(&self, input_path: &str) -> Result<Vec<u8>> {
        println!("Reading input file: {}", input_path);
        
        let input_data = std::fs::read(input_path)
            .with_context(|| format!("Failed to read input file from: {}", input_path))?;
            
        println!("Successfully read input file: {} bytes", input_data.len());
        Ok(input_data)
    }

    async fn execute_locally(&self, elf_path: &str, input_path: &str, output_dir: &str) -> Result<LocalExecutionResult> {
        println!("Executing locally...");
        
        let start_time = std::time::Instant::now();
        
        // Read ELF and input from local files
        let elf_data = self.read_elf_file(elf_path)?;
        let input_data = self.read_input_file(input_path)?;
        
        println!("ELF size: {} bytes", elf_data.len());
        println!("Input data size: {} bytes", input_data.len());
        
        // Execute with zkVM
        let result = self.execute_with_zkvm(&elf_data, &input_data, output_dir).await?;
        
        let execution_time = start_time.elapsed().as_millis();
        
        Ok(LocalExecutionResult {
            status: "completed".to_string(),
            output: result.output,
            user_cycles: result.user_cycles,
            total_cycles: result.total_cycles,
            segment_count: result.segment_count,
            execution_time_ms: execution_time,
            error: None,
        })
    }


    async fn execute_with_zkvm(&self, elf_data: &[u8], input_data: &[u8], output_dir: &str) -> Result<ExecutionResult> {
        let (segment_tx, mut segment_rx) = mpsc::channel::<Segment>(100);
        
        // Clone data and sender for the blocking task
        let elf_data = elf_data.to_vec();
        let input_data = input_data.to_vec();
        let segment_tx_clone = segment_tx.clone();
        
        // Spawn segment writer task
        let output_dir = output_dir.to_string();
        let segment_writer = tokio::spawn(async move {
            // Create output directory if it doesn't exist
            if let Err(e) = fs::create_dir_all(&output_dir) {
                eprintln!("Failed to create output directory: {}", e);
                return 0;
            }
            
            let mut segment_count = 0;
            while let Some(segment) = segment_rx.recv().await {
                segment_count += 1;
                println!("Processing segment {}: index={}", segment_count, segment.index);
                
                // Save segment to file immediately
                let segment_path = Path::new(&output_dir).join(format!("segment_{:04}.bin", segment.index));
                if let Ok(segment_data) = bincode::serialize(&segment) {
                    if let Err(e) = fs::write(&segment_path, segment_data) {
                        eprintln!("Failed to save segment {}: {}", segment.index, e);
                    } else {
                        println!("Saved segment {} to: {}", segment.index, segment_path.display());
                    }
                } else {
                    eprintln!("Failed to serialize segment {}", segment.index);
                }
            }
            segment_count
        });
        
        // Execute in blocking task (similar to reference code)
        let exec_limit = 100_000 * 1024 * 1024;
        let exec_task = tokio::task::spawn_blocking(move || -> Result<(u64, u64, Vec<u8>)> {
            // Build execution environment
            let env = ExecutorEnv::builder()
                .write_slice(&input_data)
                .session_limit(Some(exec_limit)) // 10M cycle limit
                .segment_limit_po2(22)
                .build()
                .context("Failed to build ExecutorEnv")?;
            
            // Create executor from ELF
            let mut exec = ExecutorImpl::from_elf(env, &elf_data)
                .context("Failed to create ExecutorImpl from ELF")?;
            
            // Run with segment callback (similar to reference)
            let session = exec.run_with_callback(|segment| {
                // Send segment to async processor
                if let Err(_) = segment_tx_clone.blocking_send(segment) {
                    println!("Failed to send segment to processor");
                }
                Ok(Box::new(NullSegmentRef {}))
            }).context("Execution failed")?;
            
            // Extract results
            let user_cycles = session.user_cycles;
            let total_cycles = session.total_cycles;
            let output = session.journal
                .map(|j| j.bytes)
                .unwrap_or_else(|| b"no output".to_vec());
                
            Ok((user_cycles, total_cycles, output))
        });
        
        // Wait for execution to complete
        let (user_cycles, total_cycles, output) = exec_task.await
            .context("Failed to join execution task")?
            .context("Execution failed")?;
            
        // Close segment channel and wait for writer
        drop(segment_tx);
        let segment_count = segment_writer.await
            .context("Failed to join segment writer")?;
        
        println!("Execution completed: {} cycles (user: {}), {} segments", 
                total_cycles, user_cycles, segment_count);
        
        Ok(ExecutionResult {
            user_cycles,
            total_cycles,
            segment_count,
            output,
        })
    }


    async fn save_results(&self, result: &LocalExecutionResult, output_dir: &str) -> Result<()> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)
            .context("Failed to create output directory")?;

        // Save execution result as JSON
        let result_path = Path::new(output_dir).join(format!("result.json"));
        let result_json = serde_json::to_string_pretty(result)
            .context("Failed to serialize execution result")?;
        
        fs::write(&result_path, result_json)
            .context("Failed to write result file")?;

        // Save raw output
        let output_path = Path::new(output_dir).join(format!("output.bin"));
        fs::write(&output_path, &result.output)
            .context("Failed to write output file")?;

        println!("Results saved to:");
        println!("  - Result: {}", result_path.display());
        println!("  - Output: {}", output_path.display());

        Ok(())
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("ELF path: {}", args.elf_path);
    println!("Input path: {}", args.input_path);
    println!("Output directory: {}", args.output_dir);

    // Initialize the local executor
    let executor = LocalExecutor::new();

    // Execute locally using file paths
    let result = executor.execute_locally(&args.elf_path, &args.input_path, &args.output_dir).await
        .context("Failed to execute locally")?;

    println!("Execution completed:");
    println!("  - Status: {}", result.status);
    println!("  - User cycles: {}", result.user_cycles);
    println!("  - Total cycles: {}", result.total_cycles);
    println!("  - Segment count: {}", result.segment_count);
    println!("  - Execution time: {}ms", result.execution_time_ms);

    // Save results to local storage
    executor.save_results(&result, &args.output_dir).await
        .context("Failed to save results")?;

    println!("Local execution completed successfully!");

    Ok(())
}
