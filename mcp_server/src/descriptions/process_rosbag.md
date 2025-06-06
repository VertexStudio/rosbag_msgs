Parse and extract data from ROS bag files (.bag format) using parameter combinations for metadata inspection, message filtering, and temporal analysis.

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

### Temporal Windows: `start` and `duration`
Analyze specific time periods:
- `duration=5.0` → First 5 seconds of recording
- `start=10.0, duration=5.0` → 5-second window starting at 10s
- `start=15.5` → From 15.5 seconds to end
- `start=2.5, duration=0.5` → 500ms window for precise event analysis

### Output Control: `max` parameter
Limit data volume:
- `max=10` → Maximum 10 messages per topic/type
- `max=100` → Sample 100 messages for large datasets
- Combined with `metadata=true` shows total vs. processed counts

## Effective Parameter Combinations

### Initial Exploration
```
bag="data/recording.bag", metadata=true
```
Returns: Topic structure, message types, counts, bag duration

### Sensor Data Analysis
```
bag="data/recording.bag", messages=["sensor_msgs/Imu"], max=50
```
Returns: 50 IMU readings with orientation, angular velocity, acceleration

### Multi-sensor Fusion Data
```
bag="data/recording.bag", messages=["sensor_msgs/Imu", "nav_msgs/Odometry"], start=10.0, duration=5.0
```
Returns: IMU and odometry data from 10-15 second window

### Topic-specific Investigation
```
bag="data/recording.bag", topics=["/camera/imu"], metadata=true, max=20
```
Returns: Metadata + first 20 messages from specific topic

### Event Analysis
```
bag="data/recording.bag", topics=["/cmd_vel", "/odom"], start=25.5, duration=2.0
```
Returns: Commands and odometry during 2-second event starting at 25.5s

### Performance Sampling
```
bag="data/recording.bag", messages=["sensor_msgs/Image"], max=5
```
Returns: 5 sample images without overwhelming output

## Common Workflows

1. **Discovery**: Start with `metadata=true` to see available topics and types
2. **Selection**: Use discovered types/topics with `messages` or `topics` parameters
3. **Refinement**: Add `start`/`duration` for specific time windows
4. **Sampling**: Use `max` to limit output for large datasets
5. **Validation**: Combine `metadata=true` with filters to verify selection

## Output Format
All data returned as JSON with message timestamps, topic names, and parsed message content. Metadata includes ASCII tree structures showing message field hierarchies and data types.