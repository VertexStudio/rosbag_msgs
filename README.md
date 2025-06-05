# rosbag_msgs

A Rust library and CLI tool for processing and parsing ROS bag (`.bag`) files. Supports extracting, decoding, and navigating messages in a structured way. Useful for data analysis workflows, tooling, or integrating ROS data with Rust projects.

## Features

- Parses ROS bag files, including message definitions and dependencies
- Structured access to message data with Rust types
- CLI for basic bag file inspection and extraction
- Async support via Tokio
- Message registration system for selective processing
- Type-safe value extraction with `FromValue` and `ValueExt` traits

## Getting Started

### Prerequisites

- Rust (2021/2024 edition recommended)
- Cargo package manager

### Installation

Clone the repository:

```sh
git clone <this-repo-url>
cd rosbag_msgs
```

Build the project:

```sh
cargo build --release
```

### Usage

The CLI operates on a given ROS bag file:

```sh
cargo run -- --bag path/to/your.bag
```

The CLI supports standard logging via RUST_LOG for custom log detail level:

```sh
RUST_LOG=debug cargo run -- --bag path/to/your.bag
```

### Example / Quick Try

To quickly try this project, run:

```sh
cargo run -- --bag data/race_1.bag
```

This reads the provided demo bag file in the data/ directory and prints parsed topic/type tree info along with extracted IMU data.

## Library Usage

Add to your Cargo.toml:

```toml
rosbag_msgs = { path = "../rosbag_msgs" }
```

Basic usage example:

```rust
use rosbag_msgs::{BagProcessor, MessageLog, Result, ValueExt};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let mut processor = BagProcessor::new("path/to/bag.bag".into());
    
    // Register for specific message types
    let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
    processor.register_message("sensor_msgs/Imu", sender)?;
    
    // Handle messages asynchronously
    let handler = tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            // Extract data using ValueExt methods
            let accel = msg.data.get_nested_value(&["linear_acceleration"])?;
            let x: f64 = accel.get("x")?;
            println!("IMU acceleration X: {}", x);
        }
    });
    
    // Process the bag
    processor.process_bag().await?;
    handler.await;
    Ok(())
}
```

## Project Layout

- `src/main.rs` — CLI entry point with example message registration
- `src/lib.rs` — Bag processing logic and error handling
- `src/value.rs` — Value extraction helpers and traits
- `data/` — Bag files for testing or examples

## Development & Testing

- **Build**: `cargo build`
- **Run unit tests**: `cargo test`
- **Run clippy/lints**: `cargo clippy`
- **Example with logging**: `RUST_LOG=info cargo run -- --bag data/race_1.bag`

## Message Processing Architecture

The processor works in two passes:

1. **Connection scan**: Parses message type definitions and dependencies
2. **Message processing**: Decodes binary data according to definitions

Supports selective message processing through the registration system, allowing efficient handling of specific message types without processing the entire bag.