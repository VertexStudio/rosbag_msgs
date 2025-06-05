use clap::Parser;
use env_logger;
use log::info;
use std::path::PathBuf;

use rosbag_msgs::{BagProcessor, Result};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the ROS bag file
    #[clap(short, long, value_parser)]
    bag: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with default info level
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .parse_default_env() // Allow overriding level with RUST_LOG env var
        .init();

    let args = Args::parse();
    info!("Starting rosbag_msgs with file: {}", args.bag.display());

    // Create and configure the processor
    let mut processor = BagProcessor::new(args.bag);

    // Process the bag file
    processor.process_bag().await
}
