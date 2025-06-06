use clap::{Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use tokio::sync::mpsc;

use rosbag_msgs::{BagProcessor, MessageLog, MetadataEvent, Result, extract_image_from_message};

async fn fetch_image_command(
    bag: PathBuf,
    topic: String,
    index: Option<usize>,
    timestamp: Option<f64>,
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

    // Process the bag file
    processor.process_bag(None, None, None, None, None).await?;

    // Wait for handler to finish
    let _ = handler.await;

    let images = images_collected.lock().await;

    if images.is_empty() {
        return Err(rosbag_msgs::RosbagError::MessageParsingError(format!(
            "No images found in topic: {}",
            topic
        )));
    }

    // Select image by index or timestamp
    let selected_image = if let Some(target_timestamp) = timestamp {
        // Find image closest to timestamp
        images
            .iter()
            .min_by_key(|img| {
                let img_time = img.time as f64 / 1_000_000_000.0;
                ((img_time - target_timestamp).abs() * 1000.0) as u64
            })
            .cloned()
    } else {
        // Use index (default to 0)
        let idx = index.unwrap_or(0);
        images.get(idx).cloned()
    };

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
                        index.unwrap_or(0)
                    ))
                });

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
        let requested_idx = index.unwrap_or(0);
        Err(rosbag_msgs::RosbagError::MessageParsingError(format!(
            "Image index {} out of range (available: {})",
            requested_idx, available_count
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

    /// Maximum messages to process after offset (for pagination)
    #[clap(long)]
    limit: Option<usize>,

    /// Start time for temporal filtering (seconds since bag start)
    #[clap(long)]
    start: Option<f64>,

    /// Duration for temporal filtering (seconds from start time)
    #[clap(long)]
    duration: Option<f64>,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract image from bag file and save as PNG
    FetchImage {
        /// Path to the ROS bag file
        #[clap(short, long)]
        bag: PathBuf,

        /// Topic containing image data
        #[clap(short, long)]
        topic: String,

        /// Index of image to extract (0-based)
        #[clap(short, long)]
        index: Option<usize>,

        /// Extract image closest to this timestamp (seconds from bag start)
        #[clap(long)]
        timestamp: Option<f64>,

        /// Path to nested image data (comma-separated)
        #[clap(long, value_delimiter = ',')]
        image_path: Vec<String>,

        /// Output file path for the PNG image
        #[clap(short, long)]
        output: Option<PathBuf>,
    },
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
            Commands::FetchImage {
                bag,
                topic,
                index,
                timestamp,
                image_path,
                output,
            } => {
                return fetch_image_command(bag, topic, index, timestamp, image_path, output).await;
            }
        }
    }

    let bag_path = args.bag.ok_or_else(|| {
        rosbag_msgs::RosbagError::MessageParsingError("Bag file path is required".to_string())
    })?;

    info!("Starting rosbag_msgs with file: {}", bag_path.display());

    // Create and configure the processor
    let mut processor = BagProcessor::new(bag_path);

    // Setup metadata channel if requested
    let metadata_sender = if args.metadata {
        let (sender, mut receiver) = mpsc::channel::<MetadataEvent>(100);

        // Spawn task to handle metadata events
        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                match event {
                    MetadataEvent::ConnectionDiscovered(conn) => {
                        print!("{}", conn.format_structure());
                    }
                    MetadataEvent::ProcessingStarted => {
                        println!("Processing started...");
                    }
                    MetadataEvent::ProcessingCompleted(stats) => {
                        print!("{}", stats.format_summary());
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

    for msg_type in &args.messages {
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
    for topic in &args.topics {
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
        .process_bag(
            metadata_sender,
            args.offset,
            args.limit,
            args.start,
            args.duration,
        )
        .await;

    // Wait for all handlers to finish
    for handler in handlers {
        let _ = handler.await;
    }

    process_result
}
