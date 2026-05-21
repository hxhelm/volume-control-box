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
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::i2c::master::I2c;
use esp_hal::spi::Mode;
use esp_hal::spi::master::Spi;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Blocking, main};
use volume_control_box::utils::ir_input::{IrInput, IrReceiver};
use volume_control_box::utils::lcd_screen::{Backlight, Display, Lcd};
use volume_control_box::utils::storage::VolumeStorage;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const VOLUME_INCREMENT: u8 = 4;
const VOLUME_MAX: u8 = 0b1100_0000; // 192 for a max volume of +0db to avoid amplification

const DISPLAY_MAX: u8 = 50;

const LCD_DEVICE_ADDR: u8 = 0x27;

#[allow(unused)]
enum VolumeAction {
    Up,
    Down,
    Mute,
    // Expects value to be in 0..DISPLAY range
    Set(u8),
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);
    let delay = Delay::new();

    // TODO: set up wifi connection for home-assistant integration
    // let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // let (mut _wifi_controller, _interfaces) =
    //     esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");
    // info!("Wi-Fi controller set up!");

    delay.delay_millis(500);

    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));
    let i2c = I2c::new(peripherals.I2C0, config)
        .expect("Failed to initialize I2C interface.")
        .with_sda(peripherals.GPIO21)
        .with_scl(peripherals.GPIO22);

    delay.delay_millis(100);

    let mut lcd = Lcd::new(i2c, LCD_DEVICE_ADDR).expect("Failed initializing LCD device");

    delay.delay_millis(200);

    let mut spi = Spi::new(
        peripherals.SPI2,
        esp_hal::spi::master::Config::default()
            .with_frequency(Rate::from_khz(100))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO19);

    delay.delay_millis(50);

    let mut volume_storage = VolumeStorage::new(peripherals.FLASH);
    let mut volume = volume_storage.read_volume();

    delay.delay_millis(100);

    let mut ir_receiver = IrReceiver::new(peripherals.RMT, peripherals.GPIO4);

    delay.delay_millis(50);

    set_volume_spi(&mut spi, volume);

    delay.delay_millis(20);

    lcd.set_display(Display::On).unwrap();
    lcd.set_backlight(Backlight::On).unwrap();

    lcd.clear().unwrap();
    lcd.set_cursor_position(0, 0).unwrap();

    print_volume(volume, &mut lcd);

    delay.delay_millis(100);

    loop {
        let Some(ir_input) = ir_receiver.get_incoming_signal() else {
            delay.delay_millis(100);
            continue;
        };

        let volume_buffer = match ir_input {
            IrInput::TvRemoteVolUp => update_volume(volume, VolumeAction::Up),
            IrInput::TvRemoteVolDown => update_volume(volume, VolumeAction::Down),
        };

        if volume_buffer != volume {
            volume = volume_buffer;
            volume_storage.write_volume(volume);

            if volume_buffer > volume {
                for v in volume..=volume_buffer {
                    set_volume_spi(&mut spi, v);
                    delay.delay_millis(2);
                }
            } else {
                for v in volume_buffer..=volume {
                    set_volume_spi(&mut spi, v);
                    delay.delay_millis(2);
                }
            };
        }

        print_volume(volume, &mut lcd);

        delay.delay_millis(100);
    }
}

fn set_volume_spi(spi: &mut Spi<Blocking>, vol: u8) {
    let frame: [u8; 2] = [vol, vol];
    spi.write(&frame).unwrap();
}

fn update_volume(volume: u8, action: VolumeAction) -> u8 {
    let volume = match action {
        VolumeAction::Up => volume.saturating_add(VOLUME_INCREMENT),
        VolumeAction::Down => volume.saturating_sub(VOLUME_INCREMENT),
        VolumeAction::Mute => 0,
        VolumeAction::Set(new_volume) => display_to_volume(new_volume),
    };

    if volume > VOLUME_MAX {
        VOLUME_MAX
    } else {
        volume
    }
}

fn display_to_volume(display_volume: u8) -> u8 {
    ((display_volume as u16).saturating_mul(VOLUME_MAX as u16)).saturating_div(DISPLAY_MAX as u16)
        as u8
}

fn volume_to_display(internal_volume: u8) -> u8 {
    ((internal_volume as u16).saturating_mul(DISPLAY_MAX as u16)).saturating_div(VOLUME_MAX as u16)
        as u8
}

fn print_volume(volume: u8, lcd: &mut Lcd) {
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
