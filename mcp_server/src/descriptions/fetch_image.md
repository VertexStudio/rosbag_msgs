Extract and return image data from ROS bag files as base64-encoded images for visual analysis and inspection.

## Purpose
Retrieves image messages from sensor_msgs/Image topics in ROS bag files and returns them as base64-encoded images that can be displayed by AI assistants and tools. Supports common ROS image encodings including mono8, rgb8, and bgr8.

## Parameter Usage

### Required Parameters
- `bag`: Path to the ROS bag file containing image data
- `topic`: Specific image topic to extract from (e.g., "/camera/image_raw", "/camera/fisheye2/image_raw")

### Optional Parameters  
- `index`: Which image to extract by position (defaults to 0 for first image)
- `timestamp`: Extract image closest to this timestamp in seconds from bag start

## Image Processing
- Automatically detects image encoding from ROS message metadata
- Supports mono8 (grayscale), rgb8, and bgr8 color formats
- Converts BGR to RGB for proper color display
- Validates image dimensions and data integrity
- Returns images in PNG format with base64 encoding for web compatibility

## Usage Examples

### Extract First Image
```
bag="data/recording.bag", topic="/camera/image_raw"
```
Returns: First image from the camera topic as base64 PNG

### Extract Specific Image by Index
```
bag="data/recording.bag", topic="/camera/fisheye2/image_raw", index=10
```
Returns: 11th image (0-indexed) from the fisheye camera

### Extract Image by Timestamp
```
bag="data/recording.bag", topic="/camera/image_raw", timestamp=15.5
```
Returns: Image closest to 15.5 seconds from bag start

## Output Format
Returns base64-encoded PNG image data with MIME type "image/png". The image can be directly displayed by AI assistants and visualization tools that support base64 image content.

## Common Use Cases
- Visual inspection of camera data from robotics recordings
- Extracting reference frames for analysis or documentation
- Sampling images at specific timestamps during events
- Debugging camera calibration and image quality issues