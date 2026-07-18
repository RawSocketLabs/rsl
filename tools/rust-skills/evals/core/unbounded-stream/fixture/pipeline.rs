use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub struct Pipeline {
    sender: Sender<Vec<f32>>,
    worker: JoinHandle<()>,
}

impl Pipeline {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || consume(receiver));
        Self { sender, worker }
    }

    pub fn submit(&self, samples: Vec<f32>) {
        self.sender.send(samples).unwrap();
    }
}

fn consume(receiver: Receiver<Vec<f32>>) {
    while let Ok(samples) = receiver.recv() {
        process(&samples);
    }
}

fn process(_samples: &[f32]) {}
