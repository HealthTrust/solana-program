use anchor_lang::prelude::*;

use crate::errors::OrderError;
use crate::state::OrderConfig;

#[derive(Accounts)]
pub struct InitializeOrderConfig<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + OrderConfig::INIT_SPACE,
        seeds = [b"order_config"],
        bump,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AuthorizedOrderAction<'info> {
    #[account(
        mut,
        seeds = [b"order_config"],
        bump = order_config.bump,
        has_one = owner @ OrderError::Unauthorized,
    )]
    pub order_config: Account<'info, OrderConfig>,

    pub owner: Signer<'info>,
}
