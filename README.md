# rosbag_msgs

A Rust library, CLI tool, and MCP server for processing and parsing ROS bag (`.bag`) files. Supports extracting, decoding, and navigating messages with filtering capabilities for data analysis workflows, AI-assisted robotics analysis, and integrating ROS data with Rust projects.

## Features

- Bag file parsing with message definitions and dependency resolution
- Filtering by message types, topics, or both
- Bag info inspection with statistics and structure visualization
- Selective processing for performance optimization
- Type-safe value extraction with `FromValue` and `ValueExt` traits
- Async support via Tokio for concurrent message handling
- Configurable debug logging
- JSON output for structured data analysis
- **MCP server** for AI-assisted analysis through LLMs and assistants
- **Dynamic resources** for parameterized bag file access
- **Smart prompts** for common robotics analysis workflows

## CLI Usage

### Quick Start

```bash
# Build the project
cargo build --release

# Show help and all available options
cargo run -- --help

# Basic usage examples
cargo run -- info --bag data/race_1.bag
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu"
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu"
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu" --limit 5
cargo run -- images --bag data/race_1.bag --topic "/camera/fisheye2/image_raw"
```

### Bag Info Inspection

Show bag file structure and message counts:

```bash
# Show basic bag info with statistics and topics
cargo run -- info --bag data/race_1.bag

# Show detailed info with message definitions
cargo run -- info --bag data/race_1.bag --definitions
```

Output format:

```
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

Topics and message types:
  /camera/imu: sensor_msgs/Imu
  /camera/odom/sample: nav_msgs/Odometry
  /camera/fisheye2/image_raw: sensor_msgs/Image
```

When using `--definitions`, detailed message type structures are also shown:

```
Message definitions:

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
```

### Message Type Filtering

Extract and process specific message types:

```bash
# Single message type
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu"

# Multiple message types
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu,nav_msgs/Odometry"

# With pagination
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu" --limit 5
```

### Topic Filtering

Filter by topic names:

```bash
# Single topic
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu"

# Multiple topics
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu,/camera/odom/sample"

# With pagination
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu" --offset 10 --limit 5
```

### Combined Filtering

Mix message type and topic filters:

```bash
# Process specific message types AND specific topics
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Image" --topics "/camera/imu"

# With pagination for performance
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu" --topics "/camera/odom/sample" --limit 10
```

### Pagination

Process large datasets in manageable chunks using pagination:

```bash
# First 5 messages from a topic
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu" --limit 5

# Skip first 10 messages, get next 5
cargo run -- messages --bag data/race_1.bag --topics "/camera/imu" --offset 10 --limit 5

# Page through IMU data  
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu" --offset 0 --limit 10
cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu" --offset 10 --limit 10
```

**Pagination parameters:**
- `--offset <N>`: Skip N messages before processing (defaults to 0)
- `--limit <N>`: Maximum messages to process (defaults to 1)
- Works with all filters (message types, topics)
- Pagination info shows actual results for navigation

**Example output with pagination:**
```
Pagination: Offset: 10 | Limit: 2 | Returned: 2 | Total: 5679


sensor_msgs/Imu [1] `/camera/imu`

- header: 
  - seq: 10
  - stamp: 1654544051.511655331s
  - frame_id: "camera_imu_optical_frame"
- orientation: 
  - x: 0.000
  - y: 0.000
  - z: 0.000
  - w: 0.000
- angular_velocity: 
  - x: 0.001
  - y: -0.004
  - z: -0.002
- linear_acceleration: 
  - x: -0.134
  - y: 8.682
  - z: 3.939
```
Shows messages 11-12 out of 5,679 total IMU messages.

### Processing Modes

The CLI provides different analysis modes:

- **Bag info** (`info` command): Shows bag structure, topics, message counts, and optional message definitions
- **Selective processing** (with filters): Only processes matching message types or topics
- **Paginated processing** (with offset/limit): Process large datasets in chunks

### Debug Output

Enable logging for development:

```bash
# Library debug logs
RUST_LOG=debug cargo run -- info --bag data/race_1.bag

# Verbose per-message traces
RUST_LOG=trace cargo run -- messages --bag data/race_1.bag --messages "sensor_msgs/Imu"
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
    processor.process_bag(None, Some(0), Some(10), None, None).await?;
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
    
    processor.process_bag(None, Some(0), Some(10), None, None).await?;
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
    processor.process_bag(Some(meta_sender), None, None, None, None).await?;
    meta_handler.await.unwrap();
    Ok(())
}
```

### Pagination Processing

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
    
    // Process messages with pagination: skip first 10, get next 5
    let pagination = processor.process_bag(None, Some(10), Some(5), None, None).await?;
    if let Some(info) = pagination {
        println!("Processed {} of {} messages", info.returned_count, info.total);
    }
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
- `mcp_server/` — Model Context Protocol server implementation
  - `src/main.rs` — MCP server entry point
  - `src/toolbox.rs` — MCP tools, resources, and prompts
  - `src/descriptions/` — Tool documentation files
- `data/` — Example bag files for testing
- `examples/` — Usage examples

## Development

### Building and Testing

```bash
# Build all components
cargo build --release

# Build just the MCP server
cargo build -p rosbag_mcp_server --release

# Run tests
cargo test

# Linting
cargo clippy

# Example with debug logging
RUST_LOG=debug cargo run -- --bag data/race_1.bag --metadata

# Test MCP server
cargo run -p rosbag_mcp_server -- toolbox
```

### Architecture

Two-pass processing:

1. **Connection Discovery**: Parses message type definitions and builds dependency trees
2. **Selective Processing**: Processes messages matching registered filters

Benefits:
- Metadata extraction without parsing overhead
- Efficient processing of large bag files
- Accurate statistics reflecting actual work performed

## MCP Server

The project includes a **Model Context Protocol (MCP) server** that provides programmatic access to ROS bag analysis through AI assistants and LLMs. The MCP server exposes the same functionality as the CLI through a standardized protocol.

### Quick Start

```bash
# Build and run the MCP server
cargo build -p rosbag_mcp_server
cargo run -p rosbag_mcp_server

# Check available tools
cargo run -p rosbag_mcp_server -- toolbox
```

### MCP Features

#### Tools: `info`, `messages`, `images`
Three specialized tools for ROS bag analysis:
- **`info`**: Analyze bag structure to understand available data (topics, message types, counts)
- **`messages`**: Extract message content with filtering by type/topic and pagination
- **`images`**: Extract image data as base64-encoded PNG for visual analysis
- **Flexible filtering**: Filter by message types or topic names with pagination support
- **Structured output**: All data returned as JSON for easy processing

#### Dynamic Resources
Access bag file metadata through resource URIs:
```
rosbag://{bag_path}/topics        - List all topics in bag file
rosbag://{bag_path}/message_types - List all message types
rosbag://{bag_path}/metadata      - Complete metadata with statistics
```

Example resource URIs:
- `rosbag://data/race_1.bag/topics`
- `rosbag:///absolute/path/recording.bag/message_types`
- `rosbag://relative/path/bag.bag/metadata`

#### Smart Prompts
Context-aware prompts for common workflows:
- **`inspect_bag_metadata`**: Generate metadata inspection commands
- **`extract_sensor_data`**: Smart sensor type detection (IMU, camera, LiDAR, etc.)
- **`temporal_analysis`**: Time-windowed analysis commands
- **`topic_filtering`**: Topic-specific data extraction

### Usage Examples

#### Basic Discovery
Use the `info` tool with just a bag path to understand what data is available.

#### Sensor Data Extraction  
Use the `messages` tool with specific message types (e.g., sensor_msgs/Imu) to extract sensor data with pagination support.

#### Multi-sensor Analysis
Combine multiple message types (IMU + odometry) with the `messages` tool for sensor fusion analysis.

#### Topic-Based Analysis
Filter by specific topic names (e.g., /cmd_vel, /odom) when you need data from particular sensors.

#### Image Extraction
Use the `images` tool to extract visual data as base64 PNG images for analysis.

### Integration

The MCP server can be integrated with:
- **Claude Desktop**: Add to MCP configuration for direct ROS bag analysis
- **Custom applications**: Use MCP client libraries for programmatic access
- **AI workflows**: Embed in LLM-powered robotics analysis pipelines
- **Research tools**: Integrate with data analysis and visualization platforms

### Architecture

- **Protocol compliance**: Implements MCP specification with tools, resources, and prompts
- **Dynamic processing**: Real-time bag file analysis without pre-indexing
- **Resource templates**: Parameterized URIs for flexible bag file access
- **Error handling**: Proper MCP error responses for missing files and invalid requests
- **Performance**: Streaming processing with configurable output limits

The MCP server makes ROS bag analysis accessible to AI assistants, enabling natural language queries like "Show me the IMU data from the first 10 seconds" or "What topics are available in this bag file?"

## License

[MIT](LICENSE)