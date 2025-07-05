use embedded_can::Frame;

#[derive(Debug,Default,Clone)]
pub struct CarState {
    message_count: usize,
}

impl CarState {
    pub fn process_message<F: Frame>(&mut self, frame: F) {
        self.message_count+=1
    }

    pub fn message_count(&self)->usize {
        self.message_count
    }
}