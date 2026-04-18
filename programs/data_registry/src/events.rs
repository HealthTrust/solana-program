use anchor_lang::prelude::*;

#[event]
pub struct MetaEntryCreated {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub data_type_hashes: Vec<[u8; 32]>,
    pub total_duration: u64,
    pub date_of_creation: i64,
}

#[event]
pub struct MetaAttributes {
    pub meta_id: u64,
    pub age: u8,
    pub gender: u8,
    pub height: u8,
    pub weight: u8,
    pub region: u8,
    pub physical_activity_level: u8,
    pub smoker: u8,
    pub diet: u8,
    pub chronic_conditions: Vec<u8>,
}

#[event]
pub struct MetaDeviceInfo {
    pub meta_id: u64,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
}

#[event]
pub struct UploadUnitCreated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub date_of_creation: i64,
}

#[event]
pub struct DataStored {
    pub meta_id: u64,
    pub unit_index: u32,
    pub owner: Pubkey,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub total_duration: u64,
    pub added_duration: u64,
}

#[event]
pub struct DataEntryVersionUpdated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub feat_cid: String,
    pub updater: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct DataEntryDeleted {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct UploadUnitClosed {
    pub meta_id: u64,
    pub unit_index: u32,
}

#[event]
pub struct PausedStateChanged {
    pub paused: bool,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}

#[event]
pub struct EmergencyWithdrawal {
    pub recipient: Pubkey,
    pub amount: u64,
}
