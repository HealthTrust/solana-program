use anchor_lang::prelude::*;

use crate::state::{EscrowVault, Job, OrderConfig};

#[derive(Accounts)]
pub struct RequestJob<'info> {
    #[account(
        mut,
        seeds = [b"order_config"],
        bump = order_config.bump,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        init,
        payer = researcher,
        space = 8 + Job::INIT_SPACE,
        seeds = [b"job", order_config.next_job_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub job: Account<'info, Job>,

    #[account(mut)]
    pub researcher: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct ConfirmJobAndPay<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        init,
        payer = researcher,
        space = 8 + EscrowVault::INIT_SPACE,
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut)]
    pub researcher: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct CancelJob<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
        close = researcher,
    )]
    pub job: Account<'info, Job>,

    #[account(mut)]
    pub researcher: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct SweepVaultDust<'info> {
    #[account(
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

    /// CHECK: recipient is validated in instruction logic.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}
