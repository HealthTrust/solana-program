use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum JobStatus {
    None,
    PendingPreflight,
    AwaitingConfirmation,
    Confirmed,
    Executed,
    Completed,
    Cancelled,
}

impl core::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let status = match self {
            JobStatus::None => "None",
            JobStatus::PendingPreflight => "PendingPreflight",
            JobStatus::AwaitingConfirmation => "AwaitingConfirmation",
            JobStatus::Confirmed => "Confirmed",
            JobStatus::Executed => "Executed",
            JobStatus::Completed => "Completed",
            JobStatus::Cancelled => "Cancelled",
        };
        write!(f, "{}", status)
    }
}

#[account]
#[derive(InitSpace)]
pub struct OrderConfig {
    pub owner: Pubkey,
    pub rofl_authority: Pubkey,
    pub next_job_id: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Job {
    pub job_id: u64,
    pub researcher: Pubkey,
    pub status: JobStatus,
    pub template_id: u32,
    pub num_days: u32,
    #[max_len(8, 32)]
    pub data_types: Vec<String>,
    pub max_participants: u32,
    pub start_day_utc: i64,
    #[max_len(128)]
    pub filter_query: String,
    pub escrowed: u64,
    pub effective_participants_scaled: u64,
    pub quality_tier: u8,
    pub final_total: u64,
    pub preflight_timestamp: i64,
    pub cohort_hash: [u8; 32],
    #[max_len(50)]
    pub selected_participants: Vec<Pubkey>,
    #[max_len(64)]
    pub result_cid: String,
    pub execution_timestamp: i64,
    pub output_hash: [u8; 32],
    pub amount_per_provider: u64,
    pub claimed_bitmap: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct EscrowVault {
    pub job_id: u64,
    pub bump: u8,
}
