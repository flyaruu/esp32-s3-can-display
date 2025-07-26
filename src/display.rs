use crate::FRAMEBUFFER;
use alloc::task;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as EmbassySpiDevice;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Timer;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
};
use embedded_hal::digital::OutputPin;
use esp_hal::{Async, spi::master::SpiDmaBus};
use lcd_async::{
    Builder,
    interface::SpiInterface,
    models::GC9A01,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
    raw_framebuf::RawFrameBuf,
};
use log::info;
use static_cell::StaticCell;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;
// Rgb565 uses 2 bytes per pixel
const FRAME_BUFFER_SIZE: usize = WIDTH * HEIGHT * 2;

// Use StaticCell to create a static, zero-initialized buffer.
// static FRAME_BUFFER: StaticCell<[u8; FRAME_BUFFER_SIZE]> = StaticCell::new();

#[task]
pub async fn setup_display_task(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: esp_hal::gpio::Output<'static>,
    cs: esp_hal::gpio::Output<'static>,
    dc: esp_hal::gpio::Output<'static>,
) {
    setup_display(spi_bus, reset, cs, dc).await
}

async fn setup_display<RES: OutputPin, CS: OutputPin, DC: OutputPin>(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: RES,
    cs: CS,
    dc: DC,
) {
    let spi_mutex: embassy_sync::mutex::Mutex<NoopRawMutex, _> =
        embassy_sync::mutex::Mutex::new(spi_bus);

    let spi_device = EmbassySpiDevice::new(&spi_mutex, cs);
    let lcd_interface = SpiInterface::new(spi_device, dc);
    // let frame_buffer = FRAME_BUFFER.init([0; FRAME_BUFFER_SIZE]);

    let mut display = Builder::new(GC9A01, lcd_interface)
        .reset_pin(reset)
        .display_size(240, 240)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .init(&mut embassy_time::Delay)
        .await
        .unwrap();

    {
        // let mut fbuf = RawFrameBuf::<Rgb565, _>::new(frame_buffer.as_mut_slice(), WIDTH, HEIGHT);

        loop {
            let mut maybe_buf: Option<&'static mut [u8; FRAME_BUFFER_SIZE]> =
                FRAMEBUFFER.lock(|fb| fb.borrow_mut().take());
            if let Some(buf) = maybe_buf.take() {
                let now = embassy_time::Instant::now();
                display
                    .show_raw_data(0, 0, WIDTH as u16, HEIGHT as u16, buf)
                    .await
                    .unwrap();

                // let after_draw = embassy_time::Instant::now();
                // let draw_duration = after_draw - now;
                // info!("Draw duration: {}ms", draw_duration.as_millis());
                FRAMEBUFFER.lock(|fb| {
                    *fb.borrow_mut() = Some(buf); // reclaim the buffer
                });
            } else {
                info!("Framebuffer not initialized (display)");
            }

            embassy_time::Timer::after_millis(50).await;

            // fbuf.clear(Rgb565::BLACK).unwrap();
            // Circle::new(Point::new(120, 120), 80)
            //     .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
            //     .draw(&mut fbuf)
            //     .unwrap();
            // Timer::after_millis(10);
        }
        // Draw anything from `embedded-graphics` into the in-memory buffer.
    } // 
}
