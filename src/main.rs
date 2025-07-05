#![no_std]
#![no_main]

extern crate alloc;
use core::cell::RefCell;
use core::ptr::addr_of_mut;

use alloc::boxed::Box;

mod can;
mod gauge;
mod game;
mod demo_can;
mod car_state;

use alloc::sync::Arc;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use embedded_can::Frame;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
};
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::{dma_buffers, Async};
use esp_hal::gpio::DriveMode;
use esp_hal::system::{Cpu, CpuControl, Stack};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::AnyTimer;
use esp_hal::twai::{EspTwaiFrame, Twai};
use esp_hal::{
    Blocking,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::master::Spi,
    time::Rate,
};
use esp_hal::{
    delay::Delay,
    twai::{BaudRate, TwaiConfiguration, TwaiMode},
};
use esp_hal_embassy::Executor;
use esp_println::{logger::init_logger_from_env, println};
use log::{info, warn};
use mipidsi::options::{ColorOrder, Orientation, Rotation};
use mipidsi::{Builder, models::GC9A01};
use mipidsi::{interface::SpiInterface, options::ColorInversion};
use static_cell::StaticCell;

use crate::car_state::CarState;
use crate::game::{setup_game, GaugeDisplay};


static mut APP_CORE_STACK: Stack<8192> = Stack::new();
const CHANNEL_SIZE: usize = 16;
type CanFrameChannel = Channel<CriticalSectionRawMutex, EspTwaiFrame, CHANNEL_SIZE>;
type CanFrameSender<'ch> = Sender<'ch, CriticalSectionRawMutex, EspTwaiFrame, CHANNEL_SIZE>;
type CanFrameReceiver<'ch> = Receiver<'ch, CriticalSectionRawMutex, EspTwaiFrame, CHANNEL_SIZE>;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", _info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // Increase heap size as needed.
    esp_alloc::heap_allocator!(size: 150000);
    init_logger_from_env();

    let can_frame_channel: CanFrameChannel = Channel::new();
    let can_frame_channel = Box::leak(Box::new(can_frame_channel));
    let sender = can_frame_channel.sender();

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    let mut car_state = Arc::new(Mutex::new(RefCell::new(CarState::default())));
    
    static LED_CTRL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();
    let led_ctrl_signal = &*LED_CTRL.init(Signal::new());

    let led_green = Output::new(peripherals.GPIO17, Level::Low, OutputConfig::default());
    let led_yellow = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());


    let can_rx = peripherals.GPIO33; // GREY -> yellow
    let can_tx = peripherals.GPIO21; // VIOLET -> white





    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timer0: AnyTimer = timg0.timer0.into();
    let timer1: AnyTimer = timg0.timer1.into();
    esp_hal_embassy::init([timer0, timer1]);

    let car_state_async_side = car_state.clone();
    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {

            let can = TwaiConfiguration::new(
                    peripherals.TWAI0,
                    can_rx,
                    can_tx,
                    BaudRate::B500K,
                    TwaiMode::Normal,
                )
                .into_async()
                .start();
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            let receiver = can_frame_channel.receiver();
            let sender = can_frame_channel.sender();
            executor.run(|spawner| {
                spawner.spawn(control_led_green(led_green, led_ctrl_signal)).ok();
                spawner.spawn(control_led_yellow(led_yellow)).ok();
                spawner.must_spawn(frame_received(can, sender));
                spawner.must_spawn(car_state_maintainer(car_state_async_side.clone(), receiver));
            });
        })
        .unwrap();

    // Sends periodic messages to control_led, enabling or disabling it.
    println!(
        "Starting enable_disable_led() on core {}",
        Cpu::current() as usize
    );

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
    let mut display: GaugeDisplay = Builder::new(GC9A01, di)
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



    let (mut schedule,mut world) = setup_game(peripherals.RNG, display, car_state.clone());
    let mut loop_delay = Delay::new();

    loop {
        schedule.run(&mut world);
        // loop_delay.delay_ms(10u32);
    }
}

#[task]
async fn car_state_maintainer(car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>, receiver: CanFrameReceiver<'static>) {
    loop {
        let msg= receiver.receive().await;
        car_state.lock(|state| {
            state.borrow_mut().process_message(msg);
        });
        // {
        //     let mut state = car_state.lock().await;
        //     state.process_message(msg);
        // }
    }
}

#[task]
async fn frame_received(mut twai: Twai<'static, Async>, sender: CanFrameSender<'static>) {
    loop {
        match twai.receive_async().await {
            Ok(message) => sender.send(message).await,
            Err(e) => {
                warn!("Error reading message: {:?}", e);
            },
        }
        info!("Message looping!");
    }
}

#[embassy_executor::task]
async fn control_led_green(
    mut led: Output<'static>,
    control: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    info!("Starting green on core {}", Cpu::current() as usize);
    loop {
            info!("green LED on core: {}", Cpu::current() as usize);
            led.set_high();
            Timer::after_secs(1).await;
            info!("green LED off on core: {}", Cpu::current() as usize);
            led.set_low();
            Timer::after_secs(1).await;

    }
    // loop {
    //     if control.wait().await {
    //         info!("LED on");
    //         led.set_low();
    //     } else {
    //         info!("LED off");
    //         led.set_high();
    //     }
    // }
}

#[embassy_executor::task]
async fn control_led_yellow(
    mut led: Output<'static>,
) {
    info!("Starting yellow on core {}", Cpu::current() as usize);
    loop {
            info!("yellow LED on core: {}", Cpu::current() as usize);
            led.set_high();
            Timer::after_millis(300).await;
            info!("yellow LED off on core: {}", Cpu::current() as usize);
            led.set_low();
            Timer::after_millis(100).await;

    }
}
