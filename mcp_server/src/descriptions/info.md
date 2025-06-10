# Info Tool

Analyze ROS bag file structure to understand what data is available for processing.

## What it does
- Scans bag file to discover all topics and message types
- Counts total messages by type and topic
- Reports bag timing information (duration, start/end times)
- Optionally shows detailed message field structures

## Parameters
- **bag**: Path to the ROS bag file to analyze
- **definitions**: Set to true to include detailed message field definitions (default: false)

## When to use
- First step when working with an unknown bag file
- To understand what sensors/data streams are available
- To get message counts before setting up filtering or pagination
- To see message structure before extracting specific data

## Examples
- Basic overview: Use just the bag path to get topic names, message types, and counts
- Detailed analysis: Add definitions=true to see complete message field structures for data extraction planning