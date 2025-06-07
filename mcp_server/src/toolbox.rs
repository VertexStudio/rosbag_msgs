use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::{
    Error as McpError, RoleServer, ServerHandler, model::*, schemars, service::RequestContext, tool,
};
use serde_json::json;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BagInfoRequest {
    /// Absolute or relative path to the ROS bag file (.bag extension)
    #[schemars(
        description = "File path to the ROS bag file to inspect (e.g., 'data/race_1.bag', '/path/to/recording.bag')"
    )]
    pub bag: String,

    /// Include detailed message type definitions in the output
    #[schemars(
        description = "Include complete message type definitions and field structures. When false, shows only topic names and message types."
    )]
    pub definitions: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MessagesRequest {
    /// Absolute or relative path to the ROS bag file (.bag extension)
    #[schemars(
        description = "File path to the ROS bag file to process (e.g., 'data/race_1.bag', '/path/to/recording.bag')"
    )]
    pub bag: String,

    /// Filter and extract specific ROS message types (e.g., sensor data, navigation data)
    #[schemars(
        description = "Array of ROS message types to process (e.g., ['sensor_msgs/Imu', 'nav_msgs/Odometry']). Only messages of these types will be parsed and returned."
    )]
    pub messages: Option<Vec<String>>,

    /// Filter by specific ROS topic names to extract data from particular sensors or publishers
    #[schemars(
        description = "Array of topic names to process (e.g., ['/camera/imu', '/odom']). Only messages from these topics will be parsed and returned."
    )]
    pub topics: Option<Vec<String>>,

    /// Skip this many messages before starting processing (for pagination)
    #[schemars(
        description = "Number of messages to skip before starting processing. Used with 'limit' for pagination through large datasets."
    )]
    pub offset: Option<usize>,

    /// Maximum number of messages to process after offset (for pagination)
    #[schemars(
        description = "Maximum number of messages to process after applying offset. Use with 'offset' to page through large result sets. Defaults to 1 if not specified."
    )]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImagesRequest {
    /// Absolute or relative path to the ROS bag file (.bag extension)
    #[schemars(
        description = "File path to the ROS bag file containing image data (e.g., 'data/race_1.bag', '/path/to/recording.bag')"
    )]
    pub bag: String,

    /// ROS topic name containing image data
    #[schemars(
        description = "Topic name containing image data (e.g., '/camera/image_raw', '/agent3/Comms/recv_sim_comms_visual_detections'). Can be any message type with image information."
    )]
    pub topic: String,

    /// Skip this many images before extracting (0-based offset)
    #[schemars(
        description = "Number of images to skip before extracting (0 = first image, 1 = second, etc.). Defaults to 0 if not specified."
    )]
    pub offset: Option<usize>,

    /// Path to nested image data within the message
    #[schemars(
        description = "Optional path to nested image data (e.g., ['visual_metadata', 0, 'image_chip', 0] for first image in array). If not specified, tries to find image data at the root level."
    )]
    pub image_path: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub offset: usize,
    pub limit: usize,
    pub returned_count: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageData {
    pub time: u64,
    pub topic: String,
    pub message_type: String,
    pub data: rosbag_msgs::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessagesResponse {
    pub pagination: Option<PaginationInfo>,
    pub messages: Vec<MessageData>,
    pub filter: MessageFilter,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageFilter {
    pub message_type: Option<String>,
    pub topic: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataResponse {
    pub topics: HashMap<String, String>,
    pub message_types: Vec<String>,
    pub metadata_text: String,
}

#[derive(Debug)]
struct BagMetadata {
    topics: HashMap<String, String>, // topic -> message_type
    message_types: HashSet<String>,  // unique message types
    stats_output: String,            // processing statistics
    topic_list: Vec<String>,         // formatted topic: message_type lines
    definitions: Vec<String>,        // formatted message definitions
}

#[derive(Clone)]
pub struct Toolbox {}

#[tool(tool_box)]
impl Toolbox {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    /// Handle potentially large output by either returning it directly or storing to file
    async fn handle_large_output(
        &self,
        content: String,
        prefix: &str,
    ) -> Result<CallToolResult, McpError> {
        let handled_content = rosbag_msgs::handle_large_output(content, prefix);
        Ok(CallToolResult::success(vec![Content::text(
            handled_content,
        )]))
    }

    pub fn get_tools_schema_as_json() -> String {
        let tools: Vec<rmcp::model::Tool> = Self::tool_box().list();
        match serde_json::to_string_pretty(&tools) {
            Ok(json_string) => json_string,
            Err(e) => {
                format!(
                    "{{\"error\": \"Failed to serialize tools to JSON: {}\"}}",
                    e
                )
            }
        }
    }

    fn _create_resource_text(&self, uri: &str, name: &str) -> Resource {
        RawResource::new(uri, name.to_string()).no_annotation()
    }

    async fn extract_bag_metadata(
        &self,
        bag_path: &Path,
    ) -> Result<BagMetadata, Box<dyn std::error::Error + Send + Sync>> {
        use rosbag_msgs::{BagProcessor, MetadataEvent};
        use tokio::sync::mpsc;

        let mut processor = BagProcessor::new(bag_path.to_path_buf());
        let (metadata_sender, mut metadata_receiver) = mpsc::channel::<MetadataEvent>(100);

        // Spawn task to collect metadata
        let metadata_task = tokio::spawn(async move {
            let mut topic_list = Vec::new();
            let mut definitions = Vec::new();
            let mut stats_lines = Vec::new();
            let mut discovered_topics = HashMap::new();
            let mut discovered_types = HashSet::new();

            while let Some(event) = metadata_receiver.recv().await {
                match event {
                    MetadataEvent::ConnectionDiscovered(conn) => {
                        discovered_topics.insert(conn.topic.clone(), conn.message_type.to_string());
                        discovered_types.insert(conn.message_type.to_string());
                        topic_list.push(conn.format_basic());
                        definitions.push(conn.format_definition());
                    }
                    MetadataEvent::ProcessingStarted => {
                        // Don't include processing started message
                    }
                    MetadataEvent::ProcessingCompleted(stats) => {
                        stats_lines.push(stats.format_summary());
                    }
                }
            }

            (
                discovered_topics,
                discovered_types,
                stats_lines.join(""),
                topic_list,
                definitions,
            )
        });

        // Process bag for metadata only
        processor
            .process_bag(Some(metadata_sender), None, None, None, None)
            .await?;

        // Collect results
        let (discovered_topics, discovered_types, stats_output, topic_list, definitions) =
            metadata_task.await?;

        Ok(BagMetadata {
            topics: discovered_topics,
            message_types: discovered_types,
            stats_output,
            topic_list,
            definitions,
        })
    }

    #[tool(description = include_str!("descriptions/info.md"))]
    async fn info(
        &self,
        #[tool(aggr)] BagInfoRequest { bag, definitions }: BagInfoRequest,
    ) -> Result<CallToolResult, McpError> {
        let bag_path = PathBuf::from(bag);

        // Extract metadata using the existing function
        match self.extract_bag_metadata(&bag_path).await {
            Ok(metadata) => {
                let show_definitions = definitions.unwrap_or(false);

                let mut output_lines = Vec::new();

                // Add stats
                if !metadata.stats_output.is_empty() {
                    output_lines.push(metadata.stats_output);
                    output_lines.push("".to_string());
                }

                // Add topics section
                output_lines.push("Topics and message types:".to_string());
                for topic_line in &metadata.topic_list {
                    output_lines.push(format!("  {}", topic_line));
                }

                if show_definitions {
                    // Add definitions section
                    output_lines.push("".to_string());
                    output_lines.push("Message definitions:\n".to_string());
                    output_lines.extend(metadata.definitions);

                    let full_info = output_lines.join("\n");
                    self.handle_large_output(full_info, "bag_info").await
                } else {
                    let basic_info = output_lines.join("\n");
                    Ok(CallToolResult::success(vec![Content::text(basic_info)]))
                }
            }
            Err(_e) => Err(McpError::internal_error("Failed to extract bag info", None)),
        }
    }

    #[tool(description = include_str!("descriptions/messages.md"))]
    async fn messages(
        &self,
        #[tool(aggr)] MessagesRequest {
            bag,
            messages,
            topics,
            offset,
            limit,
        }: MessagesRequest,
    ) -> Result<CallToolResult, McpError> {
        use rosbag_msgs::{BagProcessor, MessageLog};
        use tokio::sync::mpsc;

        let bag_path = PathBuf::from(bag);
        let mut processor = BagProcessor::new(bag_path);

        let mut output_lines = Vec::new();

        // Setup message handlers
        let mut handlers = Vec::new();

        if let Some(message_types) = messages {
            for msg_type in &message_types {
                let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
                if let Err(_e) = processor.register_message(msg_type, sender) {
                    return Err(McpError::internal_error(
                        "Failed to register message handler",
                        None,
                    ));
                }

                let msg_type_clone = msg_type.clone();
                let output_lines_clone = Arc::new(Mutex::new(Vec::new()));
                let output_lines_ref = output_lines_clone.clone();

                let handler = tokio::spawn(async move {
                    let mut message_count = 0;
                    while let Some(msg) = receiver.recv().await {
                        message_count += 1;
                        let mut lines = output_lines_ref.lock().await;
                        lines.push(format!(
                            "\n{} [{}] `{}`",
                            msg_type_clone, message_count, msg.topic
                        ));
                        lines.push(rosbag_msgs::format_value_as_markdown(&msg.data, 0));
                    }
                });

                handlers.push((handler, output_lines_clone));
            }
        }

        // Setup topic handlers
        if let Some(topic_names) = topics {
            for topic in &topic_names {
                let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
                if let Err(_e) = processor.register_topic(topic, sender) {
                    return Err(McpError::internal_error(
                        "Failed to register topic handler",
                        None,
                    ));
                }

                let topic_clone = topic.clone();
                let output_lines_clone = Arc::new(Mutex::new(Vec::new()));
                let output_lines_ref = output_lines_clone.clone();

                let handler = tokio::spawn(async move {
                    let mut message_count = 0;
                    while let Some(msg) = receiver.recv().await {
                        message_count += 1;
                        let mut lines = output_lines_ref.lock().await;
                        lines.push(format!(
                            "\n{} [{}] `{}`",
                            msg.msg_path, message_count, topic_clone
                        ));
                        lines.push(rosbag_msgs::format_value_as_markdown(&msg.data, 0));
                    }
                });

                handlers.push((handler, output_lines_clone));
            }
        }

        // Process the bag file
        let process_result = processor.process_bag(None, offset, limit, None, None).await;

        let pagination_info = match &process_result {
            Ok(pagination) => pagination.clone(),
            Err(_) => None,
        };

        // Wait for all handlers to finish and collect output
        for (handler, output_lines_ref) in handlers {
            let _ = handler.await;
            let lines = output_lines_ref.lock().await;
            output_lines.extend(lines.clone());
        }

        match process_result {
            Ok(_) => {
                let mut result_text = String::new();

                // Add pagination information at the beginning if available
                if let Some(pagination) = pagination_info {
                    result_text.push_str(&format!(
                        "Pagination: Offset: {} | Limit: {} | Returned: {} | Total: {}\n\n",
                        pagination.offset,
                        pagination.limit,
                        pagination.returned_count,
                        pagination.total
                    ));
                }

                // Add main content
                if output_lines.is_empty() {
                    result_text.push_str("Bag file processed successfully (no output generated)");
                } else {
                    result_text.push_str(&output_lines.join("\n"));
                }

                // Use the large output handler to potentially store to file
                self.handle_large_output(result_text, "rosbag_process")
                    .await
            }
            Err(_e) => Err(McpError::internal_error("Failed to process bag file", None)),
        }
    }

    #[tool(description = include_str!("descriptions/images.md"))]
    async fn images(
        &self,
        #[tool(aggr)] ImagesRequest {
            bag,
            topic,
            offset,
            image_path,
        }: ImagesRequest,
    ) -> Result<CallToolResult, McpError> {
        use rosbag_msgs::{BagProcessor, MessageLog};
        use tokio::sync::mpsc;

        let bag_path = PathBuf::from(bag);
        let mut processor = BagProcessor::new(bag_path);

        // Register for the specific image topic
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(100);
        if let Err(_e) = processor.register_topic(&topic, sender) {
            return Err(McpError::invalid_params(
                "Failed to register topic handler",
                Some(serde_json::json!({"topic": topic})),
            ));
        }

        // Collect images
        let images_collected = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let images_ref = images_collected.clone();

        let handler = tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                let mut images = images_ref.lock().await;
                images.push(msg);
            }
        });

        // Process the bag file using offset/limit for image fetching
        // Default to getting just 1 image at the specified offset
        let limit = Some(1);
        let process_result = processor.process_bag(None, offset, limit, None, None).await;

        let pagination_info = match &process_result {
            Ok(pagination) => pagination.clone(),
            Err(_) => None,
        };

        // Wait for handler to finish
        let _ = handler.await;

        match process_result {
            Ok(_) => {
                let images = images_collected.lock().await;

                if images.is_empty() {
                    return Err(McpError::invalid_params(
                        "No images found in the specified topic",
                        Some(serde_json::json!({"topic": topic})),
                    ));
                }

                // Use first (and only) image from the offset/limit result
                let selected_image = images.first().cloned();

                if let Some(image_msg) = selected_image {
                    // Try to extract image data using best-effort parsing
                    match rosbag_msgs::extract_image_from_message(&image_msg, image_path.as_deref())
                    {
                        Ok(png_data) => {
                            // Encode as base64
                            let base64_data = base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &png_data,
                            );

                            // Prepare content with pagination info first, then image
                            let mut contents = vec![];

                            // Add pagination information as text content at the beginning
                            let pagination_text = if let Some(ref pagination) = pagination_info {
                                format!(
                                    "Pagination: Offset: {} | Limit: {} | Returned: {} | Total: {}",
                                    pagination.offset,
                                    pagination.limit,
                                    pagination.returned_count,
                                    pagination.total
                                )
                            } else {
                                "No pagination information available".to_string()
                            };
                            contents.push(Content::text(pagination_text));

                            // Add the image content
                            contents.push(Content::image(base64_data, "image/png"));

                            Ok(CallToolResult::success(contents))
                        }
                        Err(e) => Err(McpError::internal_error(
                            "Failed to extract image from message",
                            Some(serde_json::json!({
                                "topic": topic,
                                "message_type": image_msg.msg_path,
                                "error": e.to_string()
                            })),
                        )),
                    }
                } else {
                    let available_count = images.len();
                    let requested_offset = offset.unwrap_or(0);
                    Err(McpError::invalid_params(
                        "No image found at offset",
                        Some(serde_json::json!({
                            "requested_offset": requested_offset,
                            "available_images": available_count,
                            "topic": topic
                        })),
                    ))
                }
            }
            Err(_e) => Err(McpError::internal_error("Failed to process bag file", None)),
        }
    }

    async fn get_messages_resource(
        &self,
        bag_path: &str,
        message_type: Option<&str>,
        topic_name: Option<&str>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<ReadResourceResult, McpError> {
        use rosbag_msgs::{BagProcessor, MessageLog};
        use tokio::sync::mpsc;

        let bag_path_buf = PathBuf::from(bag_path);
        if !bag_path_buf.exists() {
            return Err(McpError::invalid_params("Bag file not found", None));
        }

        let mut processor = BagProcessor::new(bag_path_buf);

        // Setup message collection
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);

        // Register handler based on filter type
        if let Some(msg_type) = message_type {
            if let Err(_e) = processor.register_message(msg_type, sender) {
                return Err(McpError::invalid_params(
                    "Failed to register message type handler",
                    Some(serde_json::json!({"message_type": msg_type})),
                ));
            }
        } else if let Some(topic) = topic_name {
            if let Err(_e) = processor.register_topic(topic, sender) {
                return Err(McpError::invalid_params(
                    "Failed to register topic handler",
                    Some(serde_json::json!({"topic": topic})),
                ));
            }
        } else {
            return Err(McpError::invalid_params(
                "Either message_type or topic_name must be specified",
                None,
            ));
        }

        // Collect messages
        let messages_collected = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let messages_ref = messages_collected.clone();

        let handler = tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                let mut messages = messages_ref.lock().await;
                messages.push(msg);
            }
        });

        // Process the bag file with pagination
        let process_result = processor.process_bag(None, offset, limit, None, None).await;

        let pagination_info = match &process_result {
            Ok(pagination) => pagination.clone(),
            Err(_) => None,
        };

        // Wait for handler to finish
        let _ = handler.await;

        match process_result {
            Ok(_) => {
                let messages = messages_collected.lock().await;

                // Convert messages to proper types
                let message_data: Vec<MessageData> = messages
                    .iter()
                    .map(|msg| MessageData {
                        time: msg.time,
                        topic: msg.topic.clone(),
                        message_type: msg.msg_path.to_string(),
                        data: msg.data.clone(),
                    })
                    .collect();

                // Create response with pagination metadata
                let response_data = MessagesResponse {
                    pagination: pagination_info.map(|p| PaginationInfo {
                        offset: p.offset,
                        limit: p.limit,
                        returned_count: p.returned_count,
                        total: p.total,
                    }),
                    messages: message_data,
                    filter: MessageFilter {
                        message_type: message_type.map(|s| s.to_string()),
                        topic: topic_name.map(|s| s.to_string()),
                    },
                };

                let content = serde_json::to_string_pretty(&response_data).map_err(|_| {
                    McpError::internal_error("Failed to serialize message data", None)
                })?;

                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::text(
                        content,
                        format!(
                            "rosbag://{}/{}{}{}",
                            bag_path,
                            if message_type.is_some() {
                                "messages/"
                            } else {
                                "topics/"
                            },
                            message_type.or(topic_name).unwrap_or(""),
                            if offset.is_some() || limit.is_some() {
                                format!(
                                    "?offset={}&limit={}",
                                    offset.unwrap_or(0),
                                    limit.unwrap_or(1)
                                )
                            } else {
                                String::new()
                            }
                        ),
                    )],
                })
            }
            Err(_e) => Err(McpError::internal_error("Failed to process bag file", None)),
        }
    }
}

// const_string!(Echo = "echo");
#[tool(tool_box)]
impl ServerHandler for Toolbox {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_prompts()
                .enable_resources()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides tools for processing and analyzing ROS bag files. Use bag_info to inspect contents and messages to extract data.".to_string()),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                self._create_resource_text("rosbag://usage", "How to use dynamic bag resources"),
            ],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        // Parse different URI patterns

        // 1. Basic metadata resources: rosbag://{bag_path}/{resource_type}
        if let Some(captures) =
            regex::Regex::new(r"^rosbag://(.+?)/(topics|message_types|metadata)$")
                .unwrap()
                .captures(&uri)
        {
            let bag_path_encoded = captures.get(1).unwrap().as_str();
            let resource_type = captures.get(2).unwrap().as_str();

            // URL decode the bag path
            let bag_path = urlencoding::decode(bag_path_encoded)
                .map_err(|_| McpError::invalid_params("Invalid URL encoding in bag path", None))?;
            let bag_path_buf = PathBuf::from(bag_path.as_ref());
            if !bag_path_buf.exists() {
                return Err(McpError::invalid_params("Bag file not found", None));
            }

            match self.extract_bag_metadata(&bag_path_buf).await {
                Ok(metadata) => {
                    let content = match resource_type {
                        "topics" => {
                            serde_json::to_string_pretty(&metadata.topics).map_err(|_| {
                                McpError::internal_error("Failed to serialize topics", None)
                            })?
                        }
                        "message_types" => {
                            let types_vec: Vec<&String> = metadata.message_types.iter().collect();
                            serde_json::to_string_pretty(&types_vec).map_err(|_| {
                                McpError::internal_error("Failed to serialize message types", None)
                            })?
                        }
                        "metadata" => {
                            let metadata_response = MetadataResponse {
                                topics: metadata.topics,
                                message_types: metadata.message_types.into_iter().collect(),
                                metadata_text: format!(
                                    "{}\n\nTopics and message types:\n{}\n\nMessage definitions:\n{}",
                                    metadata.stats_output,
                                    metadata
                                        .topic_list
                                        .iter()
                                        .map(|t| format!("  {}", t))
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                    metadata.definitions.join("\n")
                                ),
                            };
                            serde_json::to_string_pretty(&metadata_response).map_err(|_| {
                                McpError::internal_error("Failed to serialize metadata", None)
                            })?
                        }
                        _ => unreachable!(),
                    };

                    Ok(ReadResourceResult {
                        contents: vec![ResourceContents::text(content, uri)],
                    })
                }
                Err(_e) => Err(McpError::internal_error("Failed to process bag file", None)),
            }
        }
        // 2. Messages by type with pagination: rosbag://{bag_path}/messages/{message_type}?offset=X&limit=Y
        else if let Some(captures) =
            regex::Regex::new(r"^rosbag://(.+?)/messages/([^?]+)(?:\?(.+))?$")
                .unwrap()
                .captures(&uri)
        {
            let bag_path_encoded = captures.get(1).unwrap().as_str();
            let message_type_encoded = captures.get(2).unwrap().as_str();
            let query_params = captures.get(3).map(|m| m.as_str()).unwrap_or("");

            // URL decode the bag path and message type
            let bag_path = urlencoding::decode(bag_path_encoded)
                .map_err(|_| McpError::invalid_params("Invalid URL encoding in bag path", None))?;
            let message_type = urlencoding::decode(message_type_encoded).map_err(|_| {
                McpError::invalid_params("Invalid URL encoding in message type", None)
            })?;

            // Parse query parameters
            let mut offset = None;
            let mut limit = None;
            for param in query_params.split('&') {
                if let Some((key, value)) = param.split_once('=') {
                    match key {
                        "offset" => offset = value.parse().ok(),
                        "limit" => limit = value.parse().ok(),
                        _ => {}
                    }
                }
            }

            self.get_messages_resource(&bag_path, Some(&message_type), None, offset, limit)
                .await
        }
        // 3. Messages by topic with pagination: rosbag://{bag_path}/topics/{topic_name}?offset=X&limit=Y
        else if let Some(captures) =
            regex::Regex::new(r"^rosbag://(.+?)/topics/([^?]+)(?:\?(.+))?$")
                .unwrap()
                .captures(&uri)
        {
            let bag_path_encoded = captures.get(1).unwrap().as_str();
            let topic_name_encoded = captures.get(2).unwrap().as_str();
            let query_params = captures.get(3).map(|m| m.as_str()).unwrap_or("");

            // URL decode the bag path and topic name
            let bag_path = urlencoding::decode(bag_path_encoded)
                .map_err(|_| McpError::invalid_params("Invalid URL encoding in bag path", None))?;
            let topic_name = urlencoding::decode(topic_name_encoded).map_err(|_| {
                McpError::invalid_params("Invalid URL encoding in topic name", None)
            })?;

            // Parse query parameters
            let mut offset = None;
            let mut limit = None;
            for param in query_params.split('&') {
                if let Some((key, value)) = param.split_once('=') {
                    match key {
                        "offset" => offset = value.parse().ok(),
                        "limit" => limit = value.parse().ok(),
                        _ => {}
                    }
                }
            }

            self.get_messages_resource(&bag_path, None, Some(&topic_name), offset, limit)
                .await
        } else if uri == "rosbag://usage" {
            let usage_text = r#"Dynamic ROS Bag Resources Usage:

Access bag file information using these URI patterns:
• rosbag://{bag_path}/topics - List all topics in the bag file
• rosbag://{bag_path}/message_types - List all message types in the bag file  
• rosbag://{bag_path}/metadata - Complete metadata with structure and statistics
• rosbag://{bag_path}/messages/{message_type}?offset=X&limit=Y - Messages of specific type with pagination
• rosbag://{bag_path}/topics/{topic_name}?offset=X&limit=Y - Messages from specific topic with pagination

Examples:
• rosbag://data/race_1.bag/topics
• rosbag:///absolute/path/to/recording.bag/message_types
• rosbag://relative/path/bag.bag/metadata
• rosbag://data/race_1.bag/messages/sensor_msgs/Imu?offset=0&limit=10
• rosbag://data/race_1.bag/topics/camera/image_raw?offset=5&limit=3

The bag file will be processed automatically to extract real data."#;

            Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(usage_text, uri)],
            })
        } else {
            Err(McpError::resource_not_found(
                "Invalid resource URI format",
                Some(json!({
                    "uri": uri,
                    "supported_patterns": [
                        "rosbag://{bag_path}/{topics|message_types|metadata}",
                        "rosbag://{bag_path}/messages/{message_type}?offset=X&limit=Y",
                        "rosbag://{bag_path}/topics/{topic_name}?offset=X&limit=Y"
                    ]
                })),
            ))
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            next_cursor: None,
            prompts: vec![
                Prompt::new(
                    "inspect_bag_metadata",
                    Some("Generate a command to inspect bag file metadata and structure"),
                    Some(vec![PromptArgument {
                        name: "bag_path".to_string(),
                        description: Some("Path to the ROS bag file to inspect".to_string()),
                        required: Some(true),
                    }]),
                ),
                Prompt::new(
                    "extract_sensor_data",
                    Some("Generate a command to extract specific sensor data from a bag file"),
                    Some(vec![
                        PromptArgument {
                            name: "bag_path".to_string(),
                            description: Some("Path to the ROS bag file".to_string()),
                            required: Some(true),
                        },
                        PromptArgument {
                            name: "sensor_type".to_string(),
                            description: Some(
                                "Type of sensor data (imu, camera, lidar, etc.)".to_string(),
                            ),
                            required: Some(true),
                        },
                    ]),
                ),
                Prompt::new(
                    "topic_filtering",
                    Some("Generate a command to filter and process specific topics"),
                    Some(vec![
                        PromptArgument {
                            name: "bag_path".to_string(),
                            description: Some("Path to the ROS bag file".to_string()),
                            required: Some(true),
                        },
                        PromptArgument {
                            name: "topics".to_string(),
                            description: Some("Comma-separated list of topic names".to_string()),
                            required: Some(true),
                        },
                    ]),
                ),
            ],
        })
    }

    async fn get_prompt(
        &self,
        GetPromptRequestParam { name, arguments }: GetPromptRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        match name.as_str() {
            "inspect_bag_metadata" => {
                let bag_path = arguments
                    .and_then(|json| json.get("bag_path")?.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        McpError::invalid_params(
                            "No bag_path provided to inspect_bag_metadata",
                            None,
                        )
                    })?;

                let prompt = format!(
                    "Use the process_rosbag tool to inspect the metadata and structure of the ROS bag file at '{bag_path}'. \
                    Call the tool with metadata=true to get:\n\
                    - Topic structure with message type definitions\n\
                    - Message counts by type and topic\n\
                    - Processing statistics\n\
                    - Bag duration and timing information\n\n\
                    This will help you understand the contents before extracting specific data."
                );
                Ok(GetPromptResult {
                    description: Some(
                        "Command to inspect bag file metadata and structure".to_string(),
                    ),
                    messages: vec![PromptMessage {
                        role: PromptMessageRole::User,
                        content: PromptMessageContent::text(prompt),
                    }],
                })
            }
            "extract_sensor_data" => {
                let bag_path = arguments
                    .as_ref()
                    .and_then(|json| json.get("bag_path")?.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        McpError::invalid_params(
                            "No bag_path provided to extract_sensor_data",
                            None,
                        )
                    })?;
                let sensor_type = arguments
                    .as_ref()
                    .and_then(|json| json.get("sensor_type")?.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        McpError::invalid_params(
                            "No sensor_type provided to extract_sensor_data",
                            None,
                        )
                    })?;

                let (message_types, description) = match sensor_type.to_lowercase().as_str() {
                    "imu" => (
                        vec!["sensor_msgs/Imu"],
                        "IMU sensor data including orientation, angular velocity, and linear acceleration",
                    ),
                    "camera" | "image" => (
                        vec!["sensor_msgs/Image", "sensor_msgs/CompressedImage"],
                        "camera image data",
                    ),
                    "lidar" | "laser" => (
                        vec!["sensor_msgs/LaserScan", "sensor_msgs/PointCloud2"],
                        "LiDAR/laser scan data",
                    ),
                    "odom" | "odometry" => (vec!["nav_msgs/Odometry"], "robot odometry data"),
                    "pose" => (
                        vec![
                            "geometry_msgs/PoseStamped",
                            "geometry_msgs/PoseWithCovarianceStamped",
                        ],
                        "pose estimation data",
                    ),
                    "twist" | "cmd_vel" => (vec!["geometry_msgs/Twist"], "velocity command data"),
                    _ => (
                        vec![],
                        "unknown sensor type - please specify message types manually",
                    ),
                };

                let prompt = if !message_types.is_empty() {
                    format!(
                        "Use the process_rosbag tool to extract {description} from '{bag_path}'.\n\
                        Call the tool with messages={:?} to process {} messages.\n\
                        You can also add max=10 to limit output for large datasets.",
                        message_types, sensor_type
                    )
                } else {
                    format!(
                        "Use the process_rosbag tool to extract data from '{bag_path}'.\n\
                        First inspect metadata to find available message types, then specify the appropriate message types for {sensor_type} data."
                    )
                };

                Ok(GetPromptResult {
                    description: Some(format!("Command to extract {sensor_type} sensor data")),
                    messages: vec![PromptMessage {
                        role: PromptMessageRole::User,
                        content: PromptMessageContent::text(prompt),
                    }],
                })
            }
            "topic_filtering" => {
                let bag_path = arguments
                    .as_ref()
                    .and_then(|json| json.get("bag_path")?.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        McpError::invalid_params("No bag_path provided to topic_filtering", None)
                    })?;
                let topics = arguments
                    .as_ref()
                    .and_then(|json| json.get("topics")?.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        McpError::invalid_params("No topics provided to topic_filtering", None)
                    })?;

                let topic_list: Vec<&str> = topics.split(',').map(|s| s.trim()).collect();
                let prompt = format!(
                    "Use the process_rosbag tool to filter and process specific topics from '{bag_path}'.\n\
                    Call the tool with topics={:?} to process only these topics:\n{}\n\
                    Add metadata=true to see topic statistics, or max=5 to limit output per topic.\n\n\
                    This is useful for focusing on specific sensors or data streams in large bag files.",
                    topic_list,
                    topic_list
                        .iter()
                        .map(|t| format!("  - {t}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                Ok(GetPromptResult {
                    description: Some("Command to filter and process specific topics".to_string()),
                    messages: vec![PromptMessage {
                        role: PromptMessageRole::User,
                        content: PromptMessageContent::text(prompt),
                    }],
                })
            }
            _ => Err(McpError::invalid_params("prompt not found", None)),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: vec![
                RawResourceTemplate {
                    uri_template: "rosbag://{bag_path}/topics".to_string(),
                    name: "ROS Bag Topics".to_string(),
                    description: Some("List of all topics with message types as JSON".to_string()),
                    mime_type: Some("application/json".to_string()),
                }
                .no_annotation(),
                RawResourceTemplate {
                    uri_template: "rosbag://{bag_path}/message_types".to_string(),
                    name: "ROS Bag Message Types".to_string(),
                    description: Some("List of all unique message types as JSON".to_string()),
                    mime_type: Some("application/json".to_string()),
                }
                .no_annotation(),
                RawResourceTemplate {
                    uri_template: "rosbag://{bag_path}/metadata".to_string(),
                    name: "ROS Bag Metadata".to_string(),
                    description: Some(
                        "Complete metadata including topic structure and statistics as JSON"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                }
                .no_annotation(),
                RawResourceTemplate {
                    uri_template:
                        "rosbag://{bag_path}/messages/{message_type}?offset={offset}&limit={limit}"
                            .to_string(),
                    name: "ROS Bag Messages by Type".to_string(),
                    description: Some(
                        "Messages of specific type with pagination as JSON".to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                }
                .no_annotation(),
                RawResourceTemplate {
                    uri_template:
                        "rosbag://{bag_path}/topics/{topic_name}?offset={offset}&limit={limit}"
                            .to_string(),
                    name: "ROS Bag Messages by Topic".to_string(),
                    description: Some(
                        "Messages from specific topic with pagination as JSON".to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                }
                .no_annotation(),
            ],
        })
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if let Some(http_request_part) = context.extensions.get::<axum::http::request::Parts>() {
            let initialize_headers = &http_request_part.headers;
            let initialize_uri = &http_request_part.uri;
            tracing::info!(?initialize_headers, %initialize_uri, "initialize from http server");
        }
        Ok(self.get_info())
    }
}
