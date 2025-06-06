use byteorder::{LittleEndian, ReadBytesExt};
use log::{debug, error, trace, warn};
use ros_message::{DataType, Duration, FieldCase, Msg, Time};
use rosbag::{ChunkRecord, IndexRecord, MessageRecord, RosBag};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::sync::mpsc;

pub use image_utils::extract_image_from_message;
pub use ros_message::{MessagePath, Value};
pub use value::{FromValue, ValueExt};

mod image_utils;
mod value;

// Maximum output size in characters before storing to disk
pub const MAX_OUTPUT_SIZE: usize = 20_000;

#[derive(Error, Debug)]
pub enum RosbagError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Rosbag error: {0}")]
    RosbagError(#[from] rosbag::Error),

    #[error("ROS Message Definition error: {0}")]
    RosMessageError(#[from] ros_message::Error),

    #[error("Message parsing error: {0}")]
    MessageParsingError(String),

    #[error("Missing message definition for type: {0}")]
    MissingDefinitionError(String),
}

// Use the custom Result type
pub type Result<T> = std::result::Result<T, RosbagError>;

/// Store large output to a temporary file and return the file path
pub fn store_large_output_to_file(
    content: &str,
    prefix: &str,
) -> std::result::Result<String, std::io::Error> {
    use std::env;
    use std::fs::File;

    let temp_dir = env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let filename = format!("{}_{}.txt", prefix, timestamp);
    let file_path = temp_dir.join(filename);

    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Handle potentially large output by either returning it directly or truncating with file info
pub fn handle_large_output(content: String, prefix: &str) -> String {
    if content.len() > MAX_OUTPUT_SIZE {
        match store_large_output_to_file(&content, prefix) {
            Ok(file_path) => {
                let truncated_content = if content.len() > 1000 {
                    format!(
                        "{}\n\n[TRUNCATED - Full output stored at: {}]\n\nOutput size: {} characters\nUse file reading tools to access the complete result.",
                        &content[..1000],
                        file_path,
                        content.len()
                    )
                } else {
                    format!("{}\n\n[Full output stored at: {}]", content, file_path)
                };
                truncated_content
            }
            Err(e) => {
                // Fallback to truncated inline content if file creation fails
                format!(
                    "{}\n\n[TRUNCATED - Failed to store to file: {}]\nOriginal size: {} characters",
                    &content[..MAX_OUTPUT_SIZE.min(content.len())],
                    e,
                    content.len()
                )
            }
        }
    } else {
        content
    }
}

pub struct Connection {
    pub topic: String,
    pub message_path: MessagePath,
}

// Main struct to handle bag processing
pub struct BagProcessor {
    bag_path: PathBuf,
    connections: HashMap<u32, Connection>,
    definitions: HashMap<MessagePath, Vec<Msg>>,
    message_registry: HashMap<MessagePath, mpsc::Sender<MessageLog>>,
    topic_registry: HashMap<String, mpsc::Sender<MessageLog>>,
    message_counts: HashMap<MessagePath, usize>,
    topic_counts: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct MessageLog {
    pub time: u64,
    pub topic: String,
    pub msg_path: MessagePath,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub id: u32,
    pub topic: String,
    pub message_type: MessagePath,
    pub message_definition: String,
    pub dependencies: Vec<Msg>,
}

impl ConnectionInfo {
    pub fn format_structure(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("topic: {}\n", self.topic));

        for (i, msg) in self.dependencies.iter().enumerate() {
            if i == 0 {
                output.push_str(&format!("type: {}\n", msg.path()));
                for field in msg.fields() {
                    output.push_str(&format!(
                        "        |      {}: {} {:?}\n",
                        field.name(),
                        field.datatype(),
                        field.case()
                    ));
                }
            } else {
                output.push_str(&format!("        |- {}\n", msg.path()));
                for field in msg.fields() {
                    output.push_str(&format!(
                        "        |      {}: {} {:?}\n",
                        field.name(),
                        field.datatype(),
                        field.case()
                    ));
                }
            }
        }
        output.push_str(&format!("{}\n", ".".repeat(100)));
        output
    }
}

#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub total_messages: usize,
    pub total_processed: Option<usize>,
    pub message_counts: HashMap<MessagePath, usize>,
    pub topic_counts: HashMap<String, usize>,
    pub processing_duration_ms: Option<u64>,
    pub bag_start_time: Option<u64>,
    pub bag_end_time: Option<u64>,
}

impl ProcessingStats {
    pub fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str("Processing completed!\n");
        output.push_str(&format!("Total messages: {}\n", self.total_messages));

        if let Some(processed) = self.total_processed {
            output.push_str(&format!("Total processed: {}\n", processed));
        }

        if let Some(duration_ms) = self.processing_duration_ms {
            output.push_str(&format!("Processing time: {}ms\n", duration_ms));
        }

        if let (Some(start), Some(end)) = (self.bag_start_time, self.bag_end_time) {
            let duration_secs = (end - start) as f64 / 1_000_000_000.0;
            output.push_str(&format!("Bag duration: {:.3}s\n", duration_secs));

            // Convert nanosecond timestamps to seconds.nanoseconds format
            let start_sec = start / 1_000_000_000;
            let start_nsec = start % 1_000_000_000;
            let end_sec = end / 1_000_000_000;
            let end_nsec = end % 1_000_000_000;

            output.push_str(&format!(
                "Bag start time: {}.{:09}\n",
                start_sec, start_nsec
            ));
            output.push_str(&format!("Bag end time: {}.{:09}\n", end_sec, end_nsec));
        }

        output.push_str("Message counts by type:\n");
        for (msg_type, count) in &self.message_counts {
            output.push_str(&format!("  {}: {}\n", msg_type, count));
        }

        output.push_str("Message counts by topic:\n");
        for (topic, count) in &self.topic_counts {
            output.push_str(&format!("  {}: {}\n", topic, count));
        }

        output
    }
}

#[derive(Debug, Clone)]
pub enum MetadataEvent {
    ConnectionDiscovered(ConnectionInfo),
    ProcessingStarted,
    ProcessingCompleted(ProcessingStats),
}

impl BagProcessor {
    // Constructor for BagProcessor
    pub fn new(bag_path: PathBuf) -> Self {
        Self {
            bag_path,
            connections: HashMap::new(),
            definitions: HashMap::new(),
            message_registry: HashMap::new(),
            topic_registry: HashMap::new(),
            message_counts: HashMap::new(),
            topic_counts: HashMap::new(),
        }
    }

    pub fn register_message(
        &mut self,
        msg_path: &str,
        sender: mpsc::Sender<MessageLog>,
    ) -> Result<()> {
        let msg_path = MessagePath::try_from(msg_path)?;
        self.message_registry.insert(msg_path, sender);
        Ok(())
    }

    pub fn register_topic(&mut self, topic: &str, sender: mpsc::Sender<MessageLog>) -> Result<()> {
        self.topic_registry.insert(topic.to_string(), sender);
        Ok(())
    }

    pub async fn process_bag(
        &mut self,
        metadata_sender: Option<mpsc::Sender<MetadataEvent>>,
        offset: Option<usize>,
        limit: Option<usize>,
        start_time_offset: Option<f64>,
        duration: Option<f64>,
    ) -> Result<()> {
        debug!("Starting bag processing for: {}", self.bag_path.display());

        // Send processing started event
        if let Some(ref sender) = metadata_sender {
            let _ = sender.send(MetadataEvent::ProcessingStarted).await;
        }

        let bag = Self::open_bag(&self.bag_path)?;

        // --- Pass 1: Collect Connection Info and Parse Definitions ---
        debug!(
            "Scanning connections and parsing message definitions for: {}",
            self.bag_path.display()
        );
        for index_record_result in bag.index_records() {
            match index_record_result {
                Ok(IndexRecord::Connection(conn)) => {
                    // Attempt to parse the message type string into a MessagePath
                    match MessagePath::try_from(conn.tp) {
                        Ok(message_path) => {
                            debug!(
                                "Found connection: id={}, topic='{}', type='{}'",
                                conn.id, conn.topic, message_path
                            );

                            // Store connection info
                            self.connections.insert(
                                conn.id,
                                Connection {
                                    topic: conn.topic.to_string(),
                                    message_path: message_path.clone(),
                                },
                            );

                            // Store dependencies
                            let dependencies = self.get_dependencies(
                                conn.topic,
                                &message_path,
                                conn.message_definition,
                            )?;
                            self.definitions
                                .insert(message_path.clone(), dependencies.clone());

                            // Send connection discovered event
                            if let Some(ref sender) = metadata_sender {
                                let connection_info = ConnectionInfo {
                                    id: conn.id,
                                    topic: conn.topic.to_string(),
                                    message_type: message_path.clone(),
                                    message_definition: conn.message_definition.to_string(),
                                    dependencies,
                                };
                                let _ = sender
                                    .send(MetadataEvent::ConnectionDiscovered(connection_info))
                                    .await;
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Invalid message type '{}' for connection id {}: {}",
                                conn.tp, conn.id, e
                            );
                            // Skip this connection if the type name is invalid
                            continue;
                        }
                    }
                }
                Ok(_) => { /* Ignore other index record types */ }
                Err(e) => {
                    // Log error reading index record but try to continue
                    error!("Error reading index record: {}", e);
                }
            }
        }
        debug!(
            "Finished scanning connections. Found {} unique message types in: {}",
            self.definitions.len(),
            self.bag_path.display()
        );

        let skip_parsing = self.message_registry.is_empty() && self.topic_registry.is_empty();
        if skip_parsing {
            debug!("No message or topic handlers registered: will count messages but skip parsing");
        }

        // --- Pass 2: Process Messages ---
        debug!("Processing message data for: {}", self.bag_path.display());
        let mut total_messages = 0;
        let mut total_messages_processed = 0;

        // Start processing timer only if we're actually going to process messages
        let processing_start_time = if skip_parsing {
            None
        } else {
            Some(std::time::Instant::now())
        };

        // Track global message processing for offset/limit pagination
        let mut global_message_count = 0;
        let offset_val = offset.unwrap_or(0);
        let limit_val = limit.unwrap_or(5); // Default to 5 messages if no limit specified

        // Track bag start and end times from actual message timestamps
        let mut bag_start_time: Option<u64> = None;
        let mut bag_end_time: Option<u64> = None;

        // Track temporal filtering window
        let mut first_message_time: Option<u64> = None;
        let temporal_filter_active = start_time_offset.is_some() || duration.is_some();

        // Iterate through chunks
        'chunk_loop: for chunk_record_result in bag.chunk_records() {
            let chunk_record = match chunk_record_result {
                Ok(cr) => cr,
                Err(e) => {
                    error!("Error reading chunk record: {}", e);
                    continue; // Skip corrupted chunk records
                }
            };

            if let ChunkRecord::Chunk(chunk) = chunk_record {
                // Iterate through messages within the chunk
                for message_record_result in chunk.messages() {
                    match message_record_result {
                        Ok(MessageRecord::Connection(_conn)) => {}
                        Err(e) => {
                            error!("Error reading message record: {}", e);
                            continue;
                        }
                        Ok(MessageRecord::MessageData(message)) => {
                            total_messages += 1;
                            let connection = self.connections.get(&message.conn_id).ok_or_else(|| {
                                RosbagError::MessageParsingError(format!(
                                    "Internal Error: Connection ID {} not found for message data.",
                                    message.conn_id
                                ))
                            })?;
                            let msg_path = connection.message_path.clone();
                            trace!("Message path: {} {}", message.time, msg_path);

                            // Track bag start and end times from message timestamps
                            bag_start_time = Some(
                                bag_start_time
                                    .map_or(message.time, |start| start.min(message.time)),
                            );
                            bag_end_time = Some(
                                bag_end_time.map_or(message.time, |end| end.max(message.time)),
                            );

                            // Track first message time for temporal filtering
                            if first_message_time.is_none() {
                                first_message_time = Some(message.time);
                            }

                            // Apply temporal filtering if active
                            let mut message_in_time_window = true;
                            if temporal_filter_active {
                                if let Some(first_time) = first_message_time {
                                    let relative_time_secs =
                                        (message.time - first_time) as f64 / 1_000_000_000.0;

                                    // Check start time filter
                                    if let Some(start_offset) = start_time_offset {
                                        if relative_time_secs < start_offset {
                                            message_in_time_window = false;
                                        }
                                    }

                                    // Check duration filter (start + duration)
                                    if let (Some(start_offset), Some(duration_secs)) =
                                        (start_time_offset, duration)
                                    {
                                        let end_time = start_offset + duration_secs;
                                        if relative_time_secs > end_time {
                                            message_in_time_window = false;
                                        }
                                    } else if let (None, Some(duration_secs)) =
                                        (start_time_offset, duration)
                                    {
                                        // Duration without start time means duration from bag start
                                        if relative_time_secs > duration_secs {
                                            message_in_time_window = false;
                                        }
                                    }
                                }
                            }

                            // Update message count for this path (always do this for stats)
                            *self.message_counts.entry(msg_path.clone()).or_insert(0) += 1;

                            // Update topic count (always do this for stats)
                            *self
                                .topic_counts
                                .entry(connection.topic.clone())
                                .or_insert(0) += 1;

                            if skip_parsing {
                                // Skip parsing entirely if no handlers are registered
                                continue;
                            }

                            // Check if this message should be processed (by type, topic, and time window)
                            let message_sender = self.message_registry.get(&msg_path);
                            let topic_sender = self.topic_registry.get(&connection.topic);

                            if (message_sender.is_some() || topic_sender.is_some())
                                && message_in_time_window
                            {
                                let msg_defs = self.definitions.get(&msg_path).ok_or_else(|| {
                                    RosbagError::MissingDefinitionError(format!(
                                        "Internal Error: Definitions for {} not found despite connection.",
                                        msg_path
                                    ))
                                })?;
                                let msg = self.parse_message_data(message.data, msg_defs);
                                if let Ok(msg) = msg {
                                    trace!("Message: {}", msg);
                                    total_messages_processed += 1;

                                    let message_log = MessageLog {
                                        time: message.time,
                                        topic: connection.topic.clone(),
                                        msg_path: msg_path.clone(),
                                        data: msg,
                                    };

                                    // Apply global offset/limit pagination
                                    if global_message_count < offset_val {
                                        global_message_count += 1;
                                        continue; // Skip this message due to offset
                                    }

                                    if global_message_count >= offset_val + limit_val {
                                        trace!(
                                            "Reached limit of {} messages after offset {}",
                                            limit_val, offset_val
                                        );
                                        break 'chunk_loop; // Stop processing - we've hit the limit
                                    }

                                    // Send to message type handler if registered
                                    if let Some(sender) = message_sender {
                                        trace!("--> {} (by type)", msg_path);
                                        if let Err(e) = sender.send(message_log.clone()).await {
                                            warn!("Error sending message to type handler: {}", e);
                                            break 'chunk_loop;
                                        }
                                    }

                                    // Send to topic handler if registered
                                    if let Some(sender) = topic_sender {
                                        trace!("--> {} (by topic)", connection.topic);
                                        if let Err(e) = sender.send(message_log).await {
                                            warn!("Error sending message to topic handler: {}", e);
                                            break 'chunk_loop;
                                        }
                                    }

                                    global_message_count += 1;
                                } else if let Err(e) = msg {
                                    error!("Error parsing message: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!(
            "Total messages in {}: {}",
            self.bag_path.display(),
            total_messages
        );
        if !skip_parsing {
            debug!(
                "Total messages processed in {}: {}",
                self.bag_path.display(),
                total_messages_processed
            );
        }

        // Log message counts per message path
        debug!("Message counts per type:");
        for (msg_path, count) in self.message_counts.iter() {
            debug!("  {}: {}", msg_path, count);
        }

        // Log message counts per topic
        debug!("Message counts per topic:");
        for (topic, count) in self.topic_counts.iter() {
            debug!("  {}: {}", topic, count);
        }

        // Send processing completed event with stats
        if let Some(ref sender) = metadata_sender {
            let processing_duration_ms =
                processing_start_time.map(|start| start.elapsed().as_millis() as u64);
            let stats = ProcessingStats {
                total_messages,
                total_processed: if skip_parsing {
                    None
                } else {
                    Some(total_messages_processed)
                },
                message_counts: self.message_counts.clone(),
                topic_counts: self.topic_counts.clone(),
                processing_duration_ms,
                bag_start_time,
                bag_end_time,
            };
            let _ = sender.send(MetadataEvent::ProcessingCompleted(stats)).await;
        }

        // Clear registries to drop all sender channels and signal handlers to exit
        self.message_registry.clear();
        self.topic_registry.clear();

        Ok(())
    }

    fn get_dependencies(
        &self,
        topic: &str,
        msg_path: &MessagePath,
        msg_def: &str,
    ) -> Result<Vec<Msg>> {
        let mut messages = Vec::new();
        let mut defs = msg_def
            .split(
                "================================================================================",
            )
            .collect::<Vec<&str>>();

        // First source is the original message definition
        let msg_src = defs.remove(0);
        messages.push(Msg::new(msg_path.clone(), msg_src)?);

        // The rest are dependencies
        for def in &defs {
            let def = def.trim();
            if let Some(first_line_end) = def.find('\n') {
                let first_line = def[..first_line_end].to_string();
                let Some(msg_path) = first_line.split("MSG: ").nth(1) else {
                    break;
                };
                let msg_src = def[first_line_end + 1..].to_string();
                let msg_path = MessagePath::try_from(msg_path)?;
                messages.push(Msg::new(msg_path, &msg_src)?);
            }
        }

        // Trace ascii art tree of dependencies (per-message verbosity)
        trace!("topic: {}", topic);
        for (i, msg) in messages.iter().enumerate() {
            if i == 0 {
                trace!("type: {}", msg.path());
                for field in msg.fields() {
                    trace!(
                        "        |      {}: {} {:?}",
                        field.name(),
                        field.datatype(),
                        field.case()
                    );
                }
            } else {
                trace!("        |- {}", msg.path());
                for field in msg.fields() {
                    trace!(
                        "        |      {}: {} {:?}",
                        field.name(),
                        field.datatype(),
                        field.case()
                    );
                }
            }
        }
        trace!("{}", ".".repeat(100));

        Ok(messages)
    }

    // Parses binary message data based on the ros_message::Msg definition
    fn parse_message_data(&self, data: &[u8], msg_defs: &Vec<Msg>) -> Result<Value> {
        if let Some(msg_def) = msg_defs.first() {
            // `msg_defs` contains the primary message definition (at index 0) followed by its dependencies
            // Create a cursor to read from the data buffer
            let mut cursor = Cursor::new(data);
            // Use the recursive helper function starting with the primary message definition
            self.parse_message_data_recursive(&mut cursor, msg_def, msg_defs)
        } else {
            Err(RosbagError::MissingDefinitionError(
                "No message definition found".to_string(),
            ))
        }
    }

    // Recursive helper to parse message data, handling nested types
    fn parse_message_data_recursive<R: Read>(
        &self,
        cursor: &mut R,
        msg_def: &Msg,
        msg_defs: &Vec<Msg>,
    ) -> Result<Value> {
        // Use MessageValue (HashMap) to store fields, as defined in ros_message::Value
        let mut message_map = ros_message::MessageValue::new();

        // Iterate through fields defined in the ros_message::Msg object
        for field in msg_def.fields() {
            // Skip constant fields, they are part of the definition, not the data stream
            if field.is_constant() {
                continue;
            }

            let field_name = field.name();
            let field_datatype = field.datatype();
            let field_case = field.case();

            trace!(
                "Parsing field: '{}', Type: '{}', Case: {:?}",
                field_name, field_datatype, field_case
            );

            // Parse based on the field case (Unit, Vector, Array)
            let parsed_field_value = match field_case {
                FieldCase::Unit => {
                    // Parse a single value (primitive or nested message)
                    self.parse_single_value(cursor, field_datatype, msg_defs)?
                }
                FieldCase::Vector => {
                    // Variable-length array: read length prefix (u32)
                    let array_len = match cursor.read_u32::<LittleEndian>() {
                        Ok(len) => len as usize,
                        Err(e) => {
                            warn!(
                                "Failed to read array length for vector field '{}': {}. Assuming empty.",
                                field_name, e
                            );
                            // Decide how to handle: return error, or assume empty array? Assuming empty for resilience.
                            // return Err(AppError::IoError(e)); // Option: Fail hard
                            0 // Option: Assume empty
                        }
                    };

                    let mut array_values = Vec::with_capacity(array_len);
                    for i in 0..array_len {
                        match self.parse_single_value(cursor, field_datatype, msg_defs) {
                            Ok(value) => array_values.push(value),
                            Err(e) => {
                                error!(
                                    "Failed to parse element {} of vector field '{}': {}. Stopping array parse.",
                                    i, field_name, e
                                );
                                // Stop parsing this array on error, might leave cursor in wrong state
                                break;
                            }
                        }
                    }
                    Value::Array(array_values)
                }
                FieldCase::Array(fixed_len) => {
                    // Fixed-length array
                    let mut array_values = Vec::with_capacity(*fixed_len);
                    for i in 0..*fixed_len {
                        match self.parse_single_value(cursor, field_datatype, msg_defs) {
                            Ok(value) => array_values.push(value),
                            Err(e) => {
                                error!(
                                    "Failed to parse element {} of fixed array field '{}' (len {}): {}. Stopping array parse.",
                                    i, field_name, fixed_len, e
                                );
                                // Stop parsing this array on error
                                break;
                            }
                        }
                    }
                    Value::Array(array_values)
                }
                FieldCase::Const(_) => unreachable!(), // Already handled above
            };

            // Insert the parsed value into the map
            message_map.insert(field_name.to_string(), parsed_field_value);
        }

        // Wrap the HashMap in a Value::Message variant
        Ok(Value::Message(message_map))
    }

    // Parses a single value, which could be primitive or a nested message
    fn parse_single_value<R: Read>(
        &self,
        cursor: &mut R,
        datatype: &DataType,
        msg_defs: &Vec<Msg>,
    ) -> Result<Value> {
        match datatype {
            // Handle primitive types directly
            DataType::Bool => Ok(Value::Bool(cursor.read_u8()? != 0)),
            DataType::I8(_) => Ok(Value::I8(cursor.read_i8()?)), // Handles byte and int8
            DataType::I16 => Ok(Value::I16(cursor.read_i16::<LittleEndian>()?)),
            DataType::I32 => Ok(Value::I32(cursor.read_i32::<LittleEndian>()?)),
            DataType::I64 => Ok(Value::I64(cursor.read_i64::<LittleEndian>()?)),
            DataType::U8(_) => Ok(Value::U8(cursor.read_u8()?)), // Handles char and uint8
            DataType::U16 => Ok(Value::U16(cursor.read_u16::<LittleEndian>()?)),
            DataType::U32 => Ok(Value::U32(cursor.read_u32::<LittleEndian>()?)),
            DataType::U64 => Ok(Value::U64(cursor.read_u64::<LittleEndian>()?)),
            DataType::F32 => Ok(Value::F32(cursor.read_f32::<LittleEndian>()?)),
            DataType::F64 => Ok(Value::F64(cursor.read_f64::<LittleEndian>()?)),
            DataType::String => {
                let len = cursor.read_u32::<LittleEndian>()? as usize;
                let mut buffer = vec![0u8; len];
                cursor.read_exact(&mut buffer)?;
                // Handle potential UTF-8 errors
                String::from_utf8(buffer).map(Value::String).map_err(|e| {
                    RosbagError::MessageParsingError(format!(
                        "Invalid UTF-8 sequence in string: {}",
                        e
                    ))
                })
            }
            DataType::Time => {
                let sec = cursor.read_u32::<LittleEndian>()?;
                let nsec = cursor.read_u32::<LittleEndian>()?;
                Ok(Value::Time(Time { sec, nsec }))
            }
            DataType::Duration => {
                let sec = cursor.read_i32::<LittleEndian>()?;
                let nsec = cursor.read_i32::<LittleEndian>()?;
                Ok(Value::Duration(Duration { sec, nsec }))
            }
            // Handle nested message types (Local or Global)
            DataType::LocalMessage(local_name) => {
                // For local messages, find the matching message definition by local name
                // A local message typically only has the name part without the package
                let msg_def = msg_defs.iter().find(|msg| msg.path().name() == local_name);
                if let Some(nested_def) = msg_def {
                    // Recursively parse the nested message
                    self.parse_message_data_recursive(cursor, nested_def, msg_defs)
                } else {
                    // Definition wasn't found/parsed earlier
                    Err(RosbagError::MissingDefinitionError(local_name.clone()))
                }
            }
            DataType::GlobalMessage(global_path) => {
                // For global messages, we use the global path directly
                let msg_def = msg_defs.iter().find(|msg| msg.path().eq(global_path));
                if let Some(nested_def) = msg_def {
                    // Recursively parse the nested message
                    self.parse_message_data_recursive(cursor, nested_def, msg_defs)
                } else {
                    // Definition wasn't found/parsed earlier
                    Err(RosbagError::MissingDefinitionError(global_path.to_string()))
                }
            }
        }
    }

    fn open_bag<P: AsRef<Path>>(path: P) -> Result<RosBag> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(RosbagError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Bag file not found: {}", path.display()),
            )));
        }

        debug!("Opening bag file: {}", path.display());

        match RosBag::new(path) {
            Ok(bag) => Ok(bag),
            Err(e) => Err(RosbagError::IoError(std::io::Error::other(format!(
                "Failed to open bag file: {}",
                e
            )))),
        }
    }
}
