# R0-Execute

A Rust application for fetching and executing Boundless Network proof requests locally using the official Boundless SDK.

## Features

- Fetch proof requests by ID from the Boundless Network
- Execute requests locally (simulated execution for demo purposes)
- Save execution results, receipts, and journals to local storage
- Built with the official Boundless Market SDK
- Command-line interface with configurable options

## Installation

1. Make sure you have Rust installed on your system
2. Clone or navigate to this directory
3. Build the application:

```bash
cargo build --release
```

## Usage

### Basic Usage

Fetch and execute a proof request by ID:

```bash
cargo run -- --request-id "0x1234567890abcdef1234567890abcdef12345678"
```

### Advanced Usage

```bash
cargo run -- \
  --request-id "0x1234567890abcdef1234567890abcdef12345678" \
  --rpc-url "https://eth-mainnet.alchemyapi.io/v2/your-api-key" \
  --order-stream-url "https://order-stream.boundless.network" \
  --output-dir "./my-results" \
  --private-key "your-private-key-for-auth"
```

### Command Line Options

- `-r, --request-id <REQUEST_ID>`: Request ID to fetch and execute (required)
  - Can be a transaction hash (0x prefixed, 66 chars) or request digest
- `--rpc-url <RPC_URL>`: Ethereum RPC URL (default: https://eth-mainnet.alchemyapi.io/v2/your-api-key)
- `-s, --order-stream-url <ORDER_STREAM_URL>`: Order stream service URL (default: https://order-stream.boundless.network)
- `-d, --output-dir <OUTPUT_DIR>`: Output directory for results (default: ./results)
- `-k, --private-key <PRIVATE_KEY>`: Private key for authentication (optional)

## Output

The application creates multiple files in the output directory:

1. `{request_id}_result.json`: Execution metadata including:
   - Request ID
   - Execution status
   - Cycles used
   - Execution time
   - Receipt availability
   - Any errors

2. `{request_id}_output.bin`: Raw binary output from the execution

3. `{request_id}_receipt.bin`: Receipt data (if available)

4. `{request_id}_journal.bin`: Journal data (if available)

## Example Output

```
Fetching request: 0x1234567890abcdef1234567890abcdef12345678
RPC URL: https://eth-mainnet.alchemyapi.io/v2/your-api-key
Order Stream URL: https://order-stream.boundless.network
Output directory: ./results

Proof request fetched successfully:
  - Request ID: TxHash(0x1234567890abcdef1234567890abcdef12345678)
  - Image ID: 0xabcdef1234567890abcdef1234567890abcdef12
  - Program size: 2048 bytes
  - Input size: 128 bytes

Executing proof request locally...
Program size: 2048 bytes
Input size: 128 bytes

Execution completed:
  - Status: completed
  - Cycles used: 1020480
  - Execution time: 204ms
  - Receipt available: true

Results saved to:
  - Result: ./results/TxHash(0x1234567890abcdef1234567890abcdef12345678)_result.json
  - Output: ./results/TxHash(0x1234567890abcdef1234567890abcdef12345678)_output.bin

Receipt and journal saved:
  - Receipt: ./results/TxHash(0x1234567890abcdef1234567890abcdef12345678)_receipt.bin
  - Journal: ./results/TxHash(0x1234567890abcdef1234567890abcdef12345678)_journal.bin

Request execution completed successfully!
```

## Implementation Notes

This is a demo implementation that simulates request execution. In a production environment, you would:

1. Replace the `simulate_execution` function with actual RISC Zero zkVM execution
2. Implement proper error handling and retry logic for network operations
3. Add support for different execution environments and resource limits
4. Implement proper logging and monitoring

## Dependencies

- `tokio`: Async runtime
- `serde`: Serialization/deserialization
- `anyhow`: Error handling
- `clap`: Command-line argument parsing
- `uuid`: UUID handling
- `boundless-market`: Official Boundless Network SDK (from GitHub)
- `alloy`: Ethereum development toolkit
- `alloy-primitives`: Ethereum primitive types

## Real zkVM Integration

To integrate with actual RISC Zero zkVM execution, replace the `simulate_execution` method with something like:

```rust
use risc0_zkvm::{ExecutorEnv, ExecutorImpl};

async fn execute_with_zkvm(&self, program: &[u8], input_data: &[u8]) -> Result<ExecutionResult> {
    let env = ExecutorEnv::builder()
        .write_slice(input_data)
        .build()?;
    
    let mut exec = ExecutorImpl::from_elf(env, program)?;
    let session = exec.run()?;
    
    Ok(ExecutionResult {
        output: session.journal.unwrap().bytes,
        cycles: session.total_cycles,
    })
}
```

## Security Notes

- Private keys should be handled securely (consider using environment variables)
- Validate all inputs from the network before execution
- Use proper RPC endpoints for production (not default placeholder URLs)
- Implement rate limiting and proper error handling for network requests