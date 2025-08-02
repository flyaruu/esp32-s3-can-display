use bevy_ecs::resource::Resource;

use crate::{gauge::DashboardContext};

pub mod fps;
pub mod simulate;


#[derive(Resource)]
pub struct DashboardContextResource<const W: usize, const H: usize> {
    pub context: DashboardContext<'static, W, H>,
}

impl<const W: usize, const H: usize> DashboardContextResource<W, H> {
    pub fn new(context: DashboardContext<'static, W, H>) -> Self {
        Self { context }
    }
}
