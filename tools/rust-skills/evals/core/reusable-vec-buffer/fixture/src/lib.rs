pub struct ScratchBuffer {
    samples: Vec<f32>,
}

impl ScratchBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
        }
    }

    pub fn begin_block(&mut self) {
        self.samples.clear();
    }

    pub fn push(&mut self, sample: f32) {
        self.samples.push(sample);
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn capacity(&self) -> usize {
        self.samples.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::ScratchBuffer;

    #[test]
    fn clears_without_releasing_capacity() {
        let mut buffer = ScratchBuffer::with_capacity(1024);
        buffer.push(1.0);
        let capacity = buffer.capacity();

        buffer.begin_block();

        assert!(buffer.samples().is_empty());
        assert_eq!(buffer.capacity(), capacity);
    }
}
