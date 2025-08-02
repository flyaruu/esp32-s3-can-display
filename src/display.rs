use crate::{DrawBufferStatus, FRAME_BUFFER_SIZE, FRAMEBUFFER, LCD_H_RES, LCD_V_RES};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as EmbassySpiDevice;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embedded_hal::digital::OutputPin;
use esp_hal::{Async, spi::master::SpiDmaBus};
use lcd_async::{
    Builder,
    interface::SpiInterface,
    models::GC9A01,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
};
use log::info;

// Rgb565 uses 2 bytes per pixel

#[task]
pub async fn setup_display_task(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: esp_hal::gpio::Output<'static>,
    cs: esp_hal::gpio::Output<'static>,
    dc: esp_hal::gpio::Output<'static>,
) {
    display_flush_loop(spi_bus, reset, cs, dc).await
}

async fn display_flush_loop<RES: OutputPin, CS: OutputPin, DC: OutputPin>(
    spi_bus: SpiDmaBus<'static, Async>,
    reset: RES,
    cs: CS,
    dc: DC,
) {
    wait_for_flushing().await;
    let spi_mutex: embassy_sync::mutex::Mutex<NoopRawMutex, _> =
        embassy_sync::mutex::Mutex::new(spi_bus);

    let spi_device = EmbassySpiDevice::new(&spi_mutex, cs);
    let lcd_interface = SpiInterface::new(spi_device, dc);

    let mut display = Builder::new(GC9A01, lcd_interface)
        .reset_pin(reset)
        .display_size(LCD_H_RES as u16, LCD_V_RES as u16)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .init(&mut embassy_time::Delay)
        .await
        .unwrap();

    {
        loop {
            let mut maybe_buf: Option<&'static mut [u8; FRAME_BUFFER_SIZE]> =
                FRAMEBUFFER.lock(|fb| fb.borrow_mut().take());
            if let Some(buf) = maybe_buf.take() {
                display
                    .show_raw_data(0, 0, LCD_H_RES as u16, LCD_V_RES as u16, buf)
                    .await
                    .unwrap();
                FRAMEBUFFER.lock(|fb| {
                    *fb.borrow_mut() = Some(buf); // reclaim the buffer
                });
                crate::DRAW_BUFFER_SIGNAL.signal(DrawBufferStatus::Drawing);
            } else {
                info!("Framebuffer not initialized (display)");
            }
            embassy_time::Timer::after_millis(50).await;
        }
    }
}

pub async fn wait_for_flushing() {
    info!("Waiting for display to be ready to flush...");
    loop {
        match crate::DRAW_BUFFER_SIGNAL.wait().await {
            DrawBufferStatus::Flushing => {
                break;
            }
            _ => {
                info!("Waiting for display to flush...");
                embassy_time::Timer::after_millis(100).await;
            }
        }
    }
    info!("Display is ready for flushing");
}
