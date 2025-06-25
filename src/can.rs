use alloc::string::String;
use bevy_ecs::resource::Resource;
use esp_hal::{twai::{EspTwaiFrame, Twai}, Blocking};
use log::error;

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
            match self.twai.receive() {
                Ok(msg) => Some(msg),
                Err(e) => {
                    error!("Error fetching frame: {:?}",e);
                    None
                },
            }
        } else {
            None
        }
    }
}