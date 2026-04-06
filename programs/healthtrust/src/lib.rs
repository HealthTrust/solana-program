use anchor_lang::prelude::*;

declare_id!("D8snznSwvLyCsk17CxbukmKZHGsBxEx23Qi1kaqDe5Ku");

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
