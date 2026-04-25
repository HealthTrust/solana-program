use anchor_lang::prelude::*;

use crate::errors::OrderError;
use crate::state::{EscrowVault, Job, OrderConfig};

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct RoflAction<'info> {
    #[account(
        seeds = [b"order_config"],
        bump = order_config.bump,
        has_one = rofl_authority @ OrderError::NotRoflAuthority,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    pub rofl_authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct FinalizeJob<'info> {
    #[account(
        seeds = [b"order_config"],
        bump = order_config.bump,
        has_one = rofl_authority @ OrderError::NotRoflAuthority,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump = escrow_vault.bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    pub rofl_authority: Signer<'info>,
}
