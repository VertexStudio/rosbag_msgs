use rosbag_msgs::{BagProcessor, MessageLog, MetadataEvent, Result, ValueExt};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let bag_path = PathBuf::from("data/race_1.bag");
    let mut processor = BagProcessor::new(bag_path);

    // Create channels for messages and metadata
    let (imu_sender, mut imu_receiver) = mpsc::channel::<MessageLog>(1000);
    let (metadata_sender, mut metadata_receiver) = mpsc::channel::<MetadataEvent>(100);

    // Register for IMU messages
    processor.register_message("sensor_msgs/Imu", imu_sender)?;

    // Spawn task to handle metadata events
    let metadata_handler = tokio::spawn(async move {
        while let Some(event) = metadata_receiver.recv().await {
            match event {
                MetadataEvent::ConnectionDiscovered(conn) => {
                    println!(
                        "🔗 Connection {}: {} -> {}",
                        conn.id, conn.topic, conn.message_type
                    );
                }
                MetadataEvent::ProcessingStarted => {
                    println!("🚀 Processing started");
                }
                MetadataEvent::ProcessingCompleted(stats) => {
                    print!("✅ {}", stats.format_summary());
                }
            }
        }
    });

    // Spawn task to handle IMU messages (limited by process_bag)
    let imu_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = imu_receiver.recv().await {
            count += 1;

            if let Ok(accel) = msg.data.get_nested_value(&["linear_acceleration"]) {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    accel.get::<f64>("x"),
                    accel.get::<f64>("y"),
                    accel.get::<f64>("z"),
                ) {
                    println!("📊 IMU #{}: accel=({:.2}, {:.2}, {:.2})", count, x, y, z);
                }
            }
        }
        println!("📈 Received {} IMU messages (limited for demo)", count);
    });

    // Process the bag with metadata channel and offset/limit for demo
    let process_result = processor
        .process_bag(Some(metadata_sender), Some(0), Some(10), None, None)
        .await;
        
    if let Ok(Some(pagination)) = &process_result {
        println!("Example pagination: offset={}, limit={}, returned={}, total={}", 
                 pagination.offset, pagination.limit, pagination.returned_count, pagination.total);
    }

    // Channels should be closed automatically when process_bag completes
    // Wait for handlers to complete
    let _ = metadata_handler.await;
    let _ = imu_handler.await;

    process_result.map(|_| ())
}
