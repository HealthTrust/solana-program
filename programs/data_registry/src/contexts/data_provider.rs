use anchor_lang::prelude::*;

use crate::errors::RegistryError;
use crate::state::{DataEntryMeta, RegistryState, UploadUnit};

#[derive(Accounts)]
pub struct UploadNewMeta<'info> {
    #[account(
        mut,
        seeds = [b"registry_state"],
        bump = registry_state.bump,
    )]
    pub registry_state: Account<'info, RegistryState>,

    #[account(
        init,
        payer = provider,
        space = 8 + DataEntryMeta::INIT_SPACE,
        seeds = [b"meta", registry_state.next_meta_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub data_entry_meta: Account<'info, DataEntryMeta>,

    #[account(
        init,
        payer = provider,
        space = 8 + UploadUnit::INIT_SPACE,
        seeds = [
            b"unit",
            registry_state.next_meta_id.to_le_bytes().as_ref(),
            0u32.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub upload_unit: Account<'info, UploadUnit>,

    #[account(mut)]
    pub provider: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(meta_id: u64)]
pub struct RegisterRawUpload<'info> {
    #[account(
        seeds = [b"registry_state"],
        bump = registry_state.bump,
    )]
    pub registry_state: Account<'info, RegistryState>,

    #[account(
        mut,
        seeds = [b"meta", meta_id.to_le_bytes().as_ref()],
        bump = data_entry_meta.bump,
        constraint = data_entry_meta.owner == provider.key() @ RegistryError::NotOwner,
    )]
    pub data_entry_meta: Account<'info, DataEntryMeta>,

    #[account(
        init,
        payer = provider,
        space = 8 + UploadUnit::INIT_SPACE,
        seeds = [
            b"unit",
            meta_id.to_le_bytes().as_ref(),
            data_entry_meta.unit_count.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub upload_unit: Account<'info, UploadUnit>,

    #[account(mut)]
    pub provider: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(meta_id: u64, unit_index: u32)]
pub struct UpdateUploadUnit<'info> {
    #[account(
        seeds = [b"registry_state"],
        bump = registry_state.bump,
        constraint = registry_state.tee_authority == tee_authority.key() @ RegistryError::NotTeeAuthority,
    )]
    pub registry_state: Account<'info, RegistryState>,

    #[account(
        mut,
        seeds = [b"unit", meta_id.to_le_bytes().as_ref(), unit_index.to_le_bytes().as_ref()],
        bump = upload_unit.bump,
    )]
    pub upload_unit: Account<'info, UploadUnit>,

    pub tee_authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(meta_id: u64)]
pub struct CloseDataEntryMeta<'info> {
    #[account(
        seeds = [b"registry_state"],
        bump = registry_state.bump,
    )]
    pub registry_state: Account<'info, RegistryState>,

    #[account(
        mut,
        seeds = [b"meta", meta_id.to_le_bytes().as_ref()],
        bump = data_entry_meta.bump,
        constraint = data_entry_meta.owner == provider.key() @ RegistryError::NotOwner,
        close = provider,
    )]
    pub data_entry_meta: Account<'info, DataEntryMeta>,

    #[account(mut)]
    pub provider: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(meta_id: u64, unit_index: u32)]
pub struct CloseUploadUnit<'info> {
    #[account(
        seeds = [b"meta", meta_id.to_le_bytes().as_ref()],
        bump = data_entry_meta.bump,
        constraint = data_entry_meta.owner == provider.key() @ RegistryError::NotOwner,
    )]
    pub data_entry_meta: Account<'info, DataEntryMeta>,

    #[account(
        mut,
        seeds = [b"unit", meta_id.to_le_bytes().as_ref(), unit_index.to_le_bytes().as_ref()],
        bump = upload_unit.bump,
        close = provider,
    )]
    pub upload_unit: Account<'info, UploadUnit>,

    #[account(mut)]
    pub provider: Signer<'info>,
}
