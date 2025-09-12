use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use tokio::sync::mpsc;
use risc0_zkvm::{CoprocessorCallback, Digest, ExecutorEnv, ExecutorImpl, NullSegmentRef, ProveKeccakRequest, ProveZkrRequest, Segment};
use boundless_market::input::GuestEnv;

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
    
    /// Only log file sizes without saving files (dry-run mode)
    #[arg(long)]
    dry_run: bool,
}

pub type KeccakState = [u64; 25];

#[derive(Serialize)]
struct SerializableKeccakRequest {
    /// The digest of the claim that this keccak input is expected to produce.
    pub claim_digest: Digest,

    /// The requested size of the keccak proof, in powers of 2.
    pub po2: usize,

    /// The control root which identifies a particular keccak circuit revision.
    pub control_root: Digest,

    /// Input transcript to provide to the keccak circuit.
    pub input: Vec<KeccakState>,
}

impl From<&ProveKeccakRequest> for SerializableKeccakRequest {
    fn from(req: &ProveKeccakRequest) -> Self {
        SerializableKeccakRequest {
            claim_digest: req.claim_digest,
            po2: req.po2,
            control_root: req.control_root,
            input: req.input.clone(),
        }
    }
}

struct Coprocessor {
    keccak_tx: tokio::sync::mpsc::Sender<ProveKeccakRequest>,
}

impl Coprocessor {
    fn new(keccak_tx: tokio::sync::mpsc::Sender<ProveKeccakRequest>) -> Self {
        Self { keccak_tx }
    }
}

impl CoprocessorCallback for Coprocessor {
    fn prove_keccak(&mut self, request: ProveKeccakRequest) -> Result<()> {
        if let Err(_) = self.keccak_tx.blocking_send(request) {
            println!("Failed to send Keccak proof request");
        }
        Ok(())
    }

    fn prove_zkr(&mut self, _request: ProveZkrRequest) -> Result<()> {
        // TODO: Implement ZKR proving when needed
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LocalExecutionResult {
    user_cycles: u64,
    total_cycles: u64,
    segment_count: usize,
    keccak_count: usize,
    execution_time_ms: u128,
    error: Option<String>,
}

struct ExecutionResult {
    user_cycles: u64,
    total_cycles: u64,
    segment_count: usize,
    keccak_count: usize,
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

    async fn execute_locally(&self, elf_path: &str, input_path: &str, output_dir: &str, dry_run: bool) -> Result<LocalExecutionResult> {
        println!("Executing locally...");
        
        let start_time = std::time::Instant::now();
        
        // Read ELF and input from local files
        let elf_data = self.read_elf_file(elf_path)?;
        let input_data = self.read_input_file(input_path)?;
        
        println!("ELF size: {} bytes", elf_data.len());
        println!("Input data size: {} bytes", input_data.len());
        
        // Execute with zkVM
        let result = self.execute_with_zkvm(&elf_data, &input_data, output_dir, dry_run).await?;
        
        let execution_time = start_time.elapsed().as_millis();
        
        Ok(LocalExecutionResult {
            user_cycles: result.user_cycles,
            total_cycles: result.total_cycles,
            segment_count: result.segment_count,
            keccak_count: result.keccak_count,
            execution_time_ms: execution_time,
            error: None,
        })
    }


    async fn execute_with_zkvm(&self, elf_data: &[u8], input_data: &[u8], output_dir: &str, dry_run: bool) -> Result<ExecutionResult> {
        let (segment_tx, mut segment_rx) = mpsc::channel::<Segment>(100);
        let (keccak_tx, mut keccak_rx) = mpsc::channel::<ProveKeccakRequest>(100);
        
        // Clone elf data
        let elf_data = elf_data.to_vec();
        // Decode input data
        let decoded_input_data = GuestEnv::decode(input_data)?.stdin;

        // Spawn segment writer task
        let segment_output_dir = output_dir.to_string();
        let segment_writer = tokio::spawn(async move {
            if !dry_run {
                // Create output directory if it doesn't exist
                if let Err(e) = fs::create_dir_all(&segment_output_dir) {
                    eprintln!("Failed to create output directory: {}", e);
                    return 0;
                }
            }
            
            let mut segment_count = 0;
            while let Some(segment) = segment_rx.recv().await {
                segment_count += 1;
                println!("Processing segment {}: index={}", segment_count, segment.index);
                
                if let Ok(segment_data) = bincode::serialize(&segment) {
                    if dry_run {
                        // Dry run mode: only log the size
                        println!("Segment {} would be saved with size: {} bytes", segment.index, segment_data.len());
                    } else {
                        // Normal mode: save segment to file
                        let segment_path = Path::new(&segment_output_dir).join(format!("segment_{:04}.bin", segment.index));
                        if let Err(e) = fs::write(&segment_path, &segment_data) {
                            eprintln!("Failed to save segment {}: {}", segment.index, e);
                        } else {
                            println!("Saved segment {} to: {} ({} bytes)", segment.index, segment_path.display(), segment_data.len());
                        }
                    }
                } else {
                    eprintln!("Failed to serialize segment {}", segment.index);
                }
            }
            segment_count
        });

        let keccak_output_dir = output_dir.to_string();
        let keccak_writer = tokio::spawn(async move {
            let mut keccak_count = 0;
            while let Some(request) = keccak_rx.recv().await {
                keccak_count += 1;
                println!("Received Keccak proof request: {}", keccak_count);

                let serializable_request = SerializableKeccakRequest::from(&request);
                if let Ok(keccak_data) = bincode::serialize(&serializable_request) {
                    if dry_run {
                        // Dry run mode: only log the size
                        println!("Keccak proof request {} would be saved with size: {} bytes", keccak_count, keccak_data.len());
                    } else {
                        // Normal mode: save keccak request to file
                        let keccak_path = Path::new(&keccak_output_dir).join(format!("keccak_{:04}.bin", keccak_count));
                        if let Err(e) = fs::write(&keccak_path, &keccak_data) {
                            eprintln!("Failed to save Keccak proof request {}: {}", keccak_count, e);
                        } else {
                            println!("Saved Keccak proof request {} to: {} ({} bytes)", keccak_count, keccak_path.display(), keccak_data.len());
                        }
                    }
                } else {
                    eprintln!("Failed to serialize Keccak proof request {}", keccak_count);
                }
            }
            keccak_count
        });
        
        // Execute in blocking task (similar to reference code)
        let exec_limit = 100_000 * 1024 * 1024;
        let coproc = Coprocessor::new(keccak_tx);

        let exec_task = tokio::task::spawn_blocking(move || -> Result<(u64, u64)> {
            // Build execution environment
            let env = ExecutorEnv::builder()
                .write_slice(&decoded_input_data)
                .session_limit(Some(exec_limit)) // 10M cycle limit
                .coprocessor_callback(coproc)
                .segment_limit_po2(23)
                .build()?;
            
            // Create executor from ELF
            let mut exec = ExecutorImpl::from_elf(env, &elf_data)
                .context("Failed to create ExecutorImpl from ELF")?;
            
            // Run with segment callback (similar to reference)
            let session = exec.run_with_callback(|segment| {
                // Send segment to async processor
                if let Err(_) = segment_tx.blocking_send(segment) {
                    println!("Failed to send segment to processor");
                }
                Ok(Box::new(NullSegmentRef {}))
            }).context("Execution failed")?;
            
            drop(segment_tx);

            Ok((session.user_cycles, session.total_cycles))
        });
        
        // Wait for execution to complete
        let (user_cycles, total_cycles) = exec_task.await
            .context("Failed to join execution task")?
            .context("Execution failed")?;

        let segment_count = segment_writer.await
            .context("Failed to join segment writer")?;
        
        let keccak_count = keccak_writer.await
            .context("Failed to join keccak writer")?;

        println!("Execution completed: {} cycles (user: {}), {} segments", 
                total_cycles, user_cycles, segment_count);

        Ok(ExecutionResult {
            user_cycles: user_cycles,
            total_cycles: total_cycles,
            segment_count,
            keccak_count,
        })
    }


    async fn save_results(&self, result: &LocalExecutionResult, output_dir: &str, dry_run: bool) -> Result<()> {
        if dry_run {
            // Dry run mode: skip saving
            return Ok(());
        }
        
        // Normal mode: create directory and save files
        fs::create_dir_all(output_dir)
            .context("Failed to create output directory")?;

        // Save execution result as JSON
        let result_path = Path::new(output_dir).join(format!("result.json"));
        let result_json = serde_json::to_string_pretty(result)
            .context("Failed to serialize execution result")?;
        
        fs::write(&result_path, &result_json)
            .context("Failed to write result file")?;

        println!("Results saved to:");
        println!("  - Result: {} ({} bytes)", result_path.display(), result_json.len());

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
    let result = executor.execute_locally(&args.elf_path, &args.input_path, &args.output_dir, args.dry_run).await
        .context("Failed to execute locally")?;

    println!("Execution completed:");
    println!("  - User cycles: {}", result.user_cycles);
    println!("  - Total cycles: {}", result.total_cycles);
    println!("  - Segment count: {}", result.segment_count);
    println!("  - Keccak count: {}", result.keccak_count);
    println!("  - Execution time: {}ms", result.execution_time_ms);

    // Save results to local storage
    executor.save_results(&result, &args.output_dir, args.dry_run).await
        .context("Failed to save results")?;

    if args.dry_run {
        println!("Local execution completed successfully (dry-run mode - no files saved)!");
    } else {
        println!("Local execution completed successfully!");
    }

    Ok(())
}
