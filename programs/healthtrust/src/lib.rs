use anchor_lang::prelude::*;

declare_id!("gU8YvjomPHc9aviuNUwWoxD8Fpx1kKKgEY5FKLBm14E");

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
