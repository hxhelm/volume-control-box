#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

pub(crate) use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Pull};
use esp_hal::main;
use esp_hal::timer::timg::TimerGroup;
use esp_println::{print, println};
use log::info;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

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

    let clock = Input::new(
        peripherals.GPIO18,
        InputConfig::default().with_pull(Pull::None),
    );
    let data = Input::new(
        peripherals.GPIO19,
        InputConfig::default().with_pull(Pull::None),
    );

    let mut last_clock = clock.level();

    let mut bits: [u8; 128] = [0; 128];
    let mut bit_count = 0usize;

    let mut idle_counter = 0u32;

    info!("Encoder sniffer started...");

    loop {
        let now_clock = clock.level();

        if now_clock == Level::High && last_clock == Level::Low {
            let bit = match data.level() {
                Level::High => 1,
                Level::Low => 0,
            };

            if bit_count < bits.len() {
                bits[bit_count] = bit;
                bit_count += 1;
            }

            idle_counter = 0;
        }

        last_clock = now_clock;

        if bit_count == 0 {
            continue;
        }

        idle_counter += 1;

        if idle_counter > 50_000 {
            println!("--- Frame ({} bits) ---", bit_count);

            for i in bits.iter().take(bit_count) {
                print!("{}", i);

                if (i + 1) % 8 == 0 {
                    print!(" ");
                }
            }

            println!();

            if bit_count >= 8 {
                print!("HEX: ");
                let mut i = 0;
                while i + 7 < bit_count {
                    let mut value = 0u8;
                    for j in 0..8 {
                        value |= bits[i + j] << (7 - j);
                    }
                    print!("{:02X} ", value);
                    i += 8;
                }
                println!();
            }

            println!("-----------------------");

            bit_count = 0;
            idle_counter = 0;
        }
    }
}
