use core::cell::RefCell;

use alloc::{boxed::Box, format, sync::Arc};
use bevy_ecs::{resource::Resource, schedule::Schedule, system::{NonSendMut, Res, ResMut}, world::World};
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embedded_graphics::{mono_font::{ascii::{FONT_10X20, FONT_6X9}, MonoTextStyle}, pixelcolor::Rgb565, prelude::*, primitives::{Circle, PrimitiveStyle, Rectangle}, text::Text};
use embedded_graphics_framebuf::{backends::FrameBufferBackend, FrameBuf};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{delay::Delay, gpio::Output, spi::master::SpiDmaBus, time::Instant, timer::systimer::SystemTimer, Blocking};
use heapless::String;
use log::info;
use mipidsi::{interface::SpiInterface, models::GC9A01};

use crate::{car_state::CarState, gauge::{DashboardContext, Gauge}};

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
pub(crate) type GaugeDisplay = mipidsi::Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
    GC9A01,
    Output<'static>,
>;

// --- LCD Resolution and FrameBuffer Type Aliases ---
const LCD_H_RES: usize = 240;
const LCD_V_RES: usize = 240;
const LCD_BUFFER_SIZE: usize = LCD_H_RES * LCD_V_RES;

// We want our pixels stored as Rgb565.
type FbBuffer = HeapBuffer<Rgb565, LCD_BUFFER_SIZE>;
// Define a type alias for the complete FrameBuf.
type MyFrameBuf = FrameBuf<Rgb565, FbBuffer>;


#[derive(Resource)]
struct FrameBufferResource {
    frame_buf: MyFrameBuf,
}

impl FrameBufferResource {
    fn new() -> Self {
        // Allocate the framebuffer data on the heap.
        let fb_data: Box<[Rgb565; LCD_BUFFER_SIZE]> = Box::new([Rgb565::BLACK; LCD_BUFFER_SIZE]);
        let heap_buffer = HeapBuffer::new(fb_data);
        let frame_buf = MyFrameBuf::new(heap_buffer, LCD_H_RES, LCD_V_RES);
        Self { frame_buf }
    }
}

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

    let cloned = game.state.lock(|state| {
        state.borrow().clone()
    });

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
    state: Arc<Mutex<CriticalSectionRawMutex,RefCell<CarState>>>,
    last_frame: Instant,
    gauge: Gauge<'static,240,240,10,162,255>,
    gauge_context: DashboardContext<'static,240,240>,
}

// We wrap it as a NonSend resource so that Bevy doesn’t require Sync.
struct DisplayResource {
    display: GaugeDisplay,
}

fn render_system(
    mut display_res: NonSendMut<DisplayResource>,
    mut game: ResMut<AppStateResource>,
    mut fb_res: ResMut<FrameBufferResource>,
) {
    let now = Instant::now();
    let duration = now - game.last_frame;
    game.as_mut().last_frame = now;
    let fps = 1000 / duration.as_millis();

    // Clear the framebuffer.
    // fb_res.frame_buf.clear(Rgb565::BLACK).unwrap();
    // Draw the game grid (using the age-based color) and generation number.
    // draw_grid(&mut fb_res.frame_buf, &game, fps).unwrap();
    let value = game.state.lock(|state| {
        let state = state.borrow();
        // Update the gauge value based on the car state.
        state.message_count().try_into().unwrap_or(0)
    }) % 100;
    game.gauge.update_indicated();
    // let mut line = game.gauge.get_sline1();
    // write!(&mut line,"{}fps",fps);
    game.gauge.set_value(value);
    // info!("FPS: {}, Value: {}", fps, value);

    let dashboard_context = &game.gauge_context;


    game.gauge.draw_clear_mask(&mut fb_res.frame_buf, &dashboard_context);
    // game.gauge.draw_static(&mut fb_res.frame_buf,&dashboard_context);
    game.gauge.draw_dynamic(&mut fb_res.frame_buf,&dashboard_context);
    // Define the area covering the entire framebuffer.
    let area = Rectangle::new(Point::zero(), fb_res.frame_buf.size());
    // Flush the framebuffer to the physical display.
    let after_draw = Instant::now();
    let draw_duration = after_draw - now;
    info!("Draw duration: {}ms", draw_duration.as_millis());
    display_res
        .display
        .fill_contiguous(&area, fb_res.frame_buf.data.iter().copied())
        .unwrap();
    let draw_duration = Instant::now() - after_draw;
    info!("Actual draw duration: {}ms", draw_duration.as_millis());

}


pub(crate) fn setup_game(display: GaugeDisplay, car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>, system_timer: SystemTimer<'static>)->(Schedule, World) {
    // --- Initialize Game Resources ---
    let game = AppStateResource {
        state: car_state,
        last_frame: Instant::now(),
        gauge: crate::gauge::Gauge::new_speedo(["1","2","3","4","5","6","7","8","9","10","11","12","13"]),
        gauge_context: DashboardContext::new()
    };
    let mut fb_res = FrameBufferResource::new();

    game.gauge.draw_static(&mut fb_res.frame_buf, &game.gauge_context);
    let mut world = World::default();
    world.insert_resource(game);
    world.insert_non_send_resource(DisplayResource { display });
    world.insert_resource(fb_res);

    let mut schedule = Schedule::default();
    schedule.add_systems(render_system);
    (schedule, world)
}