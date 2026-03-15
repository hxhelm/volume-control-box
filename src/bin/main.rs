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
use esp_hal::main;
use esp_hal::rmt::{PulseCode, Rmt, RxChannelConfig, RxChannelCreator};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use log::info;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

enum IrInput {
    TvRemoteVolUp,
    TvRemoteVolDown,
}

impl TryFrom<u32> for IrInput {
    type Error = &'static str;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0xF8070707 => Ok(IrInput::TvRemoteVolUp),
            0xF40B0707 => Ok(IrInput::TvRemoteVolDown),
            _ => Err("Unmapped value for IrInput."),
        }
    }
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

    // TODO: set up wifi connection for home-assistant integration
    // let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // let (mut _wifi_controller, _interfaces) =
    //     esp_radio::wifi::new(&radio_init, peripherals.WIFI, Default::default())
    //         .expect("Failed to initialize Wi-Fi controller");

    // info!("Wi-Fi controller set up!");

    // Configure frequency based on chip type
    let freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, freq)
        .expect("Failed to initialize Remote Control Transceiver instance");

    let rx_config = RxChannelConfig::default()
        .with_clk_divider(80)
        .with_idle_threshold(10_000);
    let mut channel = rmt
        .channel2
        .configure_rx(peripherals.GPIO4, rx_config)
        .expect("Failed to initialize RX Channel for RMT");
    let delay = Delay::new();
    let mut data: [PulseCode; 48] = [PulseCode::default(); 48];

    info!("RMT RX set up, beginning read loop...");

    loop {
        for x in data.iter_mut() {
            x.reset()
        }

        let transaction = channel.receive(&mut data).unwrap();

        match transaction.wait() {
            Ok((symbol_count, channel_res)) => {
                channel = channel_res;

                let mut bits: u32 = 0;
                let mut bit_index = 0;

                for entry in data[..symbol_count].iter().skip(1) {
                    let low = entry.length1();
                    let high = entry.length2();

                    if low == 0 || high == 0 {
                        break;
                    }

                    // Expect ~560µs LOW
                    if !(400..=700).contains(&low) {
                        continue;
                    }

                    // Determine bit from HIGH duration
                    if high > 1000 {
                        bits |= 1 << bit_index;
                    }

                    bit_index += 1;

                    if bit_index >= 32 {
                        break;
                    }
                }

                info!("Decoded bits: 0x{:08X}", bits);

                let Ok(ir_input) = IrInput::try_from(bits) else {
                    continue;
                };
            }
            Err((err, channel_res)) => {
                channel = channel_res;
                info!("RX error: {:?}", err);
            }
        }

        delay.delay_millis(100);
    }
}
