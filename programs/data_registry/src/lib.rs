use anchor_lang::prelude::*;

pub mod constants;
pub mod contexts;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod params;
pub mod state;

use contexts::*;
use params::UploadNewMetaParams;

declare_id!("EitDizrAP7BH192FP4GivCWZrdbUjVoWJRUfQRRHbGq3");

#[program]
pub mod data_registry {
    use super::*;

    pub fn initialize_registry(
        ctx: Context<InitializeRegistry>,
        tee_authority: Pubkey,
    ) -> Result<()> {
        instructions::initialize_registry(ctx, tee_authority)
    }

    pub fn set_pricing_program(
        ctx: Context<AuthorizedRegistryAction>,
        pricing_program: Pubkey,
    ) -> Result<()> {
        instructions::set_pricing_program(ctx, pricing_program)
    }

    pub fn set_tee_authority(
        ctx: Context<AuthorizedRegistryAction>,
        tee_authority: Pubkey,
    ) -> Result<()> {
        instructions::set_tee_authority(ctx, tee_authority)
    }

    pub fn set_paused(ctx: Context<AuthorizedRegistryAction>, paused: bool) -> Result<()> {
        instructions::set_paused(ctx, paused)
    }

    pub fn transfer_registry_ownership(
        ctx: Context<AuthorizedRegistryAction>,
        new_owner: Pubkey,
    ) -> Result<()> {
        instructions::transfer_registry_ownership(ctx, new_owner)
    }

    pub fn emergency_withdraw_sol(ctx: Context<EmergencyWithdrawSol>, amount: u64) -> Result<()> {
        instructions::emergency_withdraw_sol(ctx, amount)
    }

    pub fn upload_new_meta(ctx: Context<UploadNewMeta>, params: UploadNewMetaParams) -> Result<()> {
        instructions::upload_new_meta(ctx, params)
    }

    pub fn register_raw_upload(
        ctx: Context<RegisterRawUpload>,
        meta_id: u64,
        raw_cid: String,
        day_start_timestamp: i64,
        day_end_timestamp: i64,
    ) -> Result<()> {
        instructions::register_raw_upload(
            ctx,
            meta_id,
            raw_cid,
            day_start_timestamp,
            day_end_timestamp,
        )
    }

    pub fn update_upload_unit(
        ctx: Context<UpdateUploadUnit>,
        meta_id: u64,
        unit_index: u32,
        feat_cid: String,
    ) -> Result<()> {
        instructions::update_upload_unit(ctx, meta_id, unit_index, feat_cid)
    }

    pub fn close_data_entry_meta(ctx: Context<CloseDataEntryMeta>, meta_id: u64) -> Result<()> {
        instructions::close_data_entry_meta(ctx, meta_id)
    }

    pub fn close_upload_unit(
        ctx: Context<CloseUploadUnit>,
        meta_id: u64,
        unit_index: u32,
    ) -> Result<()> {
        instructions::close_upload_unit(ctx, meta_id, unit_index)
    }
}
