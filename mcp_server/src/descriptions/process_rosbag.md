Parse and extract data from ROS bag files (.bag format) using parameter combinations for metadata inspection, message filtering, and pagination.

## Parameter Usage Patterns

### Discovery: `metadata=true` only
Start with metadata-only analysis to understand bag contents:
- Returns topic list with message types (e.g., `/camera/imu` → `sensor_msgs/Imu`)
- Shows message structure trees with field types and relationships
- Provides message counts by type and topic
- Fast scanning without parsing message content
- Essential first step before filtering

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
- Applies to both message data and metadata responses

## Effective Parameter Combinations

### Initial Exploration
```
bag="data/recording.bag", metadata=true
```
Returns: Topic structure, message types, counts, bag duration

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
bag="data/recording.bag", topics=["/camera/imu"], metadata=true
```
Returns: Metadata + first message (default) from specific topic

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

### Pagination Example
```
bag="data/recording.bag", topics=["/camera/image_raw"], offset=0, limit=10
```
Returns: First 10 images from camera topic

```
bag="data/recording.bag", topics=["/camera/image_raw"], offset=10, limit=10  
```
Returns: Next 10 images (messages 11-20)

## Common Workflows

1. **Discovery**: Start with `metadata=true` to see available topics and types
2. **Selection**: Use discovered types/topics with `messages` or `topics` parameters
3. **Sampling**: Add `limit` to control output size (defaults to 1 message)
4. **Pagination**: Use `offset`/`limit` for systematic processing of large datasets
5. **Validation**: Combine `metadata=true` with filters to verify selection

## Output Format
All data returned as JSON with message timestamps, topic names, and parsed message content. Metadata includes ASCII tree structures showing message field hierarchies and data types.

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