use crate::{Result, RosbagError, Value};
use std::collections::HashMap;

/// Format a Value as readable markdown
pub fn format_value_as_markdown(value: &Value, indent: usize) -> String {
    let prefix = "  ".repeat(indent);

    match value {
        Value::Bool(b) => format!("**{}**", if *b { "✓" } else { "✗" }),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F32(n) => format!("{:.3}", n),
        Value::F64(n) => format!("{:.3}", n),
        Value::String(s) => format!("\"{}\"", s),
        Value::Time(t) => format!("⏰ {}.{:09}s", t.sec, t.nsec),
        Value::Duration(d) => format!("⏱️ {}.{:09}s", d.sec, d.nsec),
        Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else if arr.len() <= 3 {
                // Short arrays on one line
                let items: Vec<String> =
                    arr.iter().map(|v| format_value_as_markdown(v, 0)).collect();
                format!("[{}]", items.join(", "))
            } else {
                // Long arrays as list
                format!("📊 [{} items]", arr.len())
            }
        }
        Value::Message(map) => {
            if map.is_empty() {
                "{}".to_string()
            } else {
                let mut result = String::new();
                for (key, val) in map {
                    result.push_str(&format!(
                        "\n{}- **{}**: {}",
                        prefix,
                        key,
                        format_value_as_markdown(val, indent + 1)
                    ));
                }
                result
            }
        }
    }
}

/// Trait for types that can be extracted from a Value
pub trait FromValue: Sized {
    /// Extract the value from a Value
    fn from_value(value: &Value) -> Option<Self>;

    /// Field name used for error messages
    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Checked
    fn from_value_checked(field: &str, value: &Value) -> Result<Self> {
        Self::from_value(value).ok_or_else(|| {
            RosbagError::MessageParsingError(format!(
                "Field '{}' is not a {}",
                field,
                Self::type_name()
            ))
        })
    }
}

// Implement FromValue for all the supported primitive types
impl FromValue for bool {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_bool()
    }

    fn type_name() -> &'static str {
        "bool"
    }
}

impl FromValue for i8 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_i8()
    }

    fn type_name() -> &'static str {
        "i8"
    }
}

impl FromValue for i16 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_i16()
    }

    fn type_name() -> &'static str {
        "i16"
    }
}

impl FromValue for i32 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_i32()
    }

    fn type_name() -> &'static str {
        "i32"
    }
}

impl FromValue for i64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_i64()
    }

    fn type_name() -> &'static str {
        "i64"
    }
}

impl FromValue for u8 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_u8()
    }

    fn type_name() -> &'static str {
        "u8"
    }
}

impl FromValue for u16 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_u16()
    }

    fn type_name() -> &'static str {
        "u16"
    }
}

impl FromValue for u32 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_u32()
    }

    fn type_name() -> &'static str {
        "u32"
    }
}

impl FromValue for u64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_u64()
    }

    fn type_name() -> &'static str {
        "u64"
    }
}

impl FromValue for f32 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_f32()
    }

    fn type_name() -> &'static str {
        "f32"
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_f64()
    }

    fn type_name() -> &'static str {
        "f64"
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_str().map(String::from)
    }

    fn type_name() -> &'static str {
        "string"
    }
}

pub trait ValueExt {
    /// Converts as_map() to a Result type with appropriate error
    fn as_map_result(&self) -> Result<&HashMap<String, Value>>;

    /// Gets a field from a map and applies the extractor function, returning a Result
    fn get_field<T, F>(&self, field: &str, extractor: F) -> Result<T>
    where
        F: FnOnce(&Value) -> Option<T>;

    /// Generic method to get a field value with type conversion
    fn get<T: FromValue>(&self, field: &str) -> Result<T>;

    /// Gets an array field and returns it as a slice
    fn get_array(&self, field: &str) -> Result<&[Value]>;

    /// Gets an array field and returns it as a slice, returns empty slice if field is missing
    fn get_array_optional(&self, field: &str) -> Result<&[Value]>;

    /// Gets a field as a Vec<u8>, useful for binary data
    fn get_u8_array(&self, field: &str) -> Result<Vec<u8>>;

    /// Navigates a nested structure with a path of field names
    /// For example: ["pose", "position", "x"] would access data.pose.position.x
    fn get_nested_value(&self, path: &[&str]) -> Result<&Value>;

    /// Generic method to get a nested value with type conversion
    fn get_nested<T: FromValue>(&self, path: &[&str]) -> Result<T>;
}

impl ValueExt for Value {
    fn as_map_result(&self) -> Result<&HashMap<String, Value>> {
        self.as_map()
            .ok_or_else(|| RosbagError::MessageParsingError("Not a map".to_string()))
    }

    fn get_field<T, F>(&self, field: &str, extractor: F) -> Result<T>
    where
        F: FnOnce(&Value) -> Option<T>,
    {
        let map = self.as_map_result()?;
        let value = map.get(field).ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Field '{}' not found", field))
        })?;

        extractor(value).ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Could not extract field '{}'", field))
        })
    }

    fn get<T: FromValue>(&self, field: &str) -> Result<T> {
        let map = self.as_map_result()?;
        let value = map.get(field).ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Field '{}' not found", field))
        })?;

        T::from_value(value).ok_or_else(|| {
            RosbagError::MessageParsingError(format!(
                "Field '{}' is not a {}",
                field,
                T::type_name()
            ))
        })
    }

    fn get_array(&self, field: &str) -> Result<&[Value]> {
        let map = self.as_map_result()?;
        let value = map.get(field).ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Field '{}' not found", field))
        })?;

        value.as_slice().ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Field '{}' is not an array", field))
        })
    }

    fn get_array_optional(&self, field: &str) -> Result<&[Value]> {
        let map = self.as_map_result()?;
        if let Some(value) = map.get(field) {
            value.as_slice().ok_or_else(|| {
                RosbagError::MessageParsingError(format!("Field '{}' is not an array", field))
            })
        } else {
            // Return empty slice if field is missing
            Ok(&[])
        }
    }

    fn get_u8_array(&self, field: &str) -> Result<Vec<u8>> {
        let map = self.as_map_result()?;
        let value = map.get(field).ok_or_else(|| {
            RosbagError::MessageParsingError(format!("Field '{}' not found", field))
        })?;

        if let Value::Array(values) = value {
            // Try to convert all elements at once using collect with Result
            values
                .iter()
                .map(|v| {
                    v.as_u8().ok_or_else(|| {
                        RosbagError::MessageParsingError(format!(
                            "Element in '{}' is not a u8 value",
                            field
                        ))
                    })
                })
                .collect::<Result<Vec<u8>>>()
        } else {
            Err(RosbagError::MessageParsingError(format!(
                "Field '{}' is not an array",
                field
            )))
        }
    }

    fn get_nested_value(&self, path: &[&str]) -> Result<&Value> {
        let mut current = self;

        for &field in path {
            let map = current.as_map().ok_or_else(|| {
                RosbagError::MessageParsingError(format!("Field '{}' is not a map", field))
            })?;

            current = map.get(field).ok_or_else(|| {
                RosbagError::MessageParsingError(format!("Field '{}' not found", field))
            })?;
        }

        Ok(current)
    }

    fn get_nested<T: FromValue>(&self, path: &[&str]) -> Result<T> {
        let value = self.get_nested_value(path)?;

        T::from_value(value).ok_or_else(|| {
            RosbagError::MessageParsingError(format!(
                "Nested path {:?} is not a {}",
                path,
                T::type_name()
            ))
        })
    }
}
