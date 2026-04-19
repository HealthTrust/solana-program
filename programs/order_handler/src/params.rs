use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct JobParams {
    pub template_id: u32,
    pub num_days: u32,
    pub data_type_hashes: Vec<[u8; 32]>,
    pub max_participants: u32,
    pub start_day_utc: i64,
    pub filter_query: String,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PreflightResultParams {
    pub effective_participants_scaled: u64,
    pub quality_tier: u8,
    pub final_total: u64,
    pub cohort_hash: [u8; 32],
    pub selected_participants: Vec<Pubkey>,
}
