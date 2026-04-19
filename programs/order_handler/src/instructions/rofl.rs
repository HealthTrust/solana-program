use anchor_lang::prelude::*;

use crate::constants::MAX_PARTICIPANTS;
use crate::contexts::{FinalizeJob, RoflAction};
use crate::errors::OrderError;
use crate::events::{JobCompleted, JobExecuted, PreflightSubmitted};
use crate::params::PreflightResultParams;
use crate::state::JobStatus;

pub fn submit_preflight_result(
    ctx: Context<RoflAction>,
    job_id: u64,
    preflight: PreflightResultParams,
) -> Result<()> {
    require!(
        preflight.selected_participants.len() <= MAX_PARTICIPANTS,
        OrderError::TooManyParticipants
    );

    let job = &mut ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_eq!(
        job.status,
        JobStatus::PendingPreflight,
        OrderError::InvalidStatus
    );

    job.effective_participants_scaled = preflight.effective_participants_scaled;
    job.quality_tier = preflight.quality_tier;
    job.final_total = preflight.final_total;
    job.preflight_timestamp = Clock::get()?.unix_timestamp;
    job.cohort_hash = preflight.cohort_hash;
    job.selected_participants = preflight.selected_participants;
    job.status = JobStatus::AwaitingConfirmation;
    job.updated_at = Clock::get()?.unix_timestamp;

    emit!(PreflightSubmitted {
        job_id,
        effective_participants_scaled: job.effective_participants_scaled,
        quality_tier: job.quality_tier,
        final_total: job.final_total,
        cohort_hash: job.cohort_hash,
    });
    Ok(())
}

pub fn submit_result(
    ctx: Context<RoflAction>,
    job_id: u64,
    result_cid: String,
    output_hash: [u8; 32],
) -> Result<()> {
    let job = &mut ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_eq!(job.status, JobStatus::Confirmed, OrderError::InvalidStatus);
    require!(!result_cid.is_empty(), OrderError::EmptyResultCid);

    let clock = Clock::get()?;
    job.result_cid = result_cid.clone();
    job.execution_timestamp = clock.unix_timestamp;
    job.output_hash = output_hash;
    job.status = JobStatus::Executed;
    job.updated_at = clock.unix_timestamp;

    emit!(JobExecuted { job_id, result_cid });
    Ok(())
}

pub fn finalize_job(ctx: Context<FinalizeJob>, job_id: u64) -> Result<()> {
    let job = &mut ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_eq!(job.status, JobStatus::Executed, OrderError::InvalidStatus);

    let num_providers = job.selected_participants.len() as u64;
    require!(num_providers > 0, OrderError::NoParticipants);

    let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
    let rent = Rent::get()?;
    let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
    let vault_rent_exempt = rent.minimum_balance(vault_data_len);
    let distributable = vault_lamports
        .checked_sub(vault_rent_exempt)
        .ok_or(OrderError::InsufficientEscrow)?;

    job.amount_per_provider = distributable / num_providers;
    require!(
        job.amount_per_provider > 0,
        OrderError::ZeroAmountPerProvider
    );

    job.status = JobStatus::Completed;
    job.updated_at = Clock::get()?.unix_timestamp;

    emit!(JobCompleted {
        job_id,
        amount_per_provider: job.amount_per_provider,
        num_providers,
    });
    Ok(())
}
