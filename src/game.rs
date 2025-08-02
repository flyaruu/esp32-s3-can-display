use core::cell::RefCell;

use alloc::{string::ToString, sync::Arc};
use bevy_app::{App, Update};
use bevy_ecs::{
    resource::Resource,
    system::{Res, ResMut},
};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embedded_graphics::{
    pixelcolor::Rgb565,
};
use lcd_async::raw_framebuf::RawFrameBuf;

use log::info;

use crate::{
    car_state::CarState, ecs::{
        fps::{fps_system, FPSResource}, simulate::simulate_value, DashboardContextResource
    }, gauge::{DashboardContext, Gauge}, DrawBufferStatus, FRAMEBUFFER, LCD_H_RES, LCD_V_RES
};
#[derive(Resource)]
pub(crate) struct AppStateResource {
    state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    pub(crate) gauge: Gauge<'static, LCD_H_RES, LCD_V_RES, 10, 162, 255>,
}

fn render_system(
    mut game: ResMut<AppStateResource>,
    fps: Res<FPSResource>,
    dashboard_context: Res<DashboardContextResource<LCD_H_RES, LCD_V_RES>>,
) {
    wait_for_drawing();
    use crate::FRAMEBUFFER;

    let buf = FRAMEBUFFER.lock(|fb| {
        let mut fb = fb.borrow_mut();
        fb.take()
    });
    if let Some(buf) = buf {
        // Create a new frame buffer with the static buffer.
        let mut raw_fb = RawFrameBuf::<Rgb565, _>::new(&mut buf[..], LCD_H_RES, LCD_V_RES);
        game.gauge.update_indicated(); // move the needle towards the value, should be a separate system
        game.gauge
            .draw_clear_mask(&mut raw_fb, &dashboard_context.context);
        game.gauge
            .draw_dynamic(&mut raw_fb, &dashboard_context.context);
        // draw_grid(&mut raw_fb, &game, fps.fps).unwrap();
        game.gauge.set_line1("blabla".to_string());
        game.gauge.set_line2("123456".to_string());

        // unlock the framebuffer
        FRAMEBUFFER.lock(|fb| {
            *fb.borrow_mut() = Some(buf); // reclaim the buffer
        });
        crate::DRAW_BUFFER_SIGNAL.signal(DrawBufferStatus::Flushing);
    } else {
        info!("Skipping draw, flush in progress")
    }
}

pub(crate) fn initialize_game(
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
) -> App {
    let game = AppStateResource {
        state: car_state,
        gauge: crate::gauge::Gauge::new_speedo([
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13",
        ]),
    };
    loop {
        let gauge_context = DashboardContext::new();
        let buf = FRAMEBUFFER.lock(|fb| {
            let mut fb = fb.borrow_mut();
            fb.take()
        });

        if let Some(fb_res) = buf {
            // Initialize the framebuffer with the static buffer.
            let mut draw = RawFrameBuf::new(fb_res.as_mut_slice(), LCD_H_RES, LCD_V_RES);
            game.gauge.draw_static(&mut draw, &gauge_context);
            FRAMEBUFFER.lock(|fb| {
                *fb.borrow_mut() = Some(fb_res); // reclaim the buffer
            });
            let mut app = App::new();
            app.insert_resource(game)
                .insert_resource(FPSResource::new())
                .insert_resource(DashboardContextResource::<LCD_H_RES, LCD_V_RES>::new(
                    gauge_context,
                ))
                // .add_schedule(schedule)
                .add_systems(Update, render_system)
                .add_systems(Update, simulate_value)
                .add_systems(Update, fps_system)
                .finish();
            break app;
        } else {
            info!("Framebuffer not initialized (game), should not happen");
        }
    }
}

pub fn wait_for_drawing() {
    loop {
        match crate::DRAW_BUFFER_SIGNAL.try_take(){
            Some(DrawBufferStatus::Drawing) => {
                break
            }
            Some(_status) => {
            }
            None => {
            }
        }
    }
}