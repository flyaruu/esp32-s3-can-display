use bevy_ecs::{resource::Resource, system::ResMut};
use embassy_time::Instant;
use log::info;

#[derive(Resource)]
pub struct FPSResource {
    pub fps: u64,
    pub instant: Instant,
}

impl FPSResource {
    pub fn new() -> Self {
        Self { fps: 0, instant: Instant::now() }
    }

    pub fn process(&mut self, now: Instant) {
        let duration = now - self.instant;
        let micros = duration.as_micros();
        self.fps = if micros != 0 { 1000000 / micros } else { 0 };
        self.instant = now;
    }
}

pub(crate) fn fps_system(
    mut fps_resource: ResMut<FPSResource>,
) {
    fps_resource.process(Instant::now());
}
