use kafka::consumer::Consumer;
use bevy::prelude::*;

#[derive(Debug, Resource)]
pub struct GameConsumer {
    pub c: Consumer
}

impl GameConsumer {
    pub fn new(topic: &str) -> Result<Self, kafka::error::Error> {
        println!("Creating new Consumer...");
        match Consumer::from_hosts(vec!("139.162.244.70:9092".to_owned()))
            .with_topic_partitions(topic.to_owned(), &[0])
            .with_fallback_offset(kafka::consumer::FetchOffset::Earliest)
            .with_group("testgroup".to_owned())
            .with_offset_storage(kafka::consumer::GroupOffsetStorage::Kafka)
            .create() {
                Ok(consumer) => {
                    Ok(Self{c: consumer})
                },
                Err(e) => {
                    println!("There was an error connecting to kafka broker: {:?}", e);
                    Err(e)
                }
            }

    }

    pub fn recv(mut self) -> String {
        
        let mut message = String::from("");
        
		for ms in self.c.poll().unwrap().iter() {
			for n in (ms.messages().len() - 1)..ms.messages().len() {
				println!("{:?}", ms.messages()[n]);
				println!("{}", n);
				message = String::from_utf8_lossy(ms.messages()[n].value).into();
			}
		};
		match self.c.commit_consumed() {
			Ok(()) => {
				println!("Committed consumed messages...");
			},
			Err(e) => {
				println!("Error committing consumed messages: {:?}", e);
			}
		};		
        return message;
    }
}