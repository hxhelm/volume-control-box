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
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Blocking, main};
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
    let delay = Delay::new();

    // TODO: set up wifi connection for home-assistant integration
    // let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // let (mut _wifi_controller, _interfaces) =
    //     esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");
    // info!("Wi-Fi controller set up!");

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_khz(100))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO19);

    let mut volume_storage = VolumeStorage::new(peripherals.FLASH);
    let mut volume = volume_storage.read_volume();
    set_volume_spi(&mut spi, volume);

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

        delay.delay_millis(100);
    }
}

fn set_volume_spi(spi: &mut Spi<Blocking>, vol: u8) {
    let frame: [u8; 2] = [vol, vol];
    spi.write(&frame).unwrap();
}
