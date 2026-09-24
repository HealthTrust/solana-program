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

    /// Salted commitment to the provider's OFF-chain attribute profile version
    /// that was current at upload time: `sha256(salt || canonical_profile)`.
    /// Personal attributes (age, gender, height, weight, region, activity,
    /// smoker, diet, chronic conditions) are Art. 9 health data and are never
    /// written on-chain; they live in the backend `provider_profile` and are
    /// erasable. The commitment lets a study verify the served profile matches
    /// what was attested, while an erased profile (salt deleted) leaves these
    /// 32 bytes unrecoverable. See MVP/OFFCHAIN_ATTRIBUTES_DESIGN.md.
    pub profile_commit: [u8; 32],

    #[max_len(18, 32)]
    pub data_types: Vec<String>,

    pub total_duration: u64,
    /// Total units ever appended — drives the next unit PDA seed. Never
    /// decremented, so closed unit indexes can never be re-created.
    pub unit_count: u32,
    /// Units currently open. Decremented by close_upload_unit and the
    /// close_data_entry_meta cascade; the meta can only close at zero, so a
    /// withdrawal can span many transactions without orphaning units.
    pub open_unit_count: u32,
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
