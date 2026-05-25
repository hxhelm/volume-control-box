#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};
use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::Blocking;
use esp_hal::clock::CpuClock;
use esp_hal::i2c::master::I2c;
use esp_hal::spi::master::Spi;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use volume_control_box::utils::ir_input::{IrInput, IrReceiver};
use volume_control_box::utils::lcd_screen::{Backlight, Display, Lcd};
use volume_control_box::utils::storage::{AppConfig, ConfigStorage};

extern crate alloc;

const VOLUME_INCREMENT: u8 = 4;
const VOLUME_MAX: u8 = 0b1100_0000; // 192 for a max volume of +0db to avoid amplification
const DISPLAY_MAX: u8 = 50;
const LCD_DEVICE_ADDR: u8 = 0x27;
const FLASH_WRITE_INACTIVITY_TIMER: u8 = 60;

static CURRENT_VOLUME: AtomicU8 = AtomicU8::new(50);

#[allow(unused)]
enum VolumeAction {
    Up,
    Down,
    Mute,
    // Expects value to be in 0..DISPLAY range
    Set(u8),
}

esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // TODO: set up wifi connection for home-assistant integration
    // let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // let (mut _wifi_controller, _interfaces) =
    //     esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");
    // info!("Wi-Fi controller set up!");

    let spi = Spi::new(
        peripherals.SPI2,
        esp_hal::spi::master::Config::default().with_frequency(Rate::from_khz(100)),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO19);

    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));
    let i2c = I2c::new(peripherals.I2C0, config)
        .expect("Failed to initialize I2C interface.")
        .with_sda(peripherals.GPIO21)
        .with_scl(peripherals.GPIO22);

    let lcd = Lcd::new(i2c, LCD_DEVICE_ADDR).expect("Failed initializing LCD device");

    spawner.must_spawn(persist_volume(ConfigStorage::new(peripherals.FLASH)));

    // wait for volume read from flash
    Timer::after(Duration::from_millis(500)).await;

    spawner.must_spawn(update_audio(spi));
    spawner.must_spawn(update_screen(lcd));
    spawner.must_spawn(ir_receive(IrReceiver::new(
        peripherals.RMT,
        peripherals.GPIO4,
    )));
}

#[embassy_executor::task]
async fn update_audio(mut spi: Spi<'static, Blocking>) {
    let mut ticker = Ticker::every(Duration::from_millis(100));

    let mut volume_buffer = CURRENT_VOLUME.load(Ordering::Relaxed);

    loop {
        let current_volume = CURRENT_VOLUME.load(Ordering::Relaxed);

        if current_volume != volume_buffer {
            if volume_buffer > current_volume {
                for v in current_volume..=volume_buffer {
                    set_volume_spi(&mut spi, v);
                    Timer::after(Duration::from_millis(2)).await;
                }
            } else {
                for v in volume_buffer..=current_volume {
                    set_volume_spi(&mut spi, v);
                    Timer::after(Duration::from_millis(2)).await;
                }
            };

            volume_buffer = current_volume;
        }

        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn ir_receive(mut ir_receiver: IrReceiver<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(100));

    loop {
        if let Some(ir_input) = ir_receiver.get_incoming_signal().await {
            match ir_input {
                IrInput::TvRemoteVolUp => update_volume(VolumeAction::Up),
                IrInput::TvRemoteVolDown => update_volume(VolumeAction::Down),
            };
        };

        ticker.next().await;
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "Embassy async futures (sequential storage) are pretty big"
)]
#[embassy_executor::task]
async fn persist_volume(mut config_storage: ConfigStorage<'static>) {
    let ticker_duration = 2;
    let mut ticker = Ticker::every(Duration::from_secs(ticker_duration as u64));
    let mut inactivity_timer = 0;

    let mut buffer = [0u8; 128];

    let mut persisted_volume = config_storage
        .read_config(&mut buffer)
        .await
        .unwrap()
        .volume;
    let mut last_observed_volume = persisted_volume;

    // initialize atomic with value from flash storage
    CURRENT_VOLUME.store(persisted_volume, Ordering::Relaxed);

    loop {
        let current_volume = CURRENT_VOLUME.load(Ordering::Relaxed);

        if current_volume != last_observed_volume {
            last_observed_volume = current_volume;
            inactivity_timer = 0;
        }

        if current_volume != persisted_volume {
            if inactivity_timer >= FLASH_WRITE_INACTIVITY_TIMER {
                persisted_volume = current_volume;
                config_storage
                    .write_config(
                        &AppConfig {
                            volume: persisted_volume,
                        },
                        &mut buffer,
                    )
                    .await;
            } else {
                inactivity_timer += ticker_duration;
            }
        }

        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn update_screen(mut lcd: Lcd<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(200));

    lcd.set_display(Display::On).unwrap();
    lcd.set_backlight(Backlight::On).unwrap();

    lcd.clear().unwrap();
    lcd.set_cursor_position(0, 0).unwrap();

    let mut volume_buffer = CURRENT_VOLUME.load(Ordering::Relaxed);

    print_volume_to_screen(volume_buffer, &mut lcd);

    loop {
        let current_volume = CURRENT_VOLUME.load(Ordering::Relaxed);

        if current_volume != volume_buffer {
            volume_buffer = current_volume;
            print_volume_to_screen(volume_buffer, &mut lcd);
        }

        ticker.next().await;
    }
}

fn set_volume_spi(spi: &mut Spi<Blocking>, vol: u8) {
    let frame: [u8; 2] = [vol, vol];
    spi.write(&frame).unwrap();
}

fn update_volume(action: VolumeAction) {
    let mut volume = CURRENT_VOLUME.load(Ordering::Relaxed);

    volume = match action {
        VolumeAction::Up => volume.saturating_add(VOLUME_INCREMENT),
        VolumeAction::Down => volume.saturating_sub(VOLUME_INCREMENT),
        VolumeAction::Mute => 0,
        VolumeAction::Set(new_volume) => display_to_volume(new_volume),
    };

    if volume > VOLUME_MAX {
        volume = VOLUME_MAX;
    }

    CURRENT_VOLUME.store(volume, Ordering::Relaxed);
}

fn display_to_volume(display_volume: u8) -> u8 {
    ((display_volume as u16).saturating_mul(VOLUME_MAX as u16)).saturating_div(DISPLAY_MAX as u16)
        as u8
}

fn volume_to_display(internal_volume: u8) -> u8 {
    ((internal_volume as u16).saturating_mul(DISPLAY_MAX as u16)).saturating_div(VOLUME_MAX as u16)
        as u8
}

fn print_volume_to_screen(volume: u8, lcd: &mut Lcd) {
    lcd.clear().unwrap();
    lcd.print(&format!("Volume: {}", volume_to_display(volume)))
        .unwrap();

    lcd.set_cursor_position(0, 1).unwrap();
    lcd.print(&volume_to_vmeter(volume)).unwrap();
}

fn volume_to_vmeter(volume: u8) -> String {
    let len = (volume as u16 * 14 / 192) as usize;

    let mut s = String::new();
    s.push('[');

    for i in 0..14 {
        if i < len {
            s.push('#');
        } else {
            s.push(' ');
        }
    }

    s.push(']');

    s
}
