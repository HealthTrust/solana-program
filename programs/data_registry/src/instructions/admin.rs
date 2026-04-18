use anchor_lang::prelude::*;

use crate::contexts::{AuthorizedRegistryAction, EmergencyWithdrawSol, InitializeRegistry};
use crate::errors::RegistryError;
use crate::events::{EmergencyWithdrawal, OwnershipTransferred, PausedStateChanged};

pub fn initialize_registry(ctx: Context<InitializeRegistry>, tee_authority: Pubkey) -> Result<()> {
    require_keys_neq!(tee_authority, Pubkey::default(), RegistryError::InvalidAuthority);
    let s = &mut ctx.accounts.registry_state;
    s.owner = ctx.accounts.owner.key();
    s.pricing_program = Pubkey::default();
    s.tee_authority = tee_authority;
    s.next_meta_id = 1;
    s.paused = false;
    s.bump = ctx.bumps.registry_state;
    Ok(())
}

pub fn set_pricing_program(
    ctx: Context<AuthorizedRegistryAction>,
    pricing_program: Pubkey,
) -> Result<()> {
    ctx.accounts.registry_state.pricing_program = pricing_program;
    Ok(())
}

pub fn set_tee_authority(
    ctx: Context<AuthorizedRegistryAction>,
    tee_authority: Pubkey,
) -> Result<()> {
    require_keys_neq!(tee_authority, Pubkey::default(), RegistryError::InvalidAuthority);
    ctx.accounts.registry_state.tee_authority = tee_authority;
    Ok(())
}

pub fn set_paused(ctx: Context<AuthorizedRegistryAction>, paused: bool) -> Result<()> {
    ctx.accounts.registry_state.paused = paused;
    emit!(PausedStateChanged { paused });
    Ok(())
}

pub fn transfer_registry_ownership(
    ctx: Context<AuthorizedRegistryAction>,
    new_owner: Pubkey,
) -> Result<()> {
    require_keys_neq!(new_owner, Pubkey::default(), RegistryError::InvalidOwner);
    let previous = ctx.accounts.registry_state.owner;
    ctx.accounts.registry_state.owner = new_owner;
    emit!(OwnershipTransferred {
        previous,
        new: new_owner,
    });
    Ok(())
}

pub fn emergency_withdraw_sol(ctx: Context<EmergencyWithdrawSol>, amount: u64) -> Result<()> {
    ctx.accounts.registry_state.sub_lamports(amount)?;
    ctx.accounts.recipient.add_lamports(amount)?;
    emit!(EmergencyWithdrawal {
        recipient: ctx.accounts.recipient.key(),
        amount,
    });
    Ok(())
}
