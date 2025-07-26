use core::cell::RefCell;

use alloc::{boxed::Box, format, sync::Arc};
use bevy_ecs::{
    resource::Resource,
    schedule::Schedule,
    system::{NonSendMut, Res, ResMut},
    world::World,
};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as EmbassySpiDevice;
use embassy_sync::blocking_mutex::{
    Mutex,
    raw::{CriticalSectionRawMutex, NoopRawMutex},
};
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X9, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_graphics_framebuf::{FrameBuf, backends::FrameBufferBackend};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Async, Blocking, delay::Delay, gpio::Output, spi::master::SpiDmaBus, time::Instant,
    timer::systimer::SystemTimer,
};
use heapless::String;
use lcd_async::{interface::SpiInterface, models::GC9A01, raw_framebuf::RawFrameBuf};

use log::info;

use crate::{
    car_state::CarState,
    gauge::{DashboardContext, Gauge}, FRAMEBUFFER,
};

/// A wrapper around a boxed array that implements FrameBufferBackend.
/// This allows the framebuffer to be allocated on the heap.
pub struct HeapBuffer<C: PixelColor, const N: usize>(Box<[C; N]>);

impl<C: PixelColor, const N: usize> HeapBuffer<C, N> {
    pub fn new(data: Box<[C; N]>) -> Self {
        Self(data)
    }
}

impl<C: PixelColor, const N: usize> core::ops::Deref for HeapBuffer<C, N> {
    type Target = [C; N];
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl<C: PixelColor, const N: usize> core::ops::DerefMut for HeapBuffer<C, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

impl<C: PixelColor, const N: usize> FrameBufferBackend for HeapBuffer<C, N> {
    type Color = C;
    fn set(&mut self, index: usize, color: Self::Color) {
        self.0[index] = color;
    }
    fn get(&self, index: usize) -> Self::Color {
        self.0[index]
    }
    fn nr_elements(&self) -> usize {
        N
    }
}

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

// embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice

// --- LCD Resolution and FrameBuffer Type Aliases ---
const LCD_H_RES: usize = 240;
const LCD_V_RES: usize = 240;
// const DYNAMIC_SCREEN: usize = 162;
// const DYNAMIC_SCREEN_BUFFER_SIZE: usize = DYNAMIC_SCREEN* DYNAMIC_SCREEN;
const LCD_BUFFER_SIZE: usize = LCD_H_RES * LCD_V_RES;

// We want our pixels stored as Rgb565.
type FbBuffer = HeapBuffer<Rgb565, LCD_BUFFER_SIZE>;
// Define a type alias for the complete FrameBuf.
type MyFrameBuf = FrameBuf<Rgb565, FbBuffer>;

// #[derive(Resource)]
// struct FrameBufferResource {
//     frame_buf: MyFrameBuf,
// }

// impl FrameBufferResource {
//     fn new() -> Self {
//         // Allocate the framebuffer data on the heap.
//         let fb_data: Box<[Rgb565; DYNAMIC_SCREEN*DYNAMIC_SCREEN]> = Box::new([Rgb565::BLACK; DYNAMIC_SCREEN*DYNAMIC_SCREEN]);
//         let heap_buffer = HeapBuffer::new(fb_data);
//         let frame_buf = MyFrameBuf::new(heap_buffer, DYNAMIC_SCREEN, DYNAMIC_SCREEN);
//         Self { frame_buf }
//     }
// }

/// Draws the game grid using the cell age for color.
fn draw_grid<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    game: &ResMut<AppStateResource>,
    fps: u64,
) -> Result<(), D::Error> {
    let border_color = Rgb565::new(230, 230, 230);

    Circle::with_center(Point::new(120, 120), 140)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE))
        .draw(display)?;
    Rectangle::new(Point::new(10, 10), Size::new(7, 7))
        .into_styled(PrimitiveStyle::with_fill(border_color))
        .draw(display)?;

    let cloned = game.state.lock(|state| state.borrow().clone());

    Text::new(
        format!("msgs rcv: {}", cloned.message_count()).as_str(),
        Point::new(65, 120),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
    )
    .draw(display)?;
    Text::new(
        format!("voltage: {}", cloned.voltage()).as_str(),
        Point::new(65, 80),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
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

const DRAW_EVERY_NTH_FRAME: u32 = 12;


fn render_system(mut game: ResMut<AppStateResource>, mut flag: ResMut<FramebufferDrawFlag>,) {
    let now = Instant::now();
    let duration = now - game.last_frame;
    game.as_mut().last_frame = now;
    let count = game.as_mut().frame_counter;
    if count > DRAW_EVERY_NTH_FRAME {
        game.as_mut().frame_counter = 0;
    } else {
        game.as_mut().frame_counter += 1;
    }
    // let fps = 1000 / duration.as_millis(); // Watch out for devision by zero

    // if !flag.needs_redraw {
    //     return;
    // }
    if count % DRAW_EVERY_NTH_FRAME != 0 {
        // info!("Skipping frame rendering");
        return;
    }
    use crate::FRAMEBUFFER;

    let buf = FRAMEBUFFER.lock(|fb| {
        let mut fb = fb.borrow_mut();
        fb.take()
    });

    if let Some(mut buf) = buf {
        let mut raw_fb = RawFrameBuf::<Rgb565, _>::new(&mut buf[..], LCD_H_RES, LCD_V_RES);

        let value = game.state.lock(|state| {
            let state = state.borrow();
            // Update the gauge value based on the car state.
            state.message_count().try_into().unwrap_or(0)
        }) % 100;
        let value = game.gauge.value;
        // info!("Gauge value: {}", value);
        game.gauge.update_indicated();
        // game.gauge.set_value(value);

        let dashboard_context = &game.gauge_context;

        // let a= &mut display_res.display;

        game.gauge.draw_clear_mask(&mut raw_fb, &dashboard_context);
        // game.gauge.draw_static(&mut fb_res.frame_buf,&dashboard_context);
        game.gauge.draw_dynamic(&mut raw_fb, &dashboard_context);

        // Define the area covering the entire framebuffer.
        // let area = Rectangle::new(Point::zero(), raw_fb.size());
        // Flush the framebuffer to the physical display.
        // info!(
        //     "DDynamic bounding box: {:?}",
        //     game.gauge.dynamic_bounding_box()
        // );
        let after_draw = Instant::now();
        let draw_duration = after_draw - now;
        info!("Draw duration: {}ms", draw_duration.as_millis());

        FRAMEBUFFER.lock(|fb| {
            *fb.borrow_mut() = Some(buf); // reclaim the buffer
        });


        // let bounding_box = game.gauge.dynamic_bounding_box();
        // let clipped = fb_res.frame_buf.clipped(&bounding_box);
        // let draw_duration = Instant::now() - after_draw;
        // info!("Actual draw duration: {}ms", draw_duration.as_millis());

        flag.needs_redraw = false;
    } else {
        info!("Skipping draw, flush in progress")
        // optional: log or track skipped frame
    }
}

fn simulate_value(mut game: ResMut<AppStateResource>) {
    let gauge = &mut game.as_mut().gauge;
    let value = gauge.value;
    let new_value = if value < 200 {
        value + 1
    } else {
        0
    };
    gauge.set_value(new_value);
}



pub(crate) fn setup_game(
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    
) -> (Schedule, World) {
    // --- Initialize Game Resources ---
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
            let mut draw= RawFrameBuf::new(fb_res.as_mut_slice(), LCD_H_RES, LCD_V_RES);
            game.gauge
                .draw_static(&mut draw, &game.gauge_context);
            FRAMEBUFFER.lock(|fb| {
                *fb.borrow_mut() = Some(fb_res); // reclaim the buffer
            });
            let mut world = World::default();
            world.insert_resource(game);
            world.insert_resource(FramebufferDrawFlag::default());
            // world.insert_non_send_resource(DisplayResource { display });

            let mut schedule = Schedule::default();
            schedule.add_systems(render_system);
            schedule.add_systems(simulate_value);
            break (schedule, world)
        } else {
            info!("Framebuffer not initialized (game)");
        }
}
}
