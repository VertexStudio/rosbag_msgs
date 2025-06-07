use clap::{Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use tokio::sync::mpsc;

use rosbag_msgs::{
    BagProcessor, MessageLog, MetadataEvent, Result, extract_image_from_message,
    format_value_as_markdown,
};

async fn fetch_image_command(
    bag: PathBuf,
    topic: String,
    offset: Option<usize>,
    image_path: Vec<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    use std::sync::Arc;

    info!("Extracting image from bag: {}", bag.display());
    info!("Topic: {}", topic);

    let mut processor = BagProcessor::new(bag);

    // Register for the specific image topic
    let (sender, mut receiver) = mpsc::channel::<MessageLog>(100);
    processor.register_topic(&topic, sender)?;

    // Collect images
    let images_collected = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let images_ref = images_collected.clone();

    let handler = tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            let mut images = images_ref.lock().await;
            images.push(msg);
        }
    });

    // Setup metadata channel (not needed for pagination anymore, but kept for consistency)
    let (metadata_sender, mut metadata_receiver) = mpsc::channel::<MetadataEvent>(100);

    // Spawn task to handle metadata events (just consume them)
    let metadata_handler = tokio::spawn(async move {
        while let Some(_) = metadata_receiver.recv().await {
            // Just consume events, we don't need them for pagination
        }
    });

    // Process the bag file using offset/limit for image fetching
    // Default to getting just 1 image at the specified offset
    let limit = Some(1);
    let pagination_info = processor
        .process_bag(Some(metadata_sender), offset, limit, None, None)
        .await?;

    // Wait for handlers to finish
    let _ = handler.await;
    let _ = metadata_handler.await;

    let images = images_collected.lock().await;

    if images.is_empty() {
        return Err(rosbag_msgs::RosbagError::MessageParsingError(format!(
            "No images found in topic: {}",
            topic
        )));
    }

    // Use first (and only) image from the offset/limit result
    let selected_image = images.first().cloned();

    if let Some(image_msg) = selected_image {
        // Convert Vec<String> to Option<&[String]> for the image path
        let image_path_option = if image_path.is_empty() {
            None
        } else {
            Some(image_path.as_slice())
        };

        // Try to extract image data using best-effort parsing
        match extract_image_from_message(&image_msg, image_path_option) {
            Ok(png_data) => {
                let output_path = output.unwrap_or_else(|| {
                    PathBuf::from(format!(
                        "image_{}_{}.png",
                        topic.replace('/', "_"),
                        offset.unwrap_or(0)
                    ))
                });

                // Show pagination information in single line format
                if let Some(ref pagination) = pagination_info {
                    println!(
                        "Pagination: Offset: {} | Limit: {} | Returned: {} | Total: {}",
                        pagination.offset,
                        pagination.limit,
                        pagination.returned_count,
                        pagination.total
                    );
                    println!();
                }

                std::fs::write(&output_path, &png_data)?;
                println!("Image saved to: {}", output_path.display());

                Ok(())
            }
            Err(e) => Err(rosbag_msgs::RosbagError::MessageParsingError(format!(
                "Failed to extract image: {}",
                e
            ))),
        }
    } else {
        let available_count = images.len();
        let requested_offset = offset.unwrap_or(0);
        Err(rosbag_msgs::RosbagError::MessageParsingError(format!(
            "No image found at offset {} (available: {})",
            requested_offset, available_count
        )))
    }
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Option<Commands>,

    /// Path to the ROS bag file (for default messages mode)
    #[clap(short, long, value_parser)]
    bag: Option<PathBuf>,

    /// List of message types to register and print (comma-separated)
    #[clap(short, long, value_delimiter = ',')]
    messages: Vec<String>,

    /// List of topic names to filter by (comma-separated)
    #[clap(short, long, value_delimiter = ',')]
    topics: Vec<String>,

    /// Skip this many messages before starting processing (for pagination)
    #[clap(long)]
    offset: Option<usize>,

    /// Maximum messages to process after offset (defaults to 1 if not specified)
    #[clap(long)]
    limit: Option<usize>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show bag file information (topics, message types, statistics)
    Info {
        /// Path to the ROS bag file
        #[clap(short, long)]
        bag: PathBuf,

        /// Include detailed message type definitions
        #[clap(long, action)]
        definitions: bool,
    },
    /// Extract messages from ROS bag file with filtering and pagination
    Messages {
        /// Path to the ROS bag file
        #[clap(short, long)]
        bag: PathBuf,

        /// List of message types to register and print (comma-separated)
        #[clap(short, long, value_delimiter = ',')]
        messages: Vec<String>,

        /// List of topic names to filter by (comma-separated)
        #[clap(short, long, value_delimiter = ',')]
        topics: Vec<String>,

        /// Skip this many messages before starting processing (for pagination)
        #[clap(long)]
        offset: Option<usize>,

        /// Maximum messages to process after offset (defaults to 1 if not specified)
        #[clap(long)]
        limit: Option<usize>,
    },
    /// Extract images from bag file and save as PNG
    Images {
        /// Path to the ROS bag file
        #[clap(short, long)]
        bag: PathBuf,

        /// Topic containing image data
        #[clap(short, long)]
        topic: String,

        /// Skip this many images before extracting (0-based offset)
        #[clap(long)]
        offset: Option<usize>,

        /// Path to nested image data (comma-separated)
        #[clap(long, value_delimiter = ',')]
        image_path: Vec<String>,

        /// Output file path for the PNG image
        #[clap(short, long)]
        output: Option<PathBuf>,
    },
}

async fn info_command(bag: PathBuf, definitions: bool) -> Result<()> {
    info!("Getting info for bag file: {}", bag.display());

    let mut processor = BagProcessor::new(bag.clone());

    // Setup metadata channel
    let (metadata_sender, mut metadata_receiver) = mpsc::channel::<MetadataEvent>(100);

    // Spawn task to handle metadata events
    let metadata_handler = tokio::spawn(async move {
        let mut basic_lines = Vec::new();
        let mut definition_lines = Vec::new();
        let mut stats_lines = Vec::new();

        while let Some(event) = metadata_receiver.recv().await {
            match event {
                MetadataEvent::ConnectionDiscovered(conn) => {
                    basic_lines.push(conn.format_basic());
                    if definitions {
                        definition_lines.push(conn.format_definition());
                    }
                }
                MetadataEvent::ProcessingStarted => {
                    // Don't include processing started message
                }
                MetadataEvent::ProcessingCompleted(stats) => {
                    stats_lines.push(stats.format_summary());
                }
            }
        }

        // Print stats first
        if !stats_lines.is_empty() {
            for line in stats_lines {
                print!("{}", line);
            }
            println!(); // Empty line separator
        }

        // Print topics list
        if !basic_lines.is_empty() {
            println!("Topics and message types:");
            for line in basic_lines {
                println!("  {}", line);
            }
        }

        // Print definitions if requested
        if definitions && !definition_lines.is_empty() {
            println!(); // Empty line separator
            println!("Message definitions:\n");
            for line in definition_lines {
                println!("{}", line);
            }
        }
    });

    // Process bag for metadata only
    processor
        .process_bag(Some(metadata_sender), None, None, None, None)
        .await?;

    // Wait for metadata handler to finish
    let _ = metadata_handler.await;

    Ok(())
}

async fn process_bag_command(
    bag: PathBuf,
    messages: Vec<String>,
    topics: Vec<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<()> {
    info!("Processing bag file: {}", bag.display());

    // Create and configure the processor
    let mut processor = BagProcessor::new(bag);

    // Setup metadata channel only if we need pagination info
    let needs_pagination = offset.is_some() || limit.is_some();
    let metadata_sender = if needs_pagination {
        let (sender, mut receiver) = mpsc::channel::<MetadataEvent>(100);

        // Spawn task to consume metadata events (we don't print them here)
        tokio::spawn(async move {
            while let Some(_event) = receiver.recv().await {
                // Just consume events, pagination info comes from process_bag result
            }
        });

        Some(sender)
    } else {
        None
    };

    // Setup message handlers - collect messages instead of printing immediately
    let mut handlers = Vec::new();
    let all_messages = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));

    for msg_type in &messages {
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
        processor.register_message(msg_type, sender)?;
        info!("Registered handler for {} messages", msg_type);

        let msg_type_clone = msg_type.clone();
        let messages_ref = all_messages.clone();
        let handler = tokio::spawn(async move {
            let mut message_count = 0;
            while let Some(msg) = receiver.recv().await {
                message_count += 1;
                let mut messages = messages_ref.lock().await;
                messages.push((
                    format!("\n{} [{}] `{}`", msg_type_clone, message_count, msg.topic),
                    msg,
                ));
            }
            info!(
                "Processed {} {} messages total",
                message_count, msg_type_clone
            );
        });

        handlers.push(handler);
    }

    // Setup topic handlers - collect messages instead of printing immediately
    for topic in &topics {
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
        processor.register_topic(topic, sender)?;
        info!("Registered handler for {} topic", topic);

        let topic_clone = topic.clone();
        let messages_ref = all_messages.clone();
        let handler = tokio::spawn(async move {
            let mut message_count = 0;
            while let Some(msg) = receiver.recv().await {
                message_count += 1;
                let mut messages = messages_ref.lock().await;
                messages.push((
                    format!("\n{} [{}] `{}`", msg.msg_path, message_count, topic_clone),
                    msg,
                ));
            }
            info!(
                "Processed {} messages from {} topic total",
                message_count, topic_clone
            );
        });

        handlers.push(handler);
    }

    // Process the bag file
    let process_result = processor
        .process_bag(metadata_sender, offset, limit, None, None)
        .await;

    let pagination_info = match &process_result {
        Ok(pagination) => pagination.clone(),
        Err(_) => None,
    };

    // Wait for all handlers to finish
    for handler in handlers {
        let _ = handler.await;
    }

    // Show pagination info once at the top with actual results (before message content)
    if needs_pagination {
        if let Some(pagination) = &pagination_info {
            println!(
                "Pagination: Offset: {} | Limit: {} | Returned: {} | Total: {}",
                pagination.offset, pagination.limit, pagination.returned_count, pagination.total
            );
            println!();
        }
    }

    // Now print all collected messages
    let messages = all_messages.lock().await;
    for (header, msg) in messages.iter() {
        println!("{}", header);
        println!("{}", format_value_as_markdown(&msg.data, 0));
    }

    process_result.map(|_| ())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with default info level
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env() // Allow overriding level with RUST_LOG env var
        .init();

    let args = Args::parse();

    // Handle subcommands first
    if let Some(command) = args.command {
        match command {
            Commands::Info { bag, definitions } => {
                return info_command(bag, definitions).await;
            }
            Commands::Messages {
                bag,
                messages,
                topics,
                offset,
                limit,
            } => {
                return process_bag_command(bag, messages, topics, offset, limit).await;
            }
            Commands::Images {
                bag,
                topic,
                offset,
                image_path,
                output,
            } => {
                return fetch_image_command(bag, topic, offset, image_path, output).await;
            }
        }
    }

    // Default behavior: extract messages using top-level arguments
    let bag_path = args.bag.ok_or_else(|| {
        rosbag_msgs::RosbagError::MessageParsingError(
            "Bag file path is required. Use --bag <path> or a subcommand.".to_string(),
        )
    })?;

    process_bag_command(
        bag_path,
        args.messages,
        args.topics,
        args.offset,
        args.limit,
    )
    .await
}
