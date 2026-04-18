use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct RegistryState {
    pub owner: Pubkey,
    pub pricing_program: Pubkey,
    pub tee_authority: Pubkey,
    pub next_meta_id: u64,
    pub paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct DataEntryMeta {
    pub meta_id: u64,
    pub owner: Pubkey,

    #[max_len(32)]
    pub device_type: String,
    #[max_len(48)]
    pub device_model: String,
    #[max_len(48)]
    pub service_provider: String,

    pub age: u8,
    pub gender: u8,
    pub height: u8,
    pub weight: u8,
    pub region: u8,
    pub physical_activity_level: u8,
    pub smoker: u8,
    pub diet: u8,

    #[max_len(16)]
    pub chronic_conditions: Vec<u8>,

    #[max_len(8)]
    pub data_type_hashes: Vec<[u8; 32]>,

    pub total_duration: u64,
    pub unit_count: u32,
    pub date_of_creation: i64,
    pub date_of_modification: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UploadUnit {
    pub meta_id: u64,
    pub unit_index: u32,

    #[max_len(64)]
    pub raw_cid: String,

    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,

    #[max_len(64)]
    pub feat_cid: String,

    pub date_of_creation: i64,
    pub bump: u8,
}
