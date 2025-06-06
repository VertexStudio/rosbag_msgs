use rosbag_msgs::{MessageLog, Value, ValueExt};

/// Main image extraction function that handles different message formats
pub fn extract_image_from_message(msg: &MessageLog) -> Result<Vec<u8>, String> {
    let data = &msg.data;

    // Strategy 1: Try standard sensor_msgs/Image format
    if let Ok(result) = try_extract_standard_image(data) {
        return Ok(result);
    }

    // Strategy 2: Try compressed image format (look for format + data fields)
    if let Ok(result) = try_extract_compressed_image(data) {
        return Ok(result);
    }

    // Strategy 3: Search recursively for any image-like data structures
    if let Ok(result) = try_extract_nested_image(data) {
        return Ok(result);
    }

    Err(format!("No image data found in message of type: {}", msg.msg_path))
}

/// Try to extract standard sensor_msgs/Image
fn try_extract_standard_image(data: &Value) -> Result<Vec<u8>, String> {
    let pixels = data.get_u8_array("data").map_err(|_| "No 'data' field")?;
    let width = data.get::<u32>("width").map_err(|_| "No 'width' field")? as usize;
    let height = data.get::<u32>("height").map_err(|_| "No 'height' field")? as usize;
    let encoding = data.get::<String>("encoding").map_err(|_| "No 'encoding' field")?;

    create_png_from_raw_image(&pixels, width, height, &encoding)
}

/// Try to extract compressed image (format + data fields)
fn try_extract_compressed_image(data: &Value) -> Result<Vec<u8>, String> {
    let format = data.get::<String>("format").map_err(|_| "No 'format' field")?;
    let image_data = data.get_u8_array("data").map_err(|_| "No 'data' field")?;

    // If it's already a compressed format, try to decode and re-encode as PNG
    if format.to_lowercase().contains("jpeg") || format.to_lowercase().contains("jpg") {
        convert_compressed_to_png(&image_data, "jpeg")
    } else if format.to_lowercase().contains("png") {
        // Already PNG, return as-is
        Ok(image_data)
    } else {
        Err(format!("Unsupported compressed format: {}", format))
    }
}

/// Search recursively for nested image data
fn try_extract_nested_image(data: &Value) -> Result<Vec<u8>, String> {
    // Look for common image field names in nested structures
    let image_field_names = ["image_chip", "image", "compressed_image", "visual_metadata"];
    
    for field_name in &image_field_names {
        if let Ok(nested_data) = data.get_nested_value(&[field_name]) {
            // Handle arrays of images
            if let Value::Array(array) = nested_data {
                if !array.is_empty() {
                    // Try to extract from first image in array
                    if let Ok(result) = try_extract_compressed_image(&array[0]) {
                        return Ok(result);
                    }
                    if let Ok(result) = try_extract_standard_image(&array[0]) {
                        return Ok(result);
                    }
                }
            } else {
                // Single nested image
                if let Ok(result) = try_extract_compressed_image(nested_data) {
                    return Ok(result);
                }
                if let Ok(result) = try_extract_standard_image(nested_data) {
                    return Ok(result);
                }
            }
        }
    }

    Err("No nested image data found".to_string())
}

/// Convert raw image data to PNG
fn create_png_from_raw_image(data: &[u8], width: usize, height: usize, encoding: &str) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Luma, Rgb, ImageFormat};
    use std::io::Cursor;

    match encoding {
        "mono8" => {
            let img_buffer = ImageBuffer::<Luma<u8>, _>::from_raw(width as u32, height as u32, data.to_vec())
                .ok_or("Failed to create mono8 image buffer")?;

            let mut png_data = Vec::new();
            let mut cursor = Cursor::new(&mut png_data);
            
            img_buffer.write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;

            Ok(png_data)
        }
        "rgb8" => {
            let img_buffer = ImageBuffer::<Rgb<u8>, _>::from_raw(width as u32, height as u32, data.to_vec())
                .ok_or("Failed to create rgb8 image buffer")?;

            let mut png_data = Vec::new();
            let mut cursor = Cursor::new(&mut png_data);
            
            img_buffer.write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;

            Ok(png_data)
        }
        "bgr8" => {
            // Convert BGR to RGB first
            let mut rgb_data = vec![0u8; data.len()];
            for i in 0..(height * width) {
                let base = i * 3;
                if base + 2 < data.len() {
                    rgb_data[base] = data[base + 2];     // R <- B
                    rgb_data[base + 1] = data[base + 1]; // G <- G
                    rgb_data[base + 2] = data[base];     // B <- R
                }
            }

            let img_buffer = ImageBuffer::<Rgb<u8>, _>::from_raw(width as u32, height as u32, rgb_data)
                .ok_or("Failed to create bgr8 image buffer")?;

            let mut png_data = Vec::new();
            let mut cursor = Cursor::new(&mut png_data);
            
            img_buffer.write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;

            Ok(png_data)
        }
        _ => Err(format!("Unsupported encoding: {}", encoding))
    }
}

/// Convert compressed image formats to PNG
fn convert_compressed_to_png(data: &[u8], format: &str) -> Result<Vec<u8>, String> {
    use image::ImageFormat;
    use std::io::Cursor;

    let input_format = match format {
        "jpeg" | "jpg" => ImageFormat::Jpeg,
        "png" => return Ok(data.to_vec()), // Already PNG
        _ => return Err(format!("Unsupported format: {}", format))
    };

    // Decode the compressed image
    let img = image::load(Cursor::new(data), input_format)
        .map_err(|e| format!("Failed to decode {}: {}", format, e))?;

    // Re-encode as PNG
    let mut png_data = Vec::new();
    let mut cursor = Cursor::new(&mut png_data);
    img.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    Ok(png_data)
}