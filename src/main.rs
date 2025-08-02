#![no_std]
#![no_main]

extern crate alloc;
use core::cell::RefCell;
use core::ptr::addr_of_mut;

use alloc::boxed::Box;

// mod can;
mod display;
mod game;
mod gauge;
// mod demo_can;
mod car_state;
mod ecs;

use alloc::sync::Arc;
use circ_buffer::RingBuffer;
use embassy_executor::task;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::peripherals::{ADC1, GPIO1};
use esp_hal::system::{CpuControl, Stack};
use esp_hal::timer::AnyTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::twai::{BaudRate, TwaiConfiguration, TwaiMode};
use esp_hal::twai::{EspTwaiFrame, Twai};
use esp_hal::{Async, dma_buffers};
use esp_hal::{
    Blocking,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::master::Spi,
    time::Rate,
};
use esp_hal_embassy::Executor;
use esp_println::{logger::init_logger_from_env, println};
use log::{info, warn};
use static_cell::StaticCell;

use crate::car_state::CarState;
use crate::display::setup_display_task;
use crate::game::initialize_game;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawBufferStatus {
    Flushing,
    Drawing,
    #[default]
    Idle
}

type DrawBufferSignal = Signal<CriticalSectionRawMutex, DrawBufferStatus>;

pub static DRAW_BUFFER_SIGNAL: DrawBufferSignal = Signal::new();

type VoltageAdcPin = AdcPin<GPIO1<'static>, ADC1<'static>>;
type VoltageAdc = Adc<'static, ADC1<'static>, Blocking>;

pub const FRAME_BUFFER_SIZE: usize = LCD_H_RES * LCD_V_RES * 2;
pub const LCD_H_RES: usize = 240;
pub const LCD_V_RES: usize = 240;

static FB_STORAGE: StaticCell<[u8; FRAME_BUFFER_SIZE]> = StaticCell::new();

pub static FRAMEBUFFER: Mutex<
    CriticalSectionRawMutex,
    RefCell<Option<&'static mut [u8; FRAME_BUFFER_SIZE]>>,
> = Mutex::new(RefCell::new(None));

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // Increase heap size as needed.
    esp_alloc::heap_allocator!(size: 100000);
    init_logger_from_env();
    info!("Starting ESP32-S3 CAN Display");
    let buf = FB_STORAGE.init_with(|| [0u8; FRAME_BUFFER_SIZE]);
    // let buf: &'static mut [u8; FB_SIZE] = FB_STORAGE.init([0u8; FB_SIZE]);
    critical_section::with(|cs| {
        // inside here you have the CriticalSection token `cs`
        *FRAMEBUFFER.borrow(cs).borrow_mut() = Some(buf);
    });

    info!("Framebuffer initialized with size: {FRAME_BUFFER_SIZE}");
    // Store it in the Mutex-protected RefCell as `Some`

    let can_frame_channel: CanFrameChannel = Channel::new();
    let can_frame_channel = Box::leak(Box::new(can_frame_channel));

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    let car_state = Arc::new(Mutex::new(RefCell::new(CarState::default())));

    // let systimer = SystemTimer::new(peripherals.SYSTIMER);

    let can_rx = peripherals.GPIO33; // GREY -> yellow
    let can_tx = peripherals.GPIO21; // VIOLET -> white

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timer0: AnyTimer = timg0.timer0.into();
    let timer1: AnyTimer = timg0.timer1.into();
    esp_hal_embassy::init([timer0, timer1]);
    let car_state_async_side = car_state.clone();
    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
            // --- DMA Buffers for SPI ---
            let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(1024);
            let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
            let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

            // --- Display Setup using BSP values ---
            let spi = Spi::<_>::new(
                peripherals.SPI2,
                esp_hal::spi::master::Config::default()
                    .with_frequency(Rate::from_mhz(80))
                    .with_mode(esp_hal::spi::Mode::_0),
            )
            .unwrap()
            .with_sck(peripherals.GPIO10)
            .with_mosi(peripherals.GPIO11)
            .with_dma(peripherals.DMA_CH0)
            .with_buffers(dma_rx_buf, dma_tx_buf)
            .into_async();

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
            let mut adc_config = AdcConfig::default();
            let adc_pin = adc_config.enable_pin(peripherals.GPIO1, Attenuation::_0dB);
            let voltage_adc = Adc::new(peripherals.ADC1, adc_config);

            // let a = voltage_adc.read_oneshot(&mut adc_pin);

            let reset = Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default());

            let lcd_dc = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
            let cs_output = Output::new(peripherals.GPIO9, Level::High, OutputConfig::default());
            executor.run(|spawner| {
                spawner.must_spawn(frame_received(can, sender));
                spawner.must_spawn(car_state_maintainer(car_state_async_side.clone(), receiver));
                spawner.must_spawn(voltage_calculator(
                    adc_pin,
                    voltage_adc,
                    car_state_async_side.clone(),
                ));
                spawner.must_spawn(setup_display_task(
                    spi,
                    reset,
                    cs_output,
                    lcd_dc,
                ));
            });
        })
        .unwrap();

    // Backlight
    let mut backlight = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    backlight.set_high();

    let mut app = initialize_game(
        car_state.clone(),
    );
    // send a 'bootstrap' event to the game loop

    DRAW_BUFFER_SIGNAL
        .signal(DrawBufferStatus::Flushing);
    info!("Sent bootstrap signal to game loop");
    loop {
        // info!("Running app...");
        app.update();
    }
}

#[task]
async fn car_state_maintainer(
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
    receiver: CanFrameReceiver<'static>,
) {
    loop {
        let msg = receiver.receive().await;
        car_state.lock(|state| {
            state.borrow_mut().process_message(msg);
        });
    }
}

#[task]
async fn frame_received(mut twai: Twai<'static, Async>, sender: CanFrameSender<'static>) {
    loop {
        match twai.receive_async().await {
            Ok(message) => {
                info!("Received CAN message: {message:?}");
                sender.send(message).await
            }
            Err(e) => {
                warn!("Error reading message: {e:?}");
            }
        }
    }
}

#[task]
async fn voltage_calculator(
    mut pin: VoltageAdcPin,
    mut adc: VoltageAdc,
    car_state: Arc<Mutex<CriticalSectionRawMutex, RefCell<CarState>>>,
) -> ! {
    let mut buffer: RingBuffer<f32, 16> = RingBuffer::new();
    loop {
        if let Ok(value) = adc.read_oneshot(&mut pin) {
            // info!("Raw: {}", value);
            let converted = 3.3 / ((1 << 12) * 3 * value) as f32;
            // info!("Converted: {}", converted);
            // info!("Length: {}",buffer.get_size());
            buffer.push(converted);
            let sum: f32 = buffer.iter().sum();
            car_state.lock(|state| {
                state.borrow_mut().set_voltage(sum / buffer.len() as f32);
            });
        } else {
            // info!("Would block");
        }
        Timer::after_millis(100).await
    }
}
