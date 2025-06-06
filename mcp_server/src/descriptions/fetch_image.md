Extract and return image data from ROS bag files as base64-encoded images for visual analysis and inspection.

## Purpose
Retrieves image data from any ROS message type that contains image information and returns them as base64-encoded images that can be displayed by AI assistants and tools. Uses best-effort parsing to detect and extract image data from various message formats including standard sensor_msgs/Image, compressed formats, and custom image types.

## Parameter Usage

### Required Parameters
- `bag`: Path to the ROS bag file containing image data
- `topic`: Specific topic containing image data (e.g., "/camera/image_raw", "/agent3/Comms/recv_sim_comms_visual_detections")

### Optional Parameters  
- `index`: Which image to extract by position (defaults to 0 for first image)
- `timestamp`: Extract image closest to this timestamp in seconds from bag start
- `image_path`: Array specifying the nested path to image data within the message (e.g., ["visual_metadata", "0", "image_chip", "0"])

## Image Processing
- Automatically detects image data within any message structure
- Supports uncompressed formats (mono8, rgb8, bgr8) and compressed formats (JPEG, PNG)
- Handles nested image data (e.g., image_chip arrays, visual_metadata)
- Converts BGR to RGB for proper color display when format is detected
- Best-effort extraction from custom message types
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

### Extract Nested Image with Specific Path
```
bag="data/recording.bag", topic="/agent3/Comms/recv_sim_comms_visual_detections", image_path=["visual_metadata", "0", "image_chip", "0"]
```
Returns: First image from the nested image_chip array within visual_metadata

## Output Format
Returns base64-encoded PNG image data with MIME type "image/png". The image can be directly displayed by AI assistants and visualization tools that support base64 image content.

## Common Use Cases
- Visual inspection of camera data from robotics recordings
- Extracting reference frames for analysis or documentation
- Sampling images at specific timestamps during events
- Debugging camera calibration and image quality issues