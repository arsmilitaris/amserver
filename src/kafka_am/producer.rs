use std::time::Duration;
use kafka::producer::{Producer, Record, RequiredAcks};

pub struct GameProducer {
	pub p: Producer
}

impl GameProducer {
	pub fn new() -> Result<Self, kafka::error::Error> {
		println!("Creating new Producer...");
		match Producer::from_hosts(vec!("139.162.244.70:9092".to_owned()))
			.with_ack_timeout(Duration::from_secs(1))
			.with_required_acks(RequiredAcks::One)
			.create() {
				Ok(producer) => {
					Ok(Self{p: producer})
				},
				Err(e) => {
					println!("There was an error connecting to the kafka broker: {}", e);
					Err(e)
				}
			}
	}
	
	pub fn send (mut self, value: String) {
		println!("DEBUG: Sending record to Kafka...");
		self.p.send(&Record::from_value("topic2", value.as_bytes())).unwrap();
		println!("DEBUG: Sent record to Kafka.");
	}
	
	
}


        

