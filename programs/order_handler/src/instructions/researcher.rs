use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::{MAX_JOB_DATA_TYPE_LEN, MAX_JOB_DATA_TYPES};
use crate::contexts::{CancelJob, ConfirmJobAndPay, RequestJob, SweepVaultDust};
use crate::errors::OrderError;
use crate::events::{JobCancelled, JobConfirmed, JobRequested, VaultDustSwept};
use crate::params::JobParams;
use crate::state::JobStatus;

pub fn request_job(ctx: Context<RequestJob>, params: JobParams) -> Result<()> {
    require!(params.num_days > 0, OrderError::InvalidNumDays);
    require!(params.template_id > 0, OrderError::InvalidTemplateId);
    require!(!params.data_types.is_empty(), OrderError::EmptyDataTypes);
    require!(
        params.max_participants > 0,
        OrderError::InvalidMaxParticipants
    );
    require!(
        params.data_types.len() <= MAX_JOB_DATA_TYPES,
        OrderError::TooManyDataTypes
    );
    require!(
        params
            .data_types
            .iter()
            .all(|data_type| !data_type.is_empty() && data_type.len() <= MAX_JOB_DATA_TYPE_LEN),
        OrderError::TooManyDataTypes
    );

    let order_config = &mut ctx.accounts.order_config;
    let job_id = order_config.next_job_id;
    order_config.next_job_id = order_config
        .next_job_id
        .checked_add(1)
        .ok_or(OrderError::Overflow)?;

    let clock = Clock::get()?;
    let job = &mut ctx.accounts.job;
    job.job_id = job_id;
    job.researcher = ctx.accounts.researcher.key();
    job.status = JobStatus::PendingPreflight;
    job.template_id = params.template_id;
    job.num_days = params.num_days;
    job.data_types = params.data_types.clone();
    job.max_participants = params.max_participants;
    job.start_day_utc = params.start_day_utc;
    job.filter_query = params.filter_query.clone();
    job.escrowed = 0;
    job.effective_participants_scaled = 0;
    job.quality_tier = 0;
    job.final_total = 0;
    job.preflight_timestamp = 0;
    job.cohort_hash = [0u8; 32];
    job.selected_participants = Vec::new();
    job.result_cid = String::new();
    job.execution_timestamp = 0;
    job.output_hash = [0u8; 32];
    job.amount_per_provider = 0;
    job.claimed_bitmap = 0;
    job.created_at = clock.unix_timestamp;
    job.updated_at = clock.unix_timestamp;
    job.bump = ctx.bumps.job;

    emit!(JobRequested {
        job_id,
        researcher: job.researcher,
        template_id: params.template_id,
        num_days: params.num_days,
        data_types: params.data_types,
        max_participants: params.max_participants,
    });
    Ok(())
}

pub fn confirm_job_and_pay(
    ctx: Context<ConfirmJobAndPay>,
    job_id: u64,
    payment_amount: u64,
) -> Result<()> {
    let job = &mut ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_keys_eq!(
        ctx.accounts.researcher.key(),
        job.researcher,
        OrderError::Unauthorized
    );
    require_eq!(
        job.status,
        JobStatus::AwaitingConfirmation,
        OrderError::InvalidStatus
    );
    require!(
        payment_amount >= job.final_total,
        OrderError::InsufficientPayment
    );
    require!(job.final_total > 0, OrderError::ZeroFinalTotal);

    let vault = &mut ctx.accounts.escrow_vault;
    vault.job_id = job_id;
    vault.bump = ctx.bumps.escrow_vault;

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.researcher.to_account_info(),
                to: ctx.accounts.escrow_vault.to_account_info(),
            },
        ),
        payment_amount,
    )?;

    job.escrowed = payment_amount;
    job.status = JobStatus::Confirmed;
    job.updated_at = Clock::get()?.unix_timestamp;

    emit!(JobConfirmed {
        job_id,
        amount: payment_amount,
    });
    Ok(())
}

pub fn cancel_job(ctx: Context<CancelJob>, job_id: u64) -> Result<()> {
    let job = &ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_keys_eq!(
        ctx.accounts.researcher.key(),
        job.researcher,
        OrderError::Unauthorized
    );
    require!(
        job.status == JobStatus::PendingPreflight || job.status == JobStatus::AwaitingConfirmation,
        OrderError::CannotCancelAtThisStage
    );

    emit!(JobCancelled {
        job_id,
        refund_amount: 0,
    });
    Ok(())
}

pub fn sweep_vault_dust(ctx: Context<SweepVaultDust>, job_id: u64) -> Result<()> {
    let job = &ctx.accounts.job;
    require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
    require_eq!(job.status, JobStatus::Completed, OrderError::InvalidStatus);

    let all_claimed = job.selected_participants.len() == job.claimed_bitmap.count_ones() as usize;
    let is_researcher = ctx.accounts.recipient.key() == job.researcher;
    require!(all_claimed || is_researcher, OrderError::SweepNotAllowed);

    let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
    let rent = Rent::get()?;
    let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
    let vault_rent_exempt = rent.minimum_balance(vault_data_len);
    let dust = vault_lamports.checked_sub(vault_rent_exempt).unwrap_or(0);

    if dust > 0 {
        ctx.accounts.escrow_vault.sub_lamports(dust)?;
        ctx.accounts.recipient.add_lamports(dust)?;
    }

    emit!(VaultDustSwept {
        job_id,
        recipient: ctx.accounts.recipient.key(),
        amount: dust,
    });
    Ok(())
}
