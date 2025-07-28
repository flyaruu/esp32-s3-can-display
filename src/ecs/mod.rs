use bevy_ecs::resource::Resource;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Sender};

use crate::{gauge::DashboardContext, DrawCompleteEvent, FlushCompleteEvent, CHANNEL_SIZE};

pub mod fps;
pub mod simulate;

#[derive(Resource)]
pub struct DrawSenderResource {
    pub sender: Sender<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>,
}

impl DrawSenderResource {
    pub fn new(sender: Sender<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>) -> Self {
        Self { sender }
    }
}

#[derive(Resource)]
pub struct FlushCompleteReceiverResource {
    pub receiver: embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, FlushCompleteEvent, CHANNEL_SIZE>,
}



impl FlushCompleteReceiverResource {
    pub fn new(receiver: embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, FlushCompleteEvent, CHANNEL_SIZE>) -> Self {
        Self { receiver }
    }
}

#[derive(Resource)]
pub struct DashboardContextResource<const W: usize, const H: usize> {
    pub context: DashboardContext<'static, W, H>,
}

impl <const W: usize, const H: usize> DashboardContextResource<W, H> {
    pub fn new(context: DashboardContext<'static, W, H>) -> Self {
        Self { context }
    }
    
}