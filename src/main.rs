use clap::{Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use tokio::sync::mpsc;

use rosbag_msgs::{BagProcessor, MessageLog, MetadataEvent, Result, extract_image_from_message};

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

                std::fs::write(&output_path, &png_data)?;
                println!("Image saved to: {}", output_path.display());
                
                // Show pagination information
                if let Some(ref pagination) = pagination_info {
                    println!("\nPagination:");
                    println!("  Offset: {}", pagination.offset);
                    println!("  Limit: {}", pagination.limit);
                    println!("  Returned: {}", pagination.returned_count);
                    println!("  Total: {}", pagination.total);
                }
                
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

    /// Path to the ROS bag file (for process_bag mode)
    #[clap(short, long, value_parser)]
    bag: Option<PathBuf>,

    /// Print metadata (connections and stats)
    #[clap(long, action)]
    metadata: bool,

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
    /// Process ROS bag file and extract data with filtering and pagination
    ProcessBag {
        /// Path to the ROS bag file
        #[clap(short, long)]
        bag: PathBuf,

        /// Print metadata (connections and stats)
        #[clap(long, action)]
        metadata: bool,

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
    /// Extract image from bag file and save as PNG
    FetchImage {
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

async fn process_bag_command(
    bag: PathBuf,
    metadata: bool,
    messages: Vec<String>,
    topics: Vec<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<()> {
    info!("Processing bag file: {}", bag.display());

    // Create and configure the processor
    let mut processor = BagProcessor::new(bag);

    // Setup metadata channel if requested OR if we need pagination info
    let needs_pagination = offset.is_some() || limit.is_some();
    let metadata_sender = if metadata || needs_pagination {
        let (sender, mut receiver) = mpsc::channel::<MetadataEvent>(100);

        // Spawn task to handle metadata events
        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                match event {
                    MetadataEvent::ConnectionDiscovered(conn) => {
                        if metadata {
                            print!("{}", conn.format_structure());
                        }
                    }
                    MetadataEvent::ProcessingStarted => {
                        if metadata {
                            println!("Processing started...");
                        }
                    }
                    MetadataEvent::ProcessingCompleted(stats) => {
                        if metadata {
                            print!("{}", stats.format_summary());
                        }
                    }
                }
            }
        });

        Some(sender)
    } else {
        None
    };

    // Setup message handlers
    let mut handlers = Vec::new();

    for msg_type in &messages {
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
        processor.register_message(msg_type, sender)?;
        info!("Registered handler for {} messages", msg_type);

        let msg_type_clone = msg_type.clone();
        let handler = tokio::spawn(async move {
            let mut message_count = 0;
            while let Some(msg) = receiver.recv().await {
                message_count += 1;

                // Print message info directly
                println!(
                    "{} #{} [{}]: {}",
                    msg_type_clone,
                    message_count,
                    msg.topic,
                    serde_json::to_string(&msg.data)
                        .unwrap_or_else(|_| "<parse error>".to_string())
                );
            }
            info!(
                "Processed {} {} messages total",
                message_count, msg_type_clone
            );
        });

        handlers.push(handler);
    }

    // Setup topic handlers
    for topic in &topics {
        let (sender, mut receiver) = mpsc::channel::<MessageLog>(1000);
        processor.register_topic(topic, sender)?;
        info!("Registered handler for {} topic", topic);

        let topic_clone = topic.clone();
        let handler = tokio::spawn(async move {
            let mut message_count = 0;
            while let Some(msg) = receiver.recv().await {
                message_count += 1;

                // Print message info directly
                println!(
                    "{} #{} [{}]: {}",
                    msg.msg_path,
                    message_count,
                    topic_clone,
                    serde_json::to_string(&msg.data)
                        .unwrap_or_else(|_| "<parse error>".to_string())
                );
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

    // Show pagination info if available and not showing metadata
    if !metadata && needs_pagination {
        if let Some(pagination) = &pagination_info {
            println!("\nPagination:");
            println!("  Offset: {}", pagination.offset);
            println!("  Limit: {}", pagination.limit);
            println!("  Returned: {}", pagination.returned_count);
            println!("  Total: {}", pagination.total);
        }
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
            Commands::ProcessBag {
                bag,
                metadata,
                messages,
                topics,
                offset,
                limit,
            } => {
                return process_bag_command(bag, metadata, messages, topics, offset, limit).await;
            }
            Commands::FetchImage {
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

    // Default behavior: process bag using top-level arguments
    let bag_path = args.bag.ok_or_else(|| {
        rosbag_msgs::RosbagError::MessageParsingError(
            "Bag file path is required. Use --bag <path> or process-bag subcommand.".to_string(),
        )
    })?;

    process_bag_command(
        bag_path,
        args.metadata,
        args.messages,
        args.topics,
        args.offset,
        args.limit,
    )
    .await
}
