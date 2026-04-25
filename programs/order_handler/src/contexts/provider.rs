use anchor_lang::prelude::*;

use crate::state::{EscrowVault, Job};

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct ClaimPayout<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        mut,
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump = escrow_vault.bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut)]
    pub provider: Signer<'info>,
}
