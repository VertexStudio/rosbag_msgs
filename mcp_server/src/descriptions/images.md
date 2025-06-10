# Images Tool

Extract image data from ROS bag files and return as base64-encoded images for visual analysis.

## What it does
- Extracts image data from any topic containing visual information
- Converts images to PNG format with base64 encoding for display
- Supports standard formats (sensor_msgs/Image) and custom message types
- Handles both uncompressed (rgb8, bgr8, mono8) and compressed (JPEG, PNG) formats
- Can navigate nested message structures to find embedded images

## Parameters
- **bag**: Path to the ROS bag file containing image data
- **topic**: Topic name with image messages (e.g., /camera/image_raw)
- **offset**: Which image to extract (0=first, 1=second, etc.) - default: 0
- **image_path**: Array path to nested image data (for complex message structures)

## When to use
- Visual inspection of camera recordings
- Extracting sample frames for analysis
- Getting reference images from specific time points
- Analyzing image quality or camera calibration issues

## Common use cases
- First camera image: Use default offset=0 to get the first frame
- Specific frame by position: Set offset=50 to get the 51st image
- Nested image extraction: Use image_path for complex structures like visual_metadata arrays
- Sample multiple frames: Call with different offsets (0, 100, 200) to get a sequence

## Output format
Returns base64-encoded PNG image that can be directly displayed or processed by AI vision tools.