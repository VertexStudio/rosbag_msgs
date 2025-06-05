use byteorder::{LittleEndian, ReadBytesExt};
use log::{debug, error, info, trace, warn};
use ros_message::{DataType, Duration, FieldCase, Msg, Time};
use rosbag::{ChunkRecord, IndexRecord, MessageRecord, RosBag};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::sync::mpsc;

pub use ros_message::{MessagePath, Value};
pub use value::{FromValue, ValueExt};

mod value;

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

pub struct Connection {
    pub topic: String,
    pub message_path: MessagePath,
}

// Main struct to handle bag processing
pub struct BagProcessor {
    bag_path: PathBuf,
    connections: HashMap<u32, Connection>,
    definitions: HashMap<MessagePath, Vec<Msg>>,
    registry: HashMap<MessagePath, mpsc::Sender<MessageLog>>,
    message_counts: HashMap<MessagePath, usize>,
}

#[derive(Debug, Clone)]
pub struct MessageLog {
    pub time: u64,
    pub topic: String,
    pub msg_path: MessagePath,
    pub data: Value,
}

impl BagProcessor {
    // Constructor for BagProcessor
    pub fn new(bag_path: PathBuf) -> Self {
        Self {
            bag_path,
            connections: HashMap::new(),
            definitions: HashMap::new(),
            registry: HashMap::new(),
            message_counts: HashMap::new(),
        }
    }

    pub fn register_message(
        &mut self,
        msg_path: &str,
        sender: mpsc::Sender<MessageLog>,
    ) -> Result<()> {
        let msg_path = MessagePath::try_from(msg_path)?;
        self.registry.insert(msg_path, sender);
        Ok(())
    }

    pub async fn process_bag(&mut self) -> Result<()> {
        info!("Starting bag processing for: {}", self.bag_path.display());

        let bag = Self::open_bag(&self.bag_path)?;

        // --- Pass 1: Collect Connection Info and Parse Definitions ---
        info!(
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
                                &conn.topic,
                                &message_path,
                                &conn.message_definition,
                            )?;
                            self.definitions.insert(message_path, dependencies);
                        }
                        Err(e) => {
                            error!(
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
        info!(
            "Finished scanning connections. Found {} unique message types in: {}",
            self.definitions.len(),
            self.bag_path.display()
        );

        // --- Pass 2: Process Messages ---
        info!("Processing message data for: {}", self.bag_path.display());
        let mut total_messages = 0;
        let mut total_messages_processed = 0;

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
                            let msg_defs = self.definitions.get(&msg_path).ok_or_else(|| {
                                RosbagError::MissingDefinitionError(format!(
                                    "Internal Error: Definitions for {} not found despite connection.",
                                    msg_path
                                ))
                            })?;
                            let msg = self.parse_message_data(&message.data, msg_defs);
                            if let Ok(msg) = msg {
                                trace!("Message: {}", msg);
                                total_messages_processed += 1;

                                // Update message count for this path
                                *self.message_counts.entry(msg_path.clone()).or_insert(0) += 1;

                                if let Some(sender) = self.registry.get(&msg_path) {
                                    debug!("--> {}", msg_path);
                                    if let Err(e) = sender
                                        .send(MessageLog {
                                            time: message.time,
                                            topic: connection.topic.clone(),
                                            msg_path,
                                            data: msg,
                                        })
                                        .await
                                    {
                                        warn!("Error sending message: {}", e);
                                        break 'chunk_loop;
                                    }
                                }
                            } else if let Err(e) = msg {
                                error!("Error parsing message: {}", e);
                            }
                        }
                    }
                }
            }
        }

        info!(
            "Total messages in {}: {}",
            self.bag_path.display(),
            total_messages
        );
        info!(
            "Total messages processed in {}: {}",
            self.bag_path.display(),
            total_messages_processed
        );

        // Log message counts per message path
        debug!("Message counts per type:");
        for (msg_path, count) in self.message_counts.iter() {
            debug!("  {}: {}", msg_path, count);
        }

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

        // Print ascii art tree of dependencies

        println!("topic: {}", topic);
        for (i, msg) in messages.iter().enumerate() {
            if i == 0 {
                println!("type: {}", msg.path());
                for field in msg.fields() {
                    println!(
                        "        |      {}: {} {:?}",
                        field.name(),
                        field.datatype(),
                        field.case()
                    );
                }
            } else {
                println!("        |- {}", msg.path());
                for field in msg.fields() {
                    println!(
                        "        |      {}: {} {:?}",
                        field.name(),
                        field.datatype(),
                        field.case()
                    );
                }
            }
        }
        println!("{}", ".".repeat(100));

        Ok(messages)
    }

    // Parses binary message data based on the ros_message::Msg definition
    fn parse_message_data(&self, data: &[u8], msg_defs: &Vec<Msg>) -> Result<Value> {
        if let Some(msg_def) = msg_defs.get(0) {
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
                                warn!(
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
                                warn!(
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
                let msg_def = msg_defs.iter().find(|msg| msg.path().eq(&global_path));
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

        info!("Opening bag file: {}", path.display());

        match RosBag::new(path) {
            Ok(bag) => Ok(bag),
            Err(e) => Err(RosbagError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open bag file: {}", e),
            ))),
        }
    }
}
