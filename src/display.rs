use crate::{DrawCompleteEvent, FlushCompleteEvent, CHANNEL_SIZE, FRAMEBUFFER};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as EmbassySpiDevice;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embedded_hal::digital::OutputPin;
use esp_hal::{Async, spi::master::SpiDmaBus};
use lcd_async::{
    Builder,
    interface::SpiInterface,
    models::GC9A01,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
};
use log::info;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;
// Rgb565 uses 2 bytes per pixel
const FRAME_BUFFER_SIZE: usize = WIDTH * HEIGHT * 2;

#[task]
pub async fn setup_display_task(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: esp_hal::gpio::Output<'static>,
    cs: esp_hal::gpio::Output<'static>,
    dc: esp_hal::gpio::Output<'static>,
    draw_complete_receiver: embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>,
    flush_complete_sender: embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, FlushCompleteEvent, CHANNEL_SIZE>,
) {
    display_flush_loop(spi_bus, reset, cs, dc, draw_complete_receiver, flush_complete_sender).await
}

async fn display_flush_loop<RES: OutputPin, CS: OutputPin, DC: OutputPin>(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: RES,
    cs: CS,
    dc: DC,
    draw_complete_receiver: embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, DrawCompleteEvent, CHANNEL_SIZE>,
    flush_complete_sender: embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, FlushCompleteEvent, CHANNEL_SIZE>,
) {
    let spi_mutex: embassy_sync::mutex::Mutex<NoopRawMutex, _> =
        embassy_sync::mutex::Mutex::new(spi_bus);

    let spi_device = EmbassySpiDevice::new(&spi_mutex, cs);
    let lcd_interface = SpiInterface::new(spi_device, dc);

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
        loop {
            draw_complete_receiver.receive().await; // wait for signal
            let mut maybe_buf: Option<&'static mut [u8; FRAME_BUFFER_SIZE]> =
                FRAMEBUFFER.lock(|fb| fb.borrow_mut().take());
            if let Some(buf) = maybe_buf.take() {
                display
                    .show_raw_data(0, 0, WIDTH as u16, HEIGHT as u16, buf)
                    .await
                    .unwrap();
                FRAMEBUFFER.lock(|fb| {
                    *fb.borrow_mut() = Some(buf); // reclaim the buffer
                });
                flush_complete_sender.send(FlushCompleteEvent).await;
            } else {
                info!("Framebuffer not initialized (display)");
            }
            embassy_time::Timer::after_millis(50).await;
        }
    }
}
