use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    pub boot_size: u32,
    pub partition_layout: PartitionLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionLayout {
    #[serde(rename = "mbr")]
    MBR,
    #[serde(rename = "gpt")]
    GPT,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConfig {
    pub image_path: String,
    pub target_disk: String,
    pub partition_config: PartitionConfig,
    pub fast_write: bool,
}
