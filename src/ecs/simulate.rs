use bevy_ecs::system::ResMut;
use log::info;

use crate::game::AppStateResource;

pub(crate) fn simulate_value(mut game: ResMut<AppStateResource>) {
    let gauge = &mut game.as_mut().gauge;
    let value = gauge.value;
    let new_value = if value < 200 { value + 3 } else { 0 };
    info!("Simulating value: {value} -> {new_value}");
    gauge.set_value(new_value);
}
