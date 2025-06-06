use clap::Parser;
use log::info;
use std::path::PathBuf;
use tokio::sync::mpsc;

use rosbag_msgs::{BagProcessor, MessageLog, MetadataEvent, Result};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the ROS bag file
    #[clap(short, long, value_parser)]
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

    /// Maximum messages to send to each handler (for limiting output)
    #[clap(long)]
    max: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with default info level
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env() // Allow overriding level with RUST_LOG env var
        .init();

    let args = Args::parse();
    info!("Starting rosbag_msgs with file: {}", args.bag.display());

    // Create and configure the processor
    let mut processor = BagProcessor::new(args.bag);

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

                // Print message info
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

                // Print message info
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
    let process_result = processor.process_bag(metadata_sender, args.max).await;

    // Wait for all handlers to finish
    for handler in handlers {
        let _ = handler.await;
    }

    process_result
}
