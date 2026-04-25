use anchor_lang::prelude::*;

use crate::contexts::{AuthorizedOrderAction, InitializeOrderConfig};
use crate::errors::OrderError;
use crate::events::{OwnershipTransferred, RoflAuthorityUpdated};

pub fn initialize_order_config(
    ctx: Context<InitializeOrderConfig>,
    rofl_authority: Pubkey,
) -> Result<()> {
    require_keys_neq!(
        rofl_authority,
        Pubkey::default(),
        OrderError::InvalidAuthority
    );

    let order_config = &mut ctx.accounts.order_config;
    order_config.owner = ctx.accounts.owner.key();
    order_config.rofl_authority = rofl_authority;
    order_config.next_job_id = 1;
    order_config.bump = ctx.bumps.order_config;
    Ok(())
}

pub fn set_rofl_authority(
    ctx: Context<AuthorizedOrderAction>,
    rofl_authority: Pubkey,
) -> Result<()> {
    require_keys_neq!(
        rofl_authority,
        Pubkey::default(),
        OrderError::InvalidAuthority
    );
    ctx.accounts.order_config.rofl_authority = rofl_authority;
    emit!(RoflAuthorityUpdated { rofl_authority });
    Ok(())
}

pub fn transfer_order_ownership(
    ctx: Context<AuthorizedOrderAction>,
    new_owner: Pubkey,
) -> Result<()> {
    require_keys_neq!(new_owner, Pubkey::default(), OrderError::InvalidOwner);
    let previous = ctx.accounts.order_config.owner;
    ctx.accounts.order_config.owner = new_owner;
    emit!(OwnershipTransferred {
        previous,
        new: new_owner,
    });
    Ok(())
}
