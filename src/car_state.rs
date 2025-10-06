use embedded_can::{
    Frame,
    Id::{Extended, Standard},
};

#[derive(Debug, Default, Clone)]
pub struct CarState {
    message_count: usize,
    avg_voltage: f32,
    rpm: u8,
    engine_load: u8,
    throttle_position: u8,
}

impl CarState {
    pub fn process_message<F: Frame>(&mut self, frame: F) {
        self.message_count += 1;
        let data = match frame.id() {
            Standard(standard_id) => {
                if standard_id.as_raw() == 0x208 {
                    Some(frame.data())
                } else {
                    None
                }
            }
            Extended(_extended_id) => None,
        };
        if let Some(data) = data {
            self.rpm = data[0];
            self.engine_load = data[2];
            self.throttle_position = data[3];
        }
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }

    pub fn set_voltage(&mut self, value: f32) {
        self.avg_voltage = value;
    }

    pub fn rpm(&self) -> u8 {
        self.rpm
    }
    pub fn engine_load(&self) -> u8 {
        self.engine_load
    }
    pub fn throttle_position(&self) -> u8 {
        self.throttle_position
    }
}
