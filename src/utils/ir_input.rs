use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralInput;
use esp_hal::peripherals::RMT;
use esp_hal::rmt::{Channel, PulseCode, Rmt, Rx, RxChannelConfig, RxChannelCreator};
use esp_hal::time::Rate;

pub(crate) enum IrInput {
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

pub(crate) struct IrReceiver<'a> {
    channel: Option<Channel<'a, Blocking, Rx>>,
    buffer: [PulseCode; 48],
}

impl<'a> IrReceiver<'a> {
    pub fn new<T: PeripheralInput<'a>>(rmt_peripheral: RMT<'a>, receive_pin: T) -> Self {
        let freq = Rate::from_mhz(80);
        let rmt = Rmt::new(rmt_peripheral, freq)
            .expect("Failed to initialize Remote Control Transceiver instance");
        let rx_config = RxChannelConfig::default()
            .with_clk_divider(80)
            .with_idle_threshold(10_000);

        let channel = rmt
            .channel2
            .configure_rx(receive_pin, rx_config)
            .expect("Failed to initialize RX Channel for RMT");
        let buffer: [PulseCode; 48] = [PulseCode::default(); 48];

        Self {
            channel: Some(channel),
            buffer,
        }
    }

    pub fn get_incoming_signal(&mut self) -> Option<IrInput> {
        for x in self.buffer.iter_mut() {
            x.reset()
        }

        let channel = self.channel.take().unwrap();

        let transaction = channel.receive(&mut self.buffer).unwrap();

        match transaction.wait() {
            Ok((symbol_count, channel_res)) => {
                self.channel = Some(channel_res);

                let mut bits: u32 = 0;
                let mut bit_index = 0;

                for entry in self.buffer[..symbol_count].iter().skip(1) {
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

                IrInput::try_from(bits).ok()
            }
            Err((_err, channel_res)) => {
                self.channel = Some(channel_res);

                None
            }
        }
    }
}
