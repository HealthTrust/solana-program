use anchor_lang::prelude::*;

#[event]
pub struct JobRequested {
    pub job_id: u64,
    pub researcher: Pubkey,
    pub template_id: u32,
    pub num_days: u32,
    pub data_types: Vec<String>,
    pub max_participants: u32,
}

#[event]
pub struct PreflightSubmitted {
    pub job_id: u64,
    pub effective_participants_scaled: u64,
    pub quality_tier: u8,
    pub final_total: u64,
    pub cohort_hash: [u8; 32],
}

#[event]
pub struct JobConfirmed {
    pub job_id: u64,
    pub amount: u64,
}

#[event]
pub struct JobCancelled {
    pub job_id: u64,
    pub refund_amount: u64,
}

#[event]
pub struct JobExecuted {
    pub job_id: u64,
    pub result_cid: String,
}

#[event]
pub struct JobCompleted {
    pub job_id: u64,
    pub amount_per_provider: u64,
    pub num_providers: u64,
}

#[event]
pub struct DataProviderPaid {
    pub job_id: u64,
    pub provider: Pubkey,
    pub amount: u64,
}

#[event]
pub struct VaultDustSwept {
    pub job_id: u64,
    pub recipient: Pubkey,
    pub amount: u64,
}

#[event]
pub struct RoflAuthorityUpdated {
    pub rofl_authority: Pubkey,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}
