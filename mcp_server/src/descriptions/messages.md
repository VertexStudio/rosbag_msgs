# Messages Tool

Extract message data from ROS bag files with filtering and pagination capabilities.

## Parameter Usage Patterns

### Message Type Extraction: `messages` parameter
Extract specific sensor data by message type:
- `messages=["sensor_msgs/Imu"]` → IMU orientation, angular velocity, linear acceleration
- `messages=["nav_msgs/Odometry"]` → Robot pose estimates and velocity
- `messages=["sensor_msgs/LaserScan"]` → LiDAR range data
- `messages=["sensor_msgs/Image"]` → Camera image data
- `messages=["geometry_msgs/Twist"]` → Velocity commands
- Multiple types: `messages=["sensor_msgs/Imu", "nav_msgs/Odometry"]`

### Topic Filtering: `topics` parameter
Focus on specific data streams:
- `topics=["/camera/imu"]` → All messages from IMU topic
- `topics=["/cmd_vel"]` → Robot velocity commands
- `topics=["/scan"]` → Laser scan data
- Multiple topics: `topics=["/camera/imu", "/odom"]`

### Pagination: `offset` and `limit` parameters
For processing large datasets in chunks:
- `offset=0, limit=10` → First 10 messages (page 1)
- `offset=10, limit=10` → Next 10 messages (page 2)
- `offset=50, limit=25` → Messages 51-75
- Used with `messages` or `topics` for systematic pagination
- Defaults to 1 message if `limit` is not specified

### Large Output Handling
Responses exceeding 20,000 characters are automatically stored to temporary files:
- Returns truncated preview with file path for full results
- Use file reading tools to access complete output
- Applies to message data responses

## Effective Parameter Combinations

### Sensor Data Analysis
```
bag="data/recording.bag", messages=["sensor_msgs/Imu"]
```
Returns: 1 IMU reading (default) with orientation, angular velocity, acceleration

### Multi-sensor Fusion Data
```
bag="data/recording.bag", messages=["sensor_msgs/Imu", "nav_msgs/Odometry"], limit=10
```
Returns: 10 messages of IMU and odometry data

### Topic-specific Investigation
```
bag="data/recording.bag", topics=["/camera/imu"], limit=5
```
Returns: First 5 messages from specific topic

### Specific Data Range
```
bag="data/recording.bag", topics=["/cmd_vel", "/odom"], offset=100, limit=20
```
Returns: Messages 100-119 from command and odometry topics

### Performance Sampling
```
bag="data/recording.bag", messages=["sensor_msgs/Image"], limit=5
```
Returns: 5 sample images without overwhelming output

## Common Workflows

1. **Discovery**: Use the `bag_info` tool first to see available topics and types
2. **Selection**: Use discovered types/topics with `messages` or `topics` parameters
3. **Sampling**: Add `limit` to control output size (defaults to 1 message)
4. **Pagination**: Use `offset`/`limit` for systematic processing of large datasets

## Output Format
All data returned as JSON with message timestamps, topic names, and parsed message content.

### Pagination Information
When using offset/limit parameters, responses include pagination metadata:
- **Offset**: Starting position used for this request
- **Limit**: Maximum messages requested
- **Returned**: Actual number of messages returned
- **Total**: Total number of messages available that match the filters

Example pagination response:
```
Pagination:
  Offset: 100 | Limit: 10 | Returned: 10 | Total: 5679
```

With this information you can calculate:
- Has more data: `offset + returned < total`
- Next offset: `offset + returned`
- Progress: `(offset + returned) / total`