use anchor_lang::prelude::*;

use crate::errors::RegistryError;
use crate::state::RegistryState;

#[derive(Accounts)]
pub struct InitializeRegistry<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + RegistryState::INIT_SPACE,
        seeds = [b"registry_state"],
        bump,
    )]
    pub registry_state: Account<'info, RegistryState>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AuthorizedRegistryAction<'info> {
    #[account(
        mut,
        seeds = [b"registry_state"],
        bump = registry_state.bump,
        has_one = owner @ RegistryError::Unauthorized,
    )]
    pub registry_state: Account<'info, RegistryState>,

    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct EmergencyWithdrawSol<'info> {
    #[account(
        mut,
        seeds = [b"registry_state"],
        bump = registry_state.bump,
        has_one = owner @ RegistryError::Unauthorized,
    )]
    pub registry_state: Account<'info, RegistryState>,

    /// CHECK: arbitrary recipient - owner's responsibility to specify correctly.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,

    pub owner: Signer<'info>,
}
