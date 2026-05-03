#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::main;
use esp_hal::timer::timg::TimerGroup;
use utils::ir_input::{IrInput, IrReceiver};
use utils::storage::VolumeStorage;

mod utils;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const VOLUME_INCREMENT: u8 = 4;

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
    let mut delay = Delay::new();

    // TODO: set up wifi connection for home-assistant integration
    // let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // let (mut _wifi_controller, _interfaces) =
    //     esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");
    // info!("Wi-Fi controller set up!");

    // communication with volume board
    let mut clock = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    let mut data = Output::new(peripherals.GPIO19, Level::Low, OutputConfig::default());

    let mut volume_storage = VolumeStorage::new(peripherals.FLASH);
    let mut volume = volume_storage.read_volume();
    set_volume(volume, &mut clock, &mut data, &mut delay);

    let mut ir_receiver = IrReceiver::new(peripherals.RMT, peripherals.GPIO4);

    loop {
        let Some(ir_input) = ir_receiver.get_incoming_signal() else {
            delay.delay_millis(100);
            continue;
        };

        let volume_buffer = match ir_input {
            IrInput::TvRemoteVolUp => volume.saturating_add(VOLUME_INCREMENT),
            IrInput::TvRemoteVolDown => volume.saturating_sub(VOLUME_INCREMENT),
        };

        if volume_buffer != volume {
            volume = volume_buffer;
            volume_storage.write_volume(volume);

            if volume_buffer > volume {
                for v in volume..=volume_buffer {
                    set_volume(v, &mut clock, &mut data, &mut delay);
                    delay.delay_millis(5);
                }
            } else {
                for v in volume_buffer..=volume {
                    set_volume(v, &mut clock, &mut data, &mut delay);
                    delay.delay_millis(5);
                }
            };
        }

        delay.delay_millis(100);
    }
}

fn set_volume(vol: u8, clock: &mut Output<'_>, data: &mut Output<'_>, delay: &mut Delay) {
    send_byte(vol, clock, data, delay);
    send_byte(vol, clock, data, delay);
}

fn send_byte(mut value: u8, clock: &mut Output<'_>, data: &mut Output<'_>, delay: &mut Delay) {
    for _ in 0..8 {
        let bit = (value & 0x80) != 0;

        if bit {
            data.set_high();
        } else {
            data.set_low();
        }

        delay.delay_micros(1);

        clock.set_high();
        delay.delay_micros(1);

        data.set_low();
        delay.delay_micros(1);

        clock.set_low();
        delay.delay_micros(1);

        value <<= 1;
    }
}
