use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

const FLASH_ADDRESS: u32 = 0x9000;

pub(crate) struct VolumeStorage<'a> {
    bytes: [u8; 1],
    storage: FlashStorage<'a>,
}

impl<'a> VolumeStorage<'a> {
    pub fn new(flash: FLASH<'a>) -> Self {
        VolumeStorage {
            bytes: [0u8; 1],
            storage: FlashStorage::new(flash),
        }
    }

    pub fn read_volume(&mut self) -> u8 {
        self.storage
            .read(FLASH_ADDRESS, &mut self.bytes)
            .expect("Failed to read volume from flash storage.");

        self.bytes[0]
    }

    pub fn write_volume(&mut self, volume: u8) {
        self.bytes[0] = volume;

        self.storage
            .write(FLASH_ADDRESS, &self.bytes)
            .expect("Failed to write volume to flash storage.");
    }
}
