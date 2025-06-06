# rosbag_msgs

A Rust library and CLI tool for processing and parsing ROS bag (`.bag`) files. Supports extracting, decoding, and navigating messages with filtering capabilities for data analysis workflows, tooling, or integrating ROS data with Rust projects.

## Features

- Bag file parsing with message definitions and dependency resolution
- Filtering by message types, topics, or both
- Metadata inspection with ASCII tree visualization
- Selective processing for performance optimization
- Type-safe value extraction with `FromValue` and `ValueExt` traits
- Async support via Tokio for concurrent message handling
- Configurable debug logging
- JSON output for structured data analysis

## CLI Usage

### Quick Start

```bash
# Build the project
cargo build --release

# Show help and all available options
cargo run -- --help

# Basic usage examples
cargo run -- --bag data/race_1.bag --metadata
cargo run -- --bag data/race_1.bag --messages "sensor_msgs/Imu"
cargo run -- --bag data/race_1.bag --topics "/camera/imu"
cargo run -- --bag data/race_1.bag --topics "/camera/imu" --max 5
cargo run -- --bag data/race_1.bag --topics "/camera/imu" --start 10 --duration 5
```

### Metadata Inspection

Show bag file structure and message counts:

```bash
# Show all connections with message structure trees
cargo run -- --bag data/race_1.bag --metadata
```

Output format:

```
topic: /camera/imu
type: sensor_msgs/Imu
        |      header: std_msgs/Header Unit
        |      orientation: geometry_msgs/Quaternion Unit
        |      orientation_covariance: float64 Array(9)
        |      angular_velocity: geometry_msgs/Vector3 Unit
        |      angular_velocity_covariance: float64 Array(9)
        |      linear_acceleration: geometry_msgs/Vector3 Unit
        |      linear_acceleration_covariance: float64 Array(9)
        |- std_msgs/Header
        |      seq: uint32 Unit
        |      stamp: time Unit
        |      frame_id: string Unit
        |- geometry_msgs/Quaternion
        |      x: float64 Unit
        |      y: float64 Unit
        |      z: float64 Unit
        |      w: float64 Unit
        |- geometry_msgs/Vector3
        |      x: float64 Unit
        |      y: float64 Unit
        |      z: float64 Unit
....................................................................................................
Processing completed!
Total messages: 12213
Message counts by type:
  sensor_msgs/Imu: 5679
  nav_msgs/Odometry: 5679
  sensor_msgs/Image: 855
Message counts by topic:
  /camera/imu: 5679
  /camera/odom/sample: 5679
  /camera/fisheye2/image_raw: 855
```

Metadata-only mode scans and counts all messages but skips parsing for faster execution.

### Message Type Filtering

Extract and process specific message types:

```bash
# Single message type
cargo run -- --bag data/race_1.bag --messages "sensor_msgs/Imu"

# Multiple message types
cargo run -- --bag data/race_1.bag --messages "sensor_msgs/Imu,nav_msgs/Odometry"

# Combine with metadata
cargo run -- --bag data/race_1.bag --metadata --messages "sensor_msgs/Imu"
```

### Topic Filtering

Filter by topic names:

```bash
# Single topic
cargo run -- --bag data/race_1.bag --topics "/camera/imu"

# Multiple topics
cargo run -- --bag data/race_1.bag --topics "/camera/imu,/camera/odom/sample"

# Combine with metadata
cargo run -- --bag data/race_1.bag --metadata --topics "/camera/imu"
```

### Combined Filtering

Mix message type and topic filters:

```bash
# Process specific message types AND specific topics
cargo run -- --bag data/race_1.bag --messages "sensor_msgs/Image" --topics "/camera/imu"

# Metadata with selective processing
cargo run -- --bag data/race_1.bag --metadata --messages "sensor_msgs/Imu" --topics "/camera/odom/sample"
```

### Temporal Filtering

Filter messages by time range using `--start` and `--duration` options:

```bash
# Process first 5 seconds of the bag
cargo run -- --bag data/race_1.bag --topics "/camera/imu" --duration 5

# Skip first 10 seconds, then process next 5 seconds
cargo run -- --bag data/race_1.bag --topics "/camera/imu" --start 10 --duration 5

# Process from 15 seconds to end of bag
cargo run -- --bag data/race_1.bag --topics "/camera/imu" --start 15

# Combine with metadata to see impact on processing stats
cargo run -- --bag data/race_1.bag --metadata --start 5 --duration 10

# Fractional seconds and multiple topics
cargo run -- --bag data/race_1.bag --topics "/camera/imu,/camera/odom/sample" --start 2.5 --duration 0.5
```

**Time parameters:**
- `--start <seconds>`: Start time offset from bag beginning (optional)
- `--duration <seconds>`: Duration from start time (optional)
- Times are relative to the first message timestamp in the bag
- Both parameters accept decimal values (e.g., `--start 2.5`)
- Works with all other filters (message types, topics, max limits)
- Processing stats show filtered vs. total message counts

**Example output with temporal filtering:**
```
Processing completed!
Total messages: 12213
Total processed: 981
Processing time: 45ms
Bag duration: 28.492s
Bag start time: 1654544051.367208004
Bag end time: 1654544079.859084368
Message counts by topic:
  /camera/imu: 5679
```
Shows 981 processed out of 12,213 total messages using `--duration 5`.

### Processing Modes

The CLI optimizes automatically:

- **Metadata-only** (`--metadata` alone): Scans connections and counts messages, skips parsing
- **Selective processing** (with filters): Only processes matching messages
- **Full processing** (no filters): Processes entire bag file

### Debug Output

Enable logging for development:

```bash
# Library debug logs
RUST_LOG=debug cargo run -- --bag data/race_1.bag --metadata

# Verbose per-message traces
RUST_LOG=trace cargo run -- --bag data/race_1.bag --messages "sensor_msgs/Imu"
```

## Library Usage

### Basic Setup

Add to your `Cargo.toml`:

```toml
[dependencies]
rosbag_msgs = { path = "../rosbag_msgs" }
tokio = { version = "1", features = ["full"] }
```

### Message Processing

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
            // Extract nested data using ValueExt
            if let Ok(accel) = msg.data.get_nested_value(&["linear_acceleration"]) {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    accel.get::<f64>("x"),
                    accel.get::<f64>("y"), 
                    accel.get::<f64>("z")
                ) {
                    println!("IMU acceleration: x={:.3}, y={:.3}, z={:.3}", x, y, z);
                }
            }
        }
    });
    
    // Process the bag
    processor.process_bag(None, None, None, None).await?;
    handler.await.unwrap();
    Ok(())
}
```

### Topic-Based Processing

```rust
use rosbag_msgs::{BagProcessor, MessageLog, Result};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let mut processor = BagProcessor::new("data/race_1.bag".into());
    
    // Register for specific topics
    let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
    processor.register_topic("/camera/imu", sender)?;
    
    let handler = tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Message from {}: {} (type: {})", 
                msg.topic, msg.time, msg.msg_path);
        }
    });
    
    processor.process_bag(None, None, None, None).await?;
    handler.await.unwrap();
    Ok(())
}
```

### Metadata Analysis

```rust
use rosbag_msgs::{BagProcessor, MetadataEvent, Result};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let mut processor = BagProcessor::new("data/race_1.bag".into());
    
    // Setup metadata channel
    let (meta_sender, mut meta_receiver) = mpsc::channel::<MetadataEvent>(100);
    
    let meta_handler = tokio::spawn(async move {
        while let Some(event) = meta_receiver.recv().await {
            match event {
                MetadataEvent::ConnectionDiscovered(conn) => {
                    println!("Found topic: {} (type: {})", conn.topic, conn.message_type);
                    // conn.format_structure() provides the ASCII tree
                }
                MetadataEvent::ProcessingCompleted(stats) => {
                    println!("Total messages: {}", stats.total_messages);
                    for (msg_type, count) in stats.message_counts {
                        println!("  {}: {}", msg_type, count);
                    }
                }
                _ => {}
            }
        }
    });
    
    // Process with metadata reporting
    processor.process_bag(Some(meta_sender), None, None, None).await?;
    meta_handler.await.unwrap();
    Ok(())
}
```

### Temporal Filtering

```rust
use rosbag_msgs::{BagProcessor, MessageLog, Result};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let mut processor = BagProcessor::new("data/race_1.bag".into());
    
    let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
    processor.register_topic("/camera/imu", sender)?;
    
    let handler = tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Message at {}: {}", msg.time, msg.msg_path);
        }
    });
    
    // Process only messages from 10s to 15s (5-second window)
    processor.process_bag(None, None, Some(10.0), Some(5.0)).await?;
    handler.await.unwrap();
    Ok(())
}
```

## Use Cases

### Data Analysis
- Sensor data extraction by message type or topic
- Message frequency analysis by both type and topic without processing overhead
- Bag file inspection and structure analysis

### Development & Debugging
- Message structure exploration for unfamiliar message types
- Topic discovery in bag files
- Processing time measurement for specific message types

### Integration
- Selective data extraction for downstream processing
- ROS data conversion to JSON format
- Topic replay from recorded data

## Performance

- **Metadata scanning**: Connection discovery and message counting without parsing overhead
- **Selective processing**: Only processes registered message types/topics
- **Memory usage**: Streaming processing, does not load entire bag into memory
- **Concurrency**: Async message handling with configurable buffer sizes

## Project Structure

- `src/main.rs` — CLI interface with filtering options
- `src/lib.rs` — Core bag processing engine
- `src/value.rs` — Type-safe value extraction utilities
- `data/` — Example bag files for testing
- `examples/` — Usage examples

## Development

### Building and Testing

```bash
# Build
cargo build --release

# Run tests
cargo test

# Linting
cargo clippy

# Example with debug logging
RUST_LOG=debug cargo run -- --bag data/race_1.bag --metadata
```

### Architecture

Two-pass processing:

1. **Connection Discovery**: Parses message type definitions and builds dependency trees
2. **Selective Processing**: Processes messages matching registered filters

Benefits:
- Metadata extraction without parsing overhead
- Efficient processing of large bag files
- Accurate statistics reflecting actual work performed

## License

[Add your license information here]