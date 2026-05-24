use embedded_storage::{ReadStorage, Storage};
use esp_bootloader_esp_idf::partitions;
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

pub struct VolumeStorage<'a> {
    bytes: [u8; 1],
    storage: FlashStorage<'a>,
    nvs_offset: u32,
}

impl<'a> VolumeStorage<'a> {
    pub fn new(flash: FLASH<'a>) -> Self {
        let mut flash_storage = FlashStorage::new(flash);
        let mut pt_mem = [0u8; partitions::PARTITION_TABLE_MAX_LEN];

        let pt = partitions::read_partition_table(&mut flash_storage, &mut pt_mem).unwrap();

        let nvs = pt
            .find_partition(partitions::PartitionType::Data(
                partitions::DataPartitionSubType::Nvs,
            ))
            .unwrap()
            .unwrap();

        VolumeStorage {
            bytes: [0u8; 1],
            storage: flash_storage,
            nvs_offset: nvs.offset(),
        }
    }

    pub fn read_volume(&mut self) -> u8 {
        self.storage
            .read(self.nvs_offset, &mut self.bytes)
            .expect("Failed to read volume from flash storage.");

        self.bytes[0]
    }

    pub fn write_volume(&mut self, volume: u8) {
        self.bytes[0] = volume;

        self.storage
            .write(self.nvs_offset, &self.bytes)
            .expect("Failed to write volume to flash storage.");
    }
}
