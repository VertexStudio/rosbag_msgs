# Messages Tool

Extract and analyze message data from ROS bag files with precise filtering and pagination.

## What it does
- Extracts message content from bag files as structured data
- Filters by message type (e.g., sensor_msgs/Imu) or topic name (e.g., /camera/imu)
- Provides pagination for processing large datasets efficiently
- Returns parsed message data with timestamps and metadata

## Parameters
- **bag**: Path to the ROS bag file containing the data
- **messages**: Array of message types to extract (e.g., sensor_msgs/Imu, nav_msgs/Odometry)
- **topics**: Array of topic names to extract (e.g., /camera/imu, /odom)
- **offset**: Skip this many messages before starting (default: 0)
- **limit**: Maximum messages to return (default: 1)

## Filtering strategies

### By message type
Use when you want specific sensor data regardless of topic names:
- IMU data: sensor_msgs/Imu
- Camera images: sensor_msgs/Image
- Robot odometry: nav_msgs/Odometry
- LiDAR scans: sensor_msgs/LaserScan

### By topic name
Use when you want data from specific sensors or publishers:
- Specific IMU: /camera/imu
- Robot commands: /cmd_vel
- Multiple sensors: /camera/imu, /lidar/scan

### Pagination for large datasets
- First batch: offset=0, limit=10
- Next batch: offset=10, limit=10
- Skip to middle: offset=1000, limit=50

## Common use cases
- Single IMU reading: Just specify message type for one sample
- Multiple sensor data: Combine IMU and odometry for sensor fusion analysis
- Specific topic data: Get messages from particular sensor topics
- Paginated processing: Process large datasets in manageable chunks
- Sampling datasets: Get first few images without processing thousands

## Best practices
- Start with limit=1 to see message structure before processing more data
- Use pagination (offset/limit) for datasets with thousands of messages
- Combine multiple message types for multi-sensor analysis
- Filter by topics when you need data from specific sensors