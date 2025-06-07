# Bag Info Tool

Inspects ROS bag file structure and metadata, similar to the `rosbag info` command.

## Parameters
- **bag**: Path to the ROS bag file (.bag extension)
- **definitions**: Optional boolean to include detailed message type definitions

## Output
- Basic info: topic names, message types, counts, and timing statistics
- With definitions=true: Complete message type definitions and field structures

## Usage Examples
```
# Basic bag info (topic summary)
bag_info(bag="data/race_1.bag")

# Detailed info with message definitions
bag_info(bag="data/race_1.bag", definitions=true)
```

This tool provides an overview of bag contents without processing actual message data, making it ideal for quick inspection and understanding of recorded data structure.