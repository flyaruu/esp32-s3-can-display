#![no_std]
#![no_main]


extern crate alloc;
use alloc::{boxed::Box, format};

mod gauge;
mod can;


use bevy_ecs::prelude::*;
use embedded_can::blocking::Can;
use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, iso_8859_14::FONT_10X20, MonoTextStyle}, pixelcolor::Rgb565, prelude::*, primitives::{Circle, PrimitiveStyle, Rectangle}, text::Text, Drawable
};
use embedded_graphics_framebuf::FrameBuf;
use embedded_graphics_framebuf::backends::FrameBufferBackend;
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{delay::Delay, twai::{BaudRate, TwaiConfiguration, TwaiMode}};
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::{
    Blocking,
    gpio::{Input, Level, Output, OutputConfig},
    main,
    rng::Rng,
    spi::master::{Spi, SpiDmaBus},
    time::Rate,
};
use esp_println::{logger::init_logger_from_env, println};
use log::info;
use mipidsi::options::{ColorOrder, Orientation, Rotation};
use mipidsi::{Builder, models::GC9A01};
use mipidsi::{interface::SpiInterface, options::ColorInversion};

use crate::can::CanResource;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", _info);
    loop {}
}

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
type MyDisplay = mipidsi::Display<
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

    Text::new(
        format!("msgs rcv: {}", game.messages_received).as_str(),
        Point::new(65, 120),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
    ).draw(display)?;

    Ok(())
}

// fn write_generation<D: DrawTarget<Color = Rgb565>>(
//     display: &mut D,
//     generation: usize,
// ) -> Result<(), D::Error> {
//     let x = 70;
//     let y = 140;

//     let mut num_str = heapless::String::<20>::new();
//     write!(num_str, "Generation: {}", generation).unwrap();
//     Text::new(
//         num_str.as_str(),
//         Point::new(x, y),
//         MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE),
//     )
//         .draw(display)?;
//     Ok(())
// }

// --- ECS Resources and Systems ---

#[derive(Resource)]
struct AppStateResource {
    messages_received: u32,
}

impl Default for AppStateResource {
    fn default() -> Self {
        Self {
            messages_received: 0,
        }
    }
}

#[derive(Resource)]
struct RngResource(Rng);

// Because our display type contains DMA descriptors and raw pointers, it isn’t Sync.
// We wrap it as a NonSend resource so that Bevy doesn’t require Sync.
struct DisplayResource {
    display: MyDisplay,
}

fn update_can_message(
    mut app_state: ResMut<AppStateResource>,
    mut can_res: ResMut<CanResource>,
) {
    info!("Reading message!");
    if let Some(msg) = can_res.read_message() {
        info!("Message found");
        app_state.messages_received+=1;
    }
}


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

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // Increase heap size as needed.
    esp_alloc::heap_allocator!(size: 150000);
    init_logger_from_env();

    // --- DMA Buffers for SPI ---
    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(1024);
    let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    // --- Display Setup using BSP values ---
    let spi = Spi::<Blocking>::new(
        peripherals.SPI2,
        esp_hal::spi::master::Config::default()
            .with_frequency(Rate::from_mhz(80))
            .with_mode(esp_hal::spi::Mode::_0),
    )
        .unwrap()
        .with_sck(peripherals.GPIO10)
        .with_mosi(peripherals.GPIO11)
        .with_dma(peripherals.DMA_CH0)
        // .with_miso(peripherals.GPIO14)
        .with_buffers(dma_rx_buf, dma_tx_buf);
    let cs_output = Output::new(peripherals.GPIO9, Level::High, OutputConfig::default());
    let spi_delay = Delay::new();
    let spi_device = ExclusiveDevice::new(spi, cs_output, spi_delay).unwrap();

    // LCD interface
    let lcd_dc = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    // Leak a Box to obtain a 'static mutable buffer.
    let buffer: &'static mut [u8; 512] = Box::leak(Box::new([0_u8; 512]));
    let di = SpiInterface::new(spi_device, lcd_dc, buffer);

    let mut display_delay = Delay::new();
    display_delay.delay_ns(500_000u32);

    // Reset pin
    let reset = Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default());
    // Initialize the display using mipidsi's builder.
    let mut display: MyDisplay = Builder::new(GC9A01, di)
        .reset_pin(reset)
        .display_size(240, 240)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .init(&mut display_delay)
        .unwrap();

    display.clear(Rgb565::BLACK).unwrap();

    // Backlight
    let mut backlight = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    backlight.set_high();

    info!("Display initialized");


    let can_rx = peripherals.GPIO33;
    let can_tx = peripherals.GPIO21;

    let can = TwaiConfiguration::new(
        peripherals.TWAI0,
        can_rx,
        can_tx,
        BaudRate::B500K,
        TwaiMode::Normal,
    ).start();



    // --- Initialize Game Resources ---
    let mut game = AppStateResource::default();
    let mut rng_instance = Rng::new(peripherals.RNG);

    let can_instance = CanResource::new(can);
    // Create the framebuffer resource.
    let fb_res = FrameBufferResource::new();

    let mut world = World::default();
    world.insert_resource(game);
    world.insert_resource(RngResource(rng_instance));
    // Insert the display as a non-send resource because its DMA pointers are not Sync.
    world.insert_non_send_resource(DisplayResource { display });
    // Insert the framebuffer resource as a normal resource.
    world.insert_resource(fb_res);
    world.insert_resource(can_instance);


    let mut schedule = Schedule::default();
    // schedule.add_systems(button_reset_system);
    // schedule.add_systems(update_game_state);
    schedule.add_systems(update_can_message);
    schedule.add_systems(render_system);

    let mut loop_delay = Delay::new();

    loop {
        schedule.run(&mut world);
        loop_delay.delay_ms(50u32);
    }
}
