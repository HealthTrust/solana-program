use anchor_lang::prelude::*;

use crate::contexts::ClaimPayout;
use crate::errors::OrderError;
use crate::events::DataProviderPaid;
use crate::state::JobStatus;

pub fn claim_payout(ctx: Context<ClaimPayout>, job_id: u64) -> Result<()> {
    let provider_key = ctx.accounts.provider.key();

    let (amount, participant_index) = {
        let job = &ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_eq!(job.status, JobStatus::Completed, OrderError::InvalidStatus);
        require!(
            job.amount_per_provider > 0,
            OrderError::ZeroAmountPerProvider
        );

        let index = job
            .selected_participants
            .iter()
            .position(|participant| participant == &provider_key)
            .ok_or(OrderError::NotAParticipant)?;

        require!(index < 64, OrderError::ParticipantIndexOutOfRange);
        require!(
            job.claimed_bitmap & (1u64 << index) == 0,
            OrderError::AlreadyClaimed
        );

        (job.amount_per_provider, index)
    };

    let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
    let rent = Rent::get()?;
    let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
    let vault_min = rent.minimum_balance(vault_data_len);
    require!(
        vault_lamports.checked_sub(amount).unwrap_or(0) >= vault_min,
        OrderError::InsufficientEscrow
    );

    ctx.accounts.escrow_vault.sub_lamports(amount)?;
    ctx.accounts.provider.add_lamports(amount)?;
    ctx.accounts.job.claimed_bitmap |= 1u64 << participant_index;

    emit!(DataProviderPaid {
        job_id,
        provider: provider_key,
        amount,
    });
    Ok(())
}
