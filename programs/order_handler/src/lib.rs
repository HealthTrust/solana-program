use anchor_lang::prelude::*;

pub mod constants;
pub mod contexts;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod params;
pub mod state;

use contexts::*;
use params::{JobParams, PreflightResultParams};

declare_id!("BmcLrSxve59aReXFqCfZ6gWSfzmZyDPVLAKNX8sAmY9F");

#[program]
pub mod order_handler {
    use super::*;

    pub fn initialize_order_config(
        ctx: Context<InitializeOrderConfig>,
        rofl_authority: Pubkey,
    ) -> Result<()> {
        instructions::initialize_order_config(ctx, rofl_authority)
    }

    pub fn set_rofl_authority(
        ctx: Context<AuthorizedOrderAction>,
        rofl_authority: Pubkey,
    ) -> Result<()> {
        instructions::set_rofl_authority(ctx, rofl_authority)
    }

    pub fn transfer_order_ownership(
        ctx: Context<AuthorizedOrderAction>,
        new_owner: Pubkey,
    ) -> Result<()> {
        instructions::transfer_order_ownership(ctx, new_owner)
    }

    pub fn request_job(ctx: Context<RequestJob>, params: JobParams) -> Result<()> {
        instructions::request_job(ctx, params)
    }

    pub fn submit_preflight_result(
        ctx: Context<RoflAction>,
        job_id: u64,
        preflight: PreflightResultParams,
    ) -> Result<()> {
        instructions::submit_preflight_result(ctx, job_id, preflight)
    }

    pub fn confirm_job_and_pay(
        ctx: Context<ConfirmJobAndPay>,
        job_id: u64,
        payment_amount: u64,
    ) -> Result<()> {
        instructions::confirm_job_and_pay(ctx, job_id, payment_amount)
    }

    pub fn cancel_job(ctx: Context<CancelJob>, job_id: u64) -> Result<()> {
        instructions::cancel_job(ctx, job_id)
    }

    pub fn submit_result(
        ctx: Context<RoflAction>,
        job_id: u64,
        result_cid: String,
        output_hash: [u8; 32],
    ) -> Result<()> {
        instructions::submit_result(ctx, job_id, result_cid, output_hash)
    }

    pub fn finalize_job(ctx: Context<FinalizeJob>, job_id: u64) -> Result<()> {
        instructions::finalize_job(ctx, job_id)
    }

    pub fn claim_payout(ctx: Context<ClaimPayout>, job_id: u64) -> Result<()> {
        instructions::claim_payout(ctx, job_id)
    }

    pub fn sweep_vault_dust(ctx: Context<SweepVaultDust>, job_id: u64) -> Result<()> {
        instructions::sweep_vault_dust(ctx, job_id)
    }
}
