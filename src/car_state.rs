use embedded_can::{
    Frame,
    Id::{Extended, Standard},
};

#[derive(Debug, Default, Clone)]
pub struct CarState {
    message_count: usize,
    avg_voltage: f32,
}

impl CarState {
    pub fn process_message<F: Frame>(&mut self, frame: F) {
        match frame.id() {
            Standard(standard_id) => if standard_id.as_raw() == 0x7e0 {},
            Extended(_extended_id) => todo!(),
        }
        self.message_count += 1
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }

    pub fn voltage(&self) -> f32 {
        self.avg_voltage
    }

    pub fn set_voltage(&mut self, value: f32) {
        self.avg_voltage = value;
    }
}
