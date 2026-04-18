// ============================================================================
// pricing_config — HealthTrust Solana Migration
// ============================================================================
// Migrated from: Pricing.sol (PricingConfig contract)
//
// Key changes vs EVM:
// - string-keyed mappings replaced with sha256-derived PDA seeds per data type
// - Bonding-curve code (commented-out in EVM) omitted entirely
// - calculateScarcityMultiplier uses fixed-point (1e9 denominator) instead of 1 ether
// - updateMultiplierForScarcity was unguarded in EVM; here any signer may call it
//   (same permissiveness), but a production deployment should set approved_registry
//   and enforce it in UpdateMultiplier accounts constraint
// - All setters collapsed into initialize_pricing + update_pricing_params
// ============================================================================

use anchor_lang::prelude::*;

declare_id!("7wdywHkoSnCwSYxBiE1xdweqkdKxi4KFXUtscK8mCTTN");
// Replace with: `anchor keys list` after running `anchor build`

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum allowed platform fee (30%). Mirrors the EVM cap.
pub const MAX_PLATFORM_FEE_BPS: u16 = 3_000;

/// Fixed-point denominator for scarcity multiplier.
/// "1.0" is represented as 1_000_000_000 (1e9).
/// Mirrors the EVM `1 ether` denominator but scaled for lamport-range math.
pub const SCARCITY_PRECISION: u64 = 1_000_000_000;

// ─── Program ─────────────────────────────────────────────────────────────────

#[program]
pub mod pricing_config {
    use super::*;

    /// One-time initialization of the global pricing singleton.
    /// Seeds: [b"pricing_params"] — there is exactly one PricingParams per deployment.
    pub fn initialize_pricing(
        ctx: Context<InitializePricing>,
        params: PricingInitParams,
    ) -> Result<()> {
        require!(
            params.platform_fee_bps <= MAX_PLATFORM_FEE_BPS,
            PricingError::FeeTooHigh
        );
        let p = &mut ctx.accounts.pricing_params;
        p.owner = ctx.accounts.owner.key();
        p.base_price = params.base_price;
        p.duration_factor = params.duration_factor;
        p.platform_fee_bps = params.platform_fee_bps;
        p.min_total_charge = params.min_total_charge;
        p.preflight_fee = params.preflight_fee;
        p.bump = ctx.bumps.pricing_params;

        emit!(PricingInitialized {
            owner: p.owner,
            base_price: p.base_price,
            duration_factor: p.duration_factor,
            platform_fee_bps: p.platform_fee_bps,
            min_total_charge: p.min_total_charge,
            preflight_fee: p.preflight_fee,
        });
        Ok(())
    }

    /// Update all pricing parameters atomically. Owner only.
    /// Replaces the individual EVM setters (setBasePrice, setDurationFactor, etc.)
    /// with a single batched instruction to reduce round-trips.
    pub fn update_pricing_params(
        ctx: Context<AuthorizedPricingAction>,
        params: PricingInitParams,
    ) -> Result<()> {
        require!(
            params.platform_fee_bps <= MAX_PLATFORM_FEE_BPS,
            PricingError::FeeTooHigh
        );
        let p = &mut ctx.accounts.pricing_params;
        p.base_price = params.base_price;
        p.duration_factor = params.duration_factor;
        p.platform_fee_bps = params.platform_fee_bps;
        p.min_total_charge = params.min_total_charge;
        p.preflight_fee = params.preflight_fee;

        emit!(PricingUpdated {
            base_price: p.base_price,
            duration_factor: p.duration_factor,
            platform_fee_bps: p.platform_fee_bps,
            min_total_charge: p.min_total_charge,
            preflight_fee: p.preflight_fee,
        });
        Ok(())
    }

    /// Transfer ownership of the PricingParams account.
    pub fn transfer_pricing_ownership(
        ctx: Context<AuthorizedPricingAction>,
        new_owner: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(new_owner, Pubkey::default(), PricingError::InvalidOwner);
        let previous = ctx.accounts.pricing_params.owner;
        ctx.accounts.pricing_params.owner = new_owner;
        emit!(OwnershipTransferred { previous, new: new_owner });
        Ok(())
    }

    /// Create a TypeMultiplier PDA for a data type that does not yet exist.
    /// The data_type_hash is sha256(data_type_string.as_bytes()), computed off-chain.
    ///
    /// DESIGN NOTE: Explicit initialization (rather than init_if_needed) ensures
    /// only the pricing owner can introduce new data types. This prevents spam
    /// creation of TypeMultiplier accounts by arbitrary callers.
    pub fn initialize_type_multiplier(
        ctx: Context<InitTypeMultiplier>,
        data_type_hash: [u8; 32],
    ) -> Result<()> {
        let tm = &mut ctx.accounts.type_multiplier;
        tm.data_type_hash = data_type_hash;
        tm.total_duration = 0;
        // Starts at 1.0 (SCARCITY_PRECISION / 1 = full scarcity — no data yet)
        tm.multiplier = SCARCITY_PRECISION;
        tm.bump = ctx.bumps.type_multiplier;
        Ok(())
    }

    /// Increment total duration for a data type and recompute its scarcity multiplier.
    ///
    /// EVM equivalent: updateMultiplierForScarcity / updateMultipliersForScarcityBatch
    ///
    /// EVM FIX: The EVM function was external with no access control — anyone could call
    /// it and artificially inflate totalDataPerType, deflating the multiplier. For
    /// production, restrict via approved_registry in PricingParams. For MVP, left open
    /// (same as EVM behavior).
    ///
    /// BATCHING NOTE: To update N data types atomically (replacing EVM's batch function),
    /// the client sends N update_multiplier_for_scarcity instructions in a single
    /// transaction. This is idiomatic Solana and avoids remaining_accounts complexity.
    pub fn update_multiplier_for_scarcity(
        ctx: Context<UpdateMultiplier>,
        _data_type_hash: [u8; 32],
        added_duration: u64,
    ) -> Result<()> {
        require!(added_duration > 0, PricingError::ZeroDuration);

        let tm = &mut ctx.accounts.type_multiplier;
        tm.total_duration = tm
            .total_duration
            .checked_add(added_duration)
            .ok_or(PricingError::Overflow)?;
        tm.multiplier = calculate_scarcity_multiplier(tm.total_duration);

        emit!(MultiplierUpdated {
            data_type_hash: tm.data_type_hash,
            total_duration: tm.total_duration,
            multiplier: tm.multiplier,
        });
        Ok(())
    }
}

// ─── Pure Helpers ─────────────────────────────────────────────────────────────

/// Mirrors EVM's `1 ether / totalDuration` using SCARCITY_PRECISION fixed-point.
/// Higher scarcity (less data) → higher multiplier.
/// Returns SCARCITY_PRECISION ("1.0") when totalDuration == 0 (no data available yet).
fn calculate_scarcity_multiplier(total_duration: u64) -> u64 {
    if total_duration == 0 {
        return SCARCITY_PRECISION;
    }
    SCARCITY_PRECISION / total_duration
}

// ─── Account Structs ──────────────────────────────────────────────────────────

/// Global singleton — one per deployment.
/// Seeds: [b"pricing_params"]
/// Size: 8 (discriminator) + 74 = 82 bytes
#[account]
#[derive(InitSpace)]
pub struct PricingParams {
    pub owner: Pubkey,          // 32
    pub base_price: u64,        // 8 — per-participant base (lamports)
    pub duration_factor: u64,   // 8 — per-participant per-day multiplier (lamports)
    pub platform_fee_bps: u16,  // 2 — e.g. 1500 = 15%
    pub min_total_charge: u64,  // 8 — minimum job total (lamports)
    pub preflight_fee: u64,     // 8 — flat preflight charge (lamports)
    pub bump: u8,               // 1
    // Reserved for future approved_registry access control:
    // pub approved_registry: Pubkey,
}

/// One PDA per data type.
/// Seeds: [b"type_multiplier", data_type_hash[..32]]
/// Size: 8 (discriminator) + 57 = 65 bytes
#[account]
#[derive(InitSpace)]
pub struct TypeMultiplier {
    /// sha256(data_type_string) — stored for on-chain auditability.
    pub data_type_hash: [u8; 32],    // 32
    /// Cumulative seconds of data across all uploads of this type.
    pub total_duration: u64,          // 8
    /// Fixed-point multiplier with SCARCITY_PRECISION denominator.
    /// Decreases as more data is uploaded (less scarcity).
    pub multiplier: u64,              // 8
    pub bump: u8,                     // 1
}

// ─── Instruction Params ───────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PricingInitParams {
    pub base_price: u64,
    pub duration_factor: u64,
    pub platform_fee_bps: u16,
    pub min_total_charge: u64,
    pub preflight_fee: u64,
}

// ─── Accounts Contexts ────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitializePricing<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + PricingParams::INIT_SPACE,
        seeds = [b"pricing_params"],
        bump,
    )]
    pub pricing_params: Account<'info, PricingParams>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Shared context for all owner-gated PricingParams mutations.
#[derive(Accounts)]
pub struct AuthorizedPricingAction<'info> {
    #[account(
        mut,
        seeds = [b"pricing_params"],
        bump = pricing_params.bump,
        has_one = owner @ PricingError::Unauthorized,
    )]
    pub pricing_params: Account<'info, PricingParams>,

    pub owner: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(data_type_hash: [u8; 32])]
pub struct InitTypeMultiplier<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + TypeMultiplier::INIT_SPACE,
        seeds = [b"type_multiplier", data_type_hash.as_ref()],
        bump,
    )]
    pub type_multiplier: Account<'info, TypeMultiplier>,

    /// Presence of pricing_params with has_one = owner proves the signer is the admin.
    #[account(
        seeds = [b"pricing_params"],
        bump = pricing_params.bump,
        has_one = owner @ PricingError::Unauthorized,
    )]
    pub pricing_params: Account<'info, PricingParams>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(data_type_hash: [u8; 32])]
pub struct UpdateMultiplier<'info> {
    #[account(
        mut,
        seeds = [b"type_multiplier", data_type_hash.as_ref()],
        bump = type_multiplier.bump,
    )]
    pub type_multiplier: Account<'info, TypeMultiplier>,

    /// Any signer may call this (mirrors EVM behavior).
    /// TODO (production): restrict to pricing_params.approved_registry.
    pub caller: Signer<'info>,
}

// ─── Events ───────────────────────────────────────────────────────────────────

#[event]
pub struct PricingInitialized {
    pub owner: Pubkey,
    pub base_price: u64,
    pub duration_factor: u64,
    pub platform_fee_bps: u16,
    pub min_total_charge: u64,
    pub preflight_fee: u64,
}

#[event]
pub struct PricingUpdated {
    pub base_price: u64,
    pub duration_factor: u64,
    pub platform_fee_bps: u16,
    pub min_total_charge: u64,
    pub preflight_fee: u64,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}

#[event]
pub struct MultiplierUpdated {
    pub data_type_hash: [u8; 32],
    pub total_duration: u64,
    pub multiplier: u64,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[error_code]
pub enum PricingError {
    #[msg("Platform fee exceeds maximum of 30%")]
    FeeTooHigh,
    #[msg("Caller is not the pricing owner")]
    Unauthorized,
    #[msg("Added duration must be greater than zero")]
    ZeroDuration,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("New owner cannot be the zero address")]
    InvalidOwner,
}
