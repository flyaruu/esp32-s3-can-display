use core::cell::RefCell;

use alloc::{format, sync::Arc};
use bevy_app::{App, Update};
use bevy_ecs::{
    resource::Resource,
    schedule::ScheduleLabel,
    system::{Res, ResMut},
};
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    channel::Sender,
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_5X7, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use lcd_async::raw_framebuf::RawFrameBuf;

use log::info;

use crate::{
    CHANNEL_SIZE, DrawCompleteEvent, FRAMEBUFFER, FlushCompleteEvent, LCD_H_RES, LCD_V_RES,
    car_state::CarState,
    ecs::{
        DashboardContextResource, DrawSenderResource, FlushCompleteReceiverResource,
        fps::{FPSResource, fps_system},
        simulate::simulate_value,
    },
    gauge::{DashboardContext, Gauge},
};

/// Draws the game grid using the cell age for color.
fn draw_grid<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    game: &ResMut<AppStateResource>,
    fps: u64,
) -> Result<(), D::Error> {
    let border_color = Rgb565::new(230, 230, 230);

    Rectangle::new(Point::new(10, 10), Size::new(7, 7))
        .into_styled(PrimitiveStyle::with_fill(border_color))
        .draw(display)?;

    let cloned = game.state.lock(|state| state.borrow().clone());

    Text::new(
        format!("msgs rcv: {}", cloned.message_count()).as_str(),
        Point::new(65, 120),
        MonoTextStyle::new(&FONT_5X7, Rgb565::WHITE),
    )
    .draw(display)?;
    Text::new(
        format!("voltage: {}", cloned.voltage()).as_str(),
        Point::new(65, 80),
        MonoTextStyle::new(&FONT_5X7, Rgb565::WHITE),
    )
    .draw(display)?;

    Text::new(
        format!("fps: {fps}").as_str(),
        Point::new(65, 170),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
    )
    .draw(display)?;

    Ok(())
}

#[derive(Resource)]
pub(crate) struct AppStateResource {
    state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    pub(crate) gauge: Gauge<'static, LCD_H_RES, LCD_V_RES, 10, 162, 255>,
}

fn render_system(
    mut game: ResMut<AppStateResource>,
    sender: ResMut<DrawSenderResource>,
    receiver: ResMut<FlushCompleteReceiverResource>,
    fps: Res<FPSResource>,
    dashboard_context: Res<DashboardContextResource<LCD_H_RES, LCD_V_RES>>,
) {
    info!("Rendering frame...");
    loop {
        // TODO hot loop
        match receiver.receiver.try_receive() {
            Ok(_) => break,
            Err(_) => {
                info!("Flush complete event not ready");
            }
        }
    }
    use crate::FRAMEBUFFER;

    let buf = FRAMEBUFFER.lock(|fb| {
        let mut fb = fb.borrow_mut();
        fb.take()
    });

    info!("Got framebuffer");

    if let Some(buf) = buf {
        // Create a new frame buffer with the static buffer.
        let mut raw_fb = RawFrameBuf::<Rgb565, _>::new(&mut buf[..], LCD_H_RES, LCD_V_RES);
        game.gauge.update_indicated(); // move the needle towards the value, should be a separate system
        game.gauge
            .draw_clear_mask(&mut raw_fb, &dashboard_context.context);
        game.gauge
            .draw_dynamic(&mut raw_fb, &dashboard_context.context);
        draw_grid(&mut raw_fb, &game, fps.fps).unwrap();

        // unlock the framebuffer
        FRAMEBUFFER.lock(|fb| {
            *fb.borrow_mut() = Some(buf); // reclaim the buffer
        });
        loop {
            match sender.sender.try_send(DrawCompleteEvent) {
                Ok(_) => break,
                Err(_) => info!("Draw channel full, retrying..."),
            }
        }
    } else {
        info!("Skipping draw, flush in progress")
    }
    info!("Frame rendered");
}

#[derive(ScheduleLabel, Hash, Debug, Clone, Eq, PartialEq)]
struct UpdateSchedule;

pub(crate) fn initialize_game(
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    draw_complete_sender: Sender<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>,
    flush_complete_receiver: embassy_sync::channel::Receiver<
        'static,
        CriticalSectionRawMutex,
        FlushCompleteEvent,
        CHANNEL_SIZE,
    >,
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
            // let schedule = Schedule::new(UpdateSchedule);
            // schedule.add_systems(render_system);
            // schedule.add_systems(simulate_value);
            // schedule.add_systems(fps_system);
            let mut app = App::new();

            // info!("Adding schedule: {:?}", schedule.label());

            app.insert_resource(game)
                .insert_resource(DrawSenderResource::new(draw_complete_sender))
                .insert_resource(FlushCompleteReceiverResource::new(flush_complete_receiver))
                .insert_resource(FPSResource::new())
                .insert_resource(DashboardContextResource::<LCD_H_RES, LCD_V_RES>::new(
                    gauge_context,
                ))
                // .add_schedule(schedule)
                .add_systems(Update, render_system)
                .add_systems(Update, simulate_value)
                .add_systems(Update, fps_system)
                .finish();

            // let mut world = World::default();
            //     world.insert_resource(game);
            //     world.insert_resource(DrawSenderResource::new(draw_complete_sender));
            //     world.insert_resource(FlushCompleteReceiverResource::new(flush_complete_receiver));
            //     world.insert_resource(FPSResource::new());
            //     world.insert_resource(DashboardContextResource::<LCD_H_RES, LCD_V_RES>::new(gauge_context));

            // app.add_schedule(schedule, UpdateSchedule);
            // app.init_schedule(UpdateSchedule);

            // break (schedule, world);
            break app;
        } else {
            info!("Framebuffer not initialized (game), should not happen");
        }
    }
}
