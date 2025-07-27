use core::cell::RefCell;

use alloc::{format, sync::Arc};
use bevy_ecs::{
    resource::Resource,
    schedule::Schedule,
    system::{Res, ResMut},
    world::World,
};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as EmbassySpiDevice;
use embassy_sync::{blocking_mutex::{
    raw::{CriticalSectionRawMutex, NoopRawMutex}, Mutex
}, channel::Sender};
use embedded_graphics::{
    mono_font::{
        ascii::FONT_10X20, ascii::FONT_5X7, MonoTextStyle
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_hal::{
    Async, delay::Delay, gpio::Output, spi::master::SpiDmaBus, time::Instant,
};
use lcd_async::{interface::SpiInterface, models::GC9A01, raw_framebuf::RawFrameBuf};

use log::info;

use crate::{
    car_state::CarState, fps::{fps_system, FPSResource}, gauge::{DashboardContext, Gauge}, DrawCompleteChannel, DrawCompleteEvent, FlushCompleteEvent, CHANNEL_SIZE, FRAMEBUFFER
};

// --- Type Alias for the Concrete Display ---
// Use the DMA-enabled SPI bus type.
pub(crate) type GaugeDisplay<'a> = lcd_async::Display<
    SpiInterface<
        EmbassySpiDevice<
            'a,
            embassy_sync::mutex::Mutex<NoopRawMutex, SpiDmaBus<'static, Async>>,
            Output<'static>,
            Delay,
        >,
        Output<'static>,
    >,
    GC9A01,
    Output<'static>,
>;

const LCD_H_RES: usize = 240;
const LCD_V_RES: usize = 240;


/// Draws the game grid using the cell age for color.
fn draw_grid<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    game: &ResMut<AppStateResource>,
    fps: u64,
) -> Result<(), D::Error> {
    let border_color = Rgb565::new(230, 230, 230);

    // Circle::with_center(Point::new(120, 120), 140)
    //     .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE))
    //     .draw(display)?;
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
        format!("fps: {}", fps).as_str(),
        Point::new(65, 170),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
    )
    .draw(display)?;

    Ok(())
}

#[derive(Resource)]
struct AppStateResource {
    state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    last_frame: Instant,
    gauge: Gauge<'static, 240, 240, 10, 162, 255>,
    gauge_context: DashboardContext<'static, 240, 240>,
    frame_counter: u32,
}

#[derive(Resource)]
pub struct FramebufferDrawFlag {
    pub needs_redraw: bool,
}

impl Default for FramebufferDrawFlag {
    fn default() -> Self {
        Self { needs_redraw: true }
    }
}

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




fn render_system(mut game: ResMut<AppStateResource>, mut flag: ResMut<FramebufferDrawFlag>, sender: ResMut<DrawSenderResource>, receiver: ResMut<FlushCompleteReceiverResource>, fps: Res<FPSResource>) {
    let now = Instant::now();
    let duration = now - game.last_frame;
    game.as_mut().last_frame = now;
    let micros = duration.as_micros();
    
    // let fps = if micros != 0 { 1000  / micros } else  { 0};

    // if !flag.needs_redraw {
    //     return;
    // }
    // if count % DRAW_EVERY_NTH_FRAME != 0 {
    //     return;
    // }

    loop { // TODO hot loop
        match receiver.receiver.try_receive() {
            Ok(_) => break,
            Err(_) => {},
        }
    }
    // info!("Waited for flush receive: {}ms", now.elapsed().as_millis());
    use crate::FRAMEBUFFER;

    let buf = FRAMEBUFFER.lock(|fb| {
        let mut fb = fb.borrow_mut();
        fb.take()
    });

    if let Some(mut buf) = buf {
        let mut raw_fb = RawFrameBuf::<Rgb565, _>::new(&mut buf[..], LCD_H_RES, LCD_V_RES);
        let before = Instant::now();
        let value = game.state.lock(|state| {
            let state = state.borrow();
            // Update the gauge value based on the car state.
            state.message_count().try_into().unwrap_or(0)
        }) % 100;
        game.gauge.update_indicated(); // move the needle towards the value, should be a separate system
        let dashboard_context = &game.gauge_context;

        game.gauge.draw_clear_mask(&mut raw_fb, &dashboard_context);
        // info!("Cleared: {}ms", before.elapsed().as_millis());
        // game.gauge.draw_static(&mut fb_res.frame_buf,&dashboard_context);
        game.gauge.draw_dynamic(&mut raw_fb, &dashboard_context);
        // info!("Dynamic draw: {}ms", before.elapsed().as_millis());

        draw_grid(&mut raw_fb, &game, fps.fps).unwrap();
        let after_draw = Instant::now();

        FRAMEBUFFER.lock(|fb| {
            *fb.borrow_mut() = Some(buf); // reclaim the buffer
        });
        // info!("Reclaimed buffer: {}ms", after_draw.elapsed().as_millis());
        loop {
            match sender.sender.try_send(DrawCompleteEvent) {
                Ok(_) => break,
                Err(_) => {},
            }
        }
        // info!("Send completed: {}ms", after_draw.elapsed().as_millis());

        // info!("Draw duration: {}ms", before.elapsed().as_millis());
        flag.needs_redraw = false;
    } else {
        info!("Skipping draw, flush in progress")
    }
}

fn simulate_value(mut game: ResMut<AppStateResource>) {
    let gauge = &mut game.as_mut().gauge;
    let value = gauge.value;
    let new_value = if value < 200 { value + 3 } else { 0 };
    gauge.set_value(new_value);
}

pub(crate) fn setup_game(
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    draw_complete_sender: Sender<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>,
    flush_complete_receiver: embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, FlushCompleteEvent, CHANNEL_SIZE>,
) -> (Schedule, World) {
    let game = AppStateResource {
        state: car_state,
        last_frame: Instant::now(),
        gauge: crate::gauge::Gauge::new_speedo([
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13",
        ]),
        gauge_context: DashboardContext::new(),
        frame_counter: 0,
    };
    loop {
        let buf = FRAMEBUFFER.lock(|fb| {
            let mut fb = fb.borrow_mut();
            fb.take()
        });
        if let Some(fb_res) = buf {
            // Initialize the framebuffer with the static buffer.
            let mut draw = RawFrameBuf::new(fb_res.as_mut_slice(), LCD_H_RES, LCD_V_RES);
            game.gauge.draw_static(&mut draw, &game.gauge_context);
            FRAMEBUFFER.lock(|fb| {
                *fb.borrow_mut() = Some(fb_res); // reclaim the buffer
            });
            let mut world = World::default();
            
            world.insert_resource(game);
            world.insert_resource(FramebufferDrawFlag::default());
            world.insert_resource(DrawSenderResource::new(draw_complete_sender));
            world.insert_resource(FlushCompleteReceiverResource::new(flush_complete_receiver));
            world.insert_resource(FPSResource::new());
            // world.insert_non_send_resource(DisplayResource { display });

            let mut schedule = Schedule::default();
            schedule.add_systems(render_system);
            schedule.add_systems(simulate_value);
            schedule.add_systems(fps_system);
            break (schedule, world);
        } else {
            info!("Framebuffer not initialized (game)");
        }
    }
}
