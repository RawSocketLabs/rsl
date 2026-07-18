use std::sync::mpsc::{SendError, SyncSender};

pub fn submit_samples(
    sender: &SyncSender<Vec<f32>>,
    samples: &Vec<f32>,
) -> Result<(), SendError<Vec<f32>>> {
    sender.send(samples.clone())
}

#[cfg(test)]
mod tests {
    use super::submit_samples;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn submits_the_complete_buffer() {
        let (sender, receiver) = sync_channel(1);
        let samples = vec![1.0, 2.0, 3.0];

        submit_samples(&sender, &samples).unwrap();

        assert_eq!(receiver.recv().unwrap(), samples);
    }
}
