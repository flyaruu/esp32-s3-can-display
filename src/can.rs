use alloc::string::String;
use bevy_ecs::resource::Resource;
use esp_hal::{twai::{self, EspTwaiFrame, Twai}, Blocking};
use log::{error, info};

#[derive(Resource)]
pub struct CanResource {
    twai: Twai<'static, Blocking>
}

impl CanResource {

    pub fn new(twai: Twai<'static, Blocking>)->Self {
        Self {
            twai
        }
    }
    pub fn read_message(&mut self)->Option<EspTwaiFrame> {
        if self.twai.num_available_messages() > 0 {
            info!("Available messages: {}", self.twai.num_available_messages());
            match self.twai.receive() {
                Ok(msg) => {
                    self.twai.clear_receive_fifo();
                    Some(msg)
                },
                Err(e) => {
                    self.twai.clear_receive_fifo();
                    error!("Error fetching frame: {:?}",e);
                    None
                },
            }
        } else {
            None
        }
    }
}