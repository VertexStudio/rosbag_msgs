use clap::Parser;
use env_logger;
use log::info;
use std::path::PathBuf;
use tokio::sync::mpsc;

use rosbag_msgs::{BagProcessor, MessageLog, Result, ValueExt};

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

    // Create a channel to receive IMU messages
    let (imu_sender, mut imu_receiver) = mpsc::channel::<MessageLog>(1000);

    // Register for IMU messages
    processor.register_message("sensor_msgs/Imu", imu_sender)?;
    info!("Registered handler for sensor_msgs/Imu messages");

    // Spawn a task to handle IMU messages
    let imu_handler = tokio::spawn(async move {
        let mut message_count = 0;
        while let Some(msg) = imu_receiver.recv().await {
            message_count += 1;

            // Extract some values from the IMU message
            if let Ok(linear_accel) = msg.data.get_nested_value(&["linear_acceleration"]) {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    linear_accel.get::<f64>("x"),
                    linear_accel.get::<f64>("y"),
                    linear_accel.get::<f64>("z"),
                ) {
                    if message_count % 1000 == 0 {
                        // Print every 1000th message
                        info!(
                            "IMU #{}: Linear acceleration: x={:.3}, y={:.3}, z={:.3}",
                            message_count, x, y, z
                        );
                    }
                }
            }
        }
        info!("Processed {} IMU messages total", message_count);
    });

    // Process the bag file
    let process_result = processor.process_bag().await;

    // Wait for the IMU handler to finish
    let _ = imu_handler.await;

    process_result
}
