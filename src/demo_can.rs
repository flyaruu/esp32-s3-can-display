use alloc::string::{String, ToString};
use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub struct DemoCanResource {
}

impl DemoCanResource {
    pub fn new() -> Self {
        Self { }
    }
    pub fn read_message(&mut self) -> Option<String> {
        Some("message".to_string())
    }
}
