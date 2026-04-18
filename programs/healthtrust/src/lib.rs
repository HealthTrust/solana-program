use anchor_lang::prelude::*;

declare_id!("48MrGAXhM98UYTYbc1bVRi267A5SMGsBvC3fLwDPQ3Em");

#[program]
pub mod healthtrust {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
