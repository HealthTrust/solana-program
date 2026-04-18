use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadNewMetaParams {
    pub raw_cid: String,
    pub data_type_hashes: Vec<[u8; 32]>,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
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
