# Info Tool

Inspects ROS bag file structure and metadata, equivalent to the CLI `info` command and similar to `rosbag info`.

## Parameters
- **bag**: Path to the ROS bag file (.bag extension)
- **definitions**: Optional boolean to include detailed message type definitions

## Output
- Basic info: topic names, message types, counts, and timing statistics
- With definitions=true: Complete message type definitions and field structures

## Usage Examples
```
# Basic bag info (topic summary)
info(bag="data/race_1.bag")

# Detailed info with message definitions
info(bag="data/race_1.bag", definitions=true)
```

This tool provides an overview of bag contents without processing actual message data, making it ideal for quick inspection and understanding of recorded data structure.