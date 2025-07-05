use core::cell::RefCell;

use alloc::{boxed::Box, format, sync::Arc};
use bevy_ecs::{resource::Resource, schedule::Schedule, system::{NonSendMut, Res, ResMut}, world::World};
use embassy_sync::{blocking_mutex::{raw::CriticalSectionRawMutex, Mutex}, channel::Channel};
use embedded_can::Frame;
use embedded_graphics::{mono_font::{ascii::FONT_10X20, MonoTextStyle}, pixelcolor::Rgb565, prelude::*, primitives::{Circle, PrimitiveStyle, Rectangle}, text::Text};
use embedded_graphics_framebuf::{backends::FrameBufferBackend, FrameBuf};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{delay::Delay, gpio::Output, peripherals::RNG, rng::Rng, spi::master::SpiDmaBus, twai::Twai, Blocking};
use log::info;
use mipidsi::{interface::SpiInterface, models::GC9A01};

use crate::{can::CanResource, car_state::CarState, demo_can::DemoCanResource};

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
    game: &Res<AppStateResource>,
) -> Result<(), D::Error> {
    let border_color = Rgb565::new(230, 230, 230);

    Circle::with_center(Point::new(120, 120), 140)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE))
        .draw(display)?;
    Rectangle::new(Point::new(10, 10), Size::new(7, 7))
        .into_styled(PrimitiveStyle::with_fill(border_color))
        .draw(display)?;

    let count = game.state.lock(|state| {
        state.borrow().message_count()
    });

    Text::new(
        format!("msgs rcv: {}", count).as_str(),
        Point::new(65, 120),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
    )
    .draw(display)?;

    Ok(())
}

#[derive(Resource)]
struct AppStateResource {
    state: Arc<Mutex<CriticalSectionRawMutex,RefCell<CarState>>>,
}

#[derive(Resource)]
struct RngResource(Rng);

// Because our display type contains DMA descriptors and raw pointers, it isn’t Sync.
// We wrap it as a NonSend resource so that Bevy doesn’t require Sync.
struct DisplayResource {
    display: GaugeDisplay,
}

// fn update_can_message(mut app_state: ResMut<AppStateResource>, mut can_res: ResMut<CanResource>) {
//     info!("Reading message!");
//     if let Some(msg) = can_res.read_message() {
//         info!("Message found");
//         app_state.messages_received += 1;
//     }
// }

// fn update_demo_can_message(mut app_state: ResMut<AppStateResource>, mut can_res: ResMut<DemoCanResource>) {
//     if let Some(msg) = can_res.read_message() {
//         app_state.messages_received += 1;
//     }
// }


/// Render the game state by drawing into the offscreen framebuffer and then flushing
/// it to the display via DMA. After drawing the game grid and generation number,
/// we overlay centered text.
fn render_system(
    mut display_res: NonSendMut<DisplayResource>,
    game: Res<AppStateResource>,
    mut fb_res: ResMut<FrameBufferResource>,
) {
    // Clear the framebuffer.
    fb_res.frame_buf.clear(Rgb565::BLACK).unwrap();
    // Draw the game grid (using the age-based color) and generation number.
    draw_grid(&mut fb_res.frame_buf, &game).unwrap();
    // write_generation(&mut fb_res.frame_buf, game.generation).unwrap();

    // Define the area covering the entire framebuffer.
    let area = Rectangle::new(Point::zero(), fb_res.frame_buf.size());
    // Flush the framebuffer to the physical display.
    display_res
        .display
        .fill_contiguous(&area, fb_res.frame_buf.data.iter().copied())
        .unwrap();
}


pub(crate) fn setup_game(rng: RNG<'static>, display: GaugeDisplay, car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>)->(Schedule, World) {
    // --- Initialize Game Resources ---
    let game = AppStateResource {
        state: car_state,
    };
    let rng_instance = Rng::new(rng);

    // let can_instance = CanResource::new(can);
    // let demo_can_instance = DemoCanResource::new();
    // Create the framebuffer resource.
    let fb_res = FrameBufferResource::new();

    let mut world = World::default();
    world.insert_resource(game);
    world.insert_resource(RngResource(rng_instance));
    // Insert the display as a non-send resource because its DMA pointers are not Sync.
    world.insert_non_send_resource(DisplayResource { display });
    // Insert the framebuffer resource as a normal resource.
    world.insert_resource(fb_res);
    // world.insert_resource(can_instance);
    // world.insert_resource(demo_can_instance);

    let mut schedule = Schedule::default();
    // schedule.add_systems(update_can_message);
    // add switch
    // schedule.add_systems(update_demo_can_message);
    schedule.add_systems(render_system);
    (schedule, world)
}