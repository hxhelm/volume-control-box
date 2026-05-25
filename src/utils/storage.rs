use embassy_embedded_hal::adapter::BlockingAsync;
use embedded_storage::ReadStorage as _;
use esp_bootloader_esp_idf::partitions;
use esp_hal::peripherals::FLASH;
use esp_println::println;
use esp_storage::FlashStorage;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{Key, MapConfig, MapStorage, SerializationError, Value};
use serde::{Deserialize, Serialize};

pub struct ConfigStorage<'a> {
    storage: MapStorage<ConfigKey, BlockingAsync<FlashStorage<'a>>, NoCache>,
}

impl<'a> ConfigStorage<'a> {
    pub fn new(flash: FLASH<'a>) -> Self {
        println!("[ConfigStorage] Initializing");

        let mut flash_storage = FlashStorage::new(flash);

        println!(
            "[ConfigStorage] Flash capacity: {} bytes",
            flash_storage.capacity()
        );

        let mut pt_mem = [0u8; partitions::PARTITION_TABLE_MAX_LEN];

        let pt = partitions::read_partition_table(&mut flash_storage, &mut pt_mem)
            .expect("Failed reading partition table");

        println!("[ConfigStorage] Partition table loaded");

        for partition in pt.iter() {
            println!(
                "[Partition] label='{}' offset=0x{:X} size=0x{:X}",
                partition.label_as_str(),
                partition.offset(),
                partition.len(),
            );
        }

        // get partition by label defined in `partisions.csv`
        let config_partition = pt
            .iter()
            .find(|p| p.label_as_str() == "config")
            .expect("Failed to resolve config partition");

        println!(
            "[ConfigStorage] Found config partition: offset=0x{:X}, size={} bytes",
            config_partition.offset(),
            config_partition.len(),
        );

        let flash_storage = embassy_embedded_hal::adapter::BlockingAsync::new(flash_storage);
        let range = config_partition.offset()..(config_partition.offset() + config_partition.len());

        println!(
            "[ConfigStorage] Config range: 0x{:X}..0x{:X}",
            range.start, range.end
        );

        let map_storage = MapStorage::new(flash_storage, MapConfig::new(range), NoCache::new());

        Self {
            storage: map_storage,
        }
    }

    pub async fn write_config(&mut self, config: &AppConfig, buffer: &mut [u8; 128]) {
        println!("[ConfigStorage] Writing config: volume={}", config.volume);

        match self.storage.store_item(buffer, &ConfigKey, config).await {
            Ok(_) => {
                println!("[ConfigStorage] Config write successful");
            }
            Err(e) => {
                println!("[ConfigStorage] Config write FAILED: {:?}", e);
            }
        };
    }

    pub async fn read_config(&mut self, buffer: &mut [u8; 128]) -> Option<AppConfig> {
        println!("[ConfigStorage] Reading config");

        match self
            .storage
            .fetch_item::<AppConfig>(buffer, &ConfigKey)
            .await
        {
            Ok(Some(config)) => {
                println!("[ConfigStorage] Loaded config: volume={}", config.volume);

                Some(config)
            }

            Ok(None) => {
                println!("[ConfigStorage] No config stored yet");

                None
            }

            Err(e) => {
                println!("[ConfigStorage] Config read FAILED: {:?}", e);

                None
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKey;

impl Key for ConfigKey {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        let bytes = b"CONFIG";
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        if buffer.starts_with(b"CONFIG") {
            Ok((Self, b"CONFIG".len()))
        } else {
            Err(SerializationError::InvalidData)
        }
    }

    fn get_len(_: &[u8]) -> Result<usize, SerializationError> {
        Ok(b"CONFIG".len())
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppConfig {
    pub volume: u8,
}

impl<'a> Value<'a> for AppConfig {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        postcard::to_slice(&self, buffer)
            .map_err(|_| SerializationError::InvalidFormat)
            .map(|s| s.len())
    }

    fn deserialize_from(buffer: &'a [u8]) -> Result<(Self, usize), SerializationError>
    where
        Self: Sized,
    {
        let config: Self =
            postcard::from_bytes(buffer).map_err(|_| SerializationError::InvalidFormat)?;

        Ok((config, buffer.len()))
    }
}
