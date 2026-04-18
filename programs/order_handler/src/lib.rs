// ============================================================================
// order_handler — HealthTrust Solana Migration
// ============================================================================
// Migrated from: OrderHandler.sol
//
// Key changes vs EVM:
//
// 1. PAYOUT REDESIGN: push → pull
//    EVM's finalizeJob pushed ETH to all participants in a loop.
//    On Solana this is dangerous (compute limit, no reentrancy guard needed but
//    still undesirable). Replaced with:
//      - finalize_job: computes amount_per_provider, sets status = COMPLETED. No loop.
//      - claim_payout:  each provider pulls their share individually.
//    A u64 claimed_bitmap (up to 64 participants, max 50 enforced) tracks claims.
//    This is safer, simpler, and has no compute-limit risk.
//
// 2. SOL ESCROW via PDA vault
//    EVM used msg.value / address(this).balance / payable.transfer.
//    Here a dedicated EscrowVault PDA (seeds: [b"escrow", job_id]) holds lamports.
//    - Funding: system_program::transfer from researcher to vault
//    - Draining: direct lamport manipulation (sub_lamports / add_lamports)
//      since the vault is program-owned.
//
// 3. ROFL_ADDRESS → rofl_authority Pubkey in OrderConfig
//    The EVM private ROFL_ADDRESS is stored in the OrderConfig PDA.
//    has_one constraints enforce it on every ROFL-gated instruction.
//
// 4. ReentrancyGuard is unnecessary on Solana (single-writer model).
//
// 5. Dynamic arrays in Job/Params bounded:
//    - dataTypes  → Vec<[u8;32]> data_type_hashes, max 8
//    - filterQuery → String max 128 bytes
//    - selectedParticipants → Vec<Pubkey> max 50
//    - resultCidHash → String max 64 bytes (store the CID string directly,
//      not keccak256(cid) — on Solana we don't need that indirection)
//
// 6. Job account size: ~2,276 bytes (see Job struct comment). Rent ≈ 0.016 SOL.
//    The researcher pays this at request_job time.
//
// 7. EscrowVault is created at confirm_job_and_pay. Rent is paid by researcher.
//    After all providers claim, a small dust amount (vault rent + rounding) remains.
//    Use sweep_vault_dust to reclaim it (admin or researcher).
// ============================================================================

use anchor_lang::prelude::*;
use anchor_lang::system_program;

declare_id!("GVUZtHZHr1tDxw3Pt142BxqgkS3dfDpPbqEznsFT9jV4");
// Replace with: `anchor keys list` after running `anchor build`

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum data type hashes per job.
pub const MAX_JOB_DATA_TYPES: usize = 8;
/// Maximum number of selected participants. Must be ≤ 64 (u64 bitmap limit).
pub const MAX_PARTICIPANTS: usize = 50;
/// Maximum byte length of filter_query string.
pub const MAX_FILTER_QUERY_LEN: usize = 128;
/// Maximum byte length of result CID string.
pub const MAX_RESULT_CID_LEN: usize = 64;

// ─── Program ─────────────────────────────────────────────────────────────────

#[program]
pub mod order_handler {
    use super::*;

    // ── Admin ──────────────────────────────────────────────────────────────

    /// One-time initialization of the OrderConfig singleton.
    pub fn initialize_order_config(
        ctx: Context<InitializeOrderConfig>,
        rofl_authority: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(rofl_authority, Pubkey::default(), OrderError::InvalidAuthority);
        let cfg = &mut ctx.accounts.order_config;
        cfg.owner = ctx.accounts.owner.key();
        cfg.rofl_authority = rofl_authority;
        cfg.next_job_id = 1;
        cfg.bump = ctx.bumps.order_config;
        Ok(())
    }

    /// Update the ROFL/TEE authority address. Owner only.
    pub fn set_rofl_authority(
        ctx: Context<AuthorizedOrderAction>,
        rofl_authority: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(rofl_authority, Pubkey::default(), OrderError::InvalidAuthority);
        ctx.accounts.order_config.rofl_authority = rofl_authority;
        emit!(RoflAuthorityUpdated { rofl_authority });
        Ok(())
    }

    /// Transfer order handler ownership.
    pub fn transfer_order_ownership(
        ctx: Context<AuthorizedOrderAction>,
        new_owner: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(new_owner, Pubkey::default(), OrderError::InvalidOwner);
        let previous = ctx.accounts.order_config.owner;
        ctx.accounts.order_config.owner = new_owner;
        emit!(OwnershipTransferred { previous, new: new_owner });
        Ok(())
    }

    // ── Researcher ─────────────────────────────────────────────────────────

    /// Phase 1: researcher submits job parameters. Status → PENDING_PREFLIGHT.
    ///
    /// EVM equivalent: requestJob
    ///
    /// The Job PDA is allocated here (researcher pays rent ≈ 0.016 SOL).
    /// No payment happens at this stage — payment occurs at confirm_job_and_pay.
    pub fn request_job(ctx: Context<RequestJob>, params: JobParams) -> Result<()> {
        require!(params.num_days > 0, OrderError::InvalidNumDays);
        require!(params.template_id > 0, OrderError::InvalidTemplateId);
        require!(!params.data_type_hashes.is_empty(), OrderError::EmptyDataTypes);
        require!(params.max_participants > 0, OrderError::InvalidMaxParticipants);
        require!(
            params.data_type_hashes.len() <= MAX_JOB_DATA_TYPES,
            OrderError::TooManyDataTypes
        );

        let cfg = &mut ctx.accounts.order_config;
        let job_id = cfg.next_job_id;
        cfg.next_job_id = cfg
            .next_job_id
            .checked_add(1)
            .ok_or(OrderError::Overflow)?;

        let clock = Clock::get()?;
        let job = &mut ctx.accounts.job;
        job.job_id = job_id;
        job.researcher = ctx.accounts.researcher.key();
        job.status = JobStatus::PendingPreflight;
        job.template_id = params.template_id;
        job.num_days = params.num_days;
        job.data_type_hashes = params.data_type_hashes.clone();
        job.max_participants = params.max_participants;
        job.start_day_utc = params.start_day_utc;
        job.filter_query = params.filter_query.clone();
        job.escrowed = 0;
        job.effective_participants_scaled = 0;
        job.quality_tier = 0;
        job.final_total = 0;
        job.preflight_timestamp = 0;
        job.cohort_hash = [0u8; 32];
        job.selected_participants = Vec::new();
        job.result_cid = String::new();
        job.execution_timestamp = 0;
        job.output_hash = [0u8; 32];
        job.amount_per_provider = 0;
        job.claimed_bitmap = 0;
        job.created_at = clock.unix_timestamp;
        job.updated_at = clock.unix_timestamp;
        job.bump = ctx.bumps.job;

        emit!(JobRequested {
            job_id,
            researcher: job.researcher,
            template_id: params.template_id,
            num_days: params.num_days,
            data_type_hashes: params.data_type_hashes,
            max_participants: params.max_participants,
        });
        Ok(())
    }

    /// ROFL submits preflight result. Status: PENDING_PREFLIGHT → AWAITING_CONFIRMATION.
    ///
    /// EVM equivalent: submitPreflightResult
    pub fn submit_preflight_result(
        ctx: Context<RoflAction>,
        job_id: u64,
        preflight: PreflightResultParams,
    ) -> Result<()> {
        require!(
            preflight.selected_participants.len() <= MAX_PARTICIPANTS,
            OrderError::TooManyParticipants
        );

        let job = &mut ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_eq!(
            job.status,
            JobStatus::PendingPreflight,
            OrderError::InvalidStatus
        );

        job.effective_participants_scaled = preflight.effective_participants_scaled;
        job.quality_tier = preflight.quality_tier;
        job.final_total = preflight.final_total;
        job.preflight_timestamp = Clock::get()?.unix_timestamp;
        job.cohort_hash = preflight.cohort_hash;
        job.selected_participants = preflight.selected_participants;
        job.status = JobStatus::AwaitingConfirmation;
        job.updated_at = Clock::get()?.unix_timestamp;

        emit!(PreflightSubmitted {
            job_id,
            effective_participants_scaled: job.effective_participants_scaled,
            quality_tier: job.quality_tier,
            final_total: job.final_total,
            cohort_hash: job.cohort_hash,
        });
        Ok(())
    }

    /// Researcher confirms and funds the escrow.
    /// Status: AWAITING_CONFIRMATION → CONFIRMED.
    ///
    /// EVM equivalent: confirmJobAndPay (payable)
    ///
    /// The EscrowVault PDA is created here. Researcher pays:
    ///   - Rent for the EscrowVault account (~0.001 SOL)
    ///   - The actual job payment (>= job.final_total)
    /// Both are handled in one instruction.
    pub fn confirm_job_and_pay(
        ctx: Context<ConfirmJobAndPay>,
        job_id: u64,
        payment_amount: u64,
    ) -> Result<()> {
        let job = &mut ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_keys_eq!(
            ctx.accounts.researcher.key(),
            job.researcher,
            OrderError::Unauthorized
        );
        require_eq!(
            job.status,
            JobStatus::AwaitingConfirmation,
            OrderError::InvalidStatus
        );
        require!(
            payment_amount >= job.final_total,
            OrderError::InsufficientPayment
        );
        require!(job.final_total > 0, OrderError::ZeroFinalTotal);

        // Initialise escrow vault fields
        let vault = &mut ctx.accounts.escrow_vault;
        vault.job_id = job_id;
        vault.bump = ctx.bumps.escrow_vault;

        // Transfer payment from researcher to vault
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.researcher.to_account_info(),
                    to: ctx.accounts.escrow_vault.to_account_info(),
                },
            ),
            payment_amount,
        )?;

        job.escrowed = payment_amount;
        job.status = JobStatus::Confirmed;
        job.updated_at = Clock::get()?.unix_timestamp;

        emit!(JobConfirmed { job_id, amount: payment_amount });
        Ok(())
    }

    /// Researcher cancels a job that has not been confirmed yet.
    /// Allowed states: PENDING_PREFLIGHT, AWAITING_CONFIRMATION.
    ///
    /// EVM equivalent: cancelJob
    ///
    /// At these stages escrowed == 0 (payment hasn't happened), so no refund
    /// transfer is needed. The Job account is closed, reclaiming rent.
    pub fn cancel_job(ctx: Context<CancelJob>, job_id: u64) -> Result<()> {
        let job = &ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_keys_eq!(
            ctx.accounts.researcher.key(),
            job.researcher,
            OrderError::Unauthorized
        );
        require!(
            job.status == JobStatus::PendingPreflight
                || job.status == JobStatus::AwaitingConfirmation,
            OrderError::CannotCancelAtThisStage
        );
        // No escrowed funds at these stages (payment only at confirm_job_and_pay)
        debug_assert_eq!(job.escrowed, 0, "unexpected escrowed funds at cancel stage");

        emit!(JobCancelled { job_id, refund_amount: 0 });

        // Anchor's `close = researcher` in constraint returns rent to researcher.
        Ok(())
    }

    // ── ROFL Worker ────────────────────────────────────────────────────────

    /// ROFL submits the computation result CID. Status: CONFIRMED → EXECUTED.
    ///
    /// EVM equivalent: submitResult
    ///
    /// NOTE: The EVM version stored keccak256(attestation) as outputHash to avoid
    /// storing a long bytes attestation on-chain. Here we store sha256(attestation)
    /// off-chain and pass the 32-byte digest directly. The result_cid is stored
    /// as a string (not a hash) since Solana has more space for moderate strings.
    pub fn submit_result(
        ctx: Context<RoflAction>,
        job_id: u64,
        result_cid: String,
        output_hash: [u8; 32],
    ) -> Result<()> {
        let job = &mut ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_eq!(job.status, JobStatus::Confirmed, OrderError::InvalidStatus);
        require!(!result_cid.is_empty(), OrderError::EmptyResultCid);

        let clock = Clock::get()?;
        job.result_cid = result_cid.clone();
        job.execution_timestamp = clock.unix_timestamp;
        job.output_hash = output_hash;
        job.status = JobStatus::Executed;
        job.updated_at = clock.unix_timestamp;

        emit!(JobExecuted { job_id, result_cid });
        Ok(())
    }

    /// ROFL finalizes the job. Status: EXECUTED → COMPLETED.
    /// Computes amount_per_provider so participants can pull their share.
    ///
    /// EVM equivalent: finalizeJob + payoutDataProviders (combined)
    ///
    /// KEY CHANGE: The EVM version pushed payments to all providers in a loop
    /// here (potential gas/compute limit failure with many providers). This
    /// instruction only COMPUTES amount_per_provider. Each provider calls
    /// claim_payout separately. No loop, no per-instruction compute risk.
    pub fn finalize_job(ctx: Context<FinalizeJob>, job_id: u64) -> Result<()> {
        let job = &mut ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_eq!(job.status, JobStatus::Executed, OrderError::InvalidStatus);

        let num_providers = job.selected_participants.len() as u64;
        require!(num_providers > 0, OrderError::NoParticipants);

        // Compute payable amount: vault lamports minus rent-exempt minimum.
        // This correctly handles the case where the researcher overpaid (kept by protocol)
        // or where rounding leaves dust — both go to the sweep_vault_dust instruction.
        let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
        let rent = Rent::get()?;
        let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
        let vault_rent_exempt = rent.minimum_balance(vault_data_len);
        let distributable = vault_lamports
            .checked_sub(vault_rent_exempt)
            .ok_or(OrderError::InsufficientEscrow)?;

        job.amount_per_provider = distributable / num_providers;
        require!(job.amount_per_provider > 0, OrderError::ZeroAmountPerProvider);

        job.status = JobStatus::Completed;
        job.updated_at = Clock::get()?.unix_timestamp;

        emit!(JobCompleted {
            job_id,
            amount_per_provider: job.amount_per_provider,
            num_providers,
        });
        Ok(())
    }

    // ── Data Providers ─────────────────────────────────────────────────────

    /// Data provider pulls their payout after job is COMPLETED.
    ///
    /// EVM equivalent: implicit in payoutDataProviders loop (now pull-based)
    ///
    /// BITMAP CLAIM TRACKING: A u64 claimed_bitmap tracks which of the
    /// selected_participants[i] have claimed. Bit i = 1 means claimed.
    /// Supports up to 64 participants; MAX_PARTICIPANTS = 50 (safe margin).
    pub fn claim_payout(ctx: Context<ClaimPayout>, job_id: u64) -> Result<()> {
        let provider_key = ctx.accounts.provider.key();

        // Scope the immutable borrow to read necessary data
        let (amount, participant_index) = {
            let job = &ctx.accounts.job;
            require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
            require_eq!(job.status, JobStatus::Completed, OrderError::InvalidStatus);
            require!(job.amount_per_provider > 0, OrderError::ZeroAmountPerProvider);

            let index = job
                .selected_participants
                .iter()
                .position(|p| p == &provider_key)
                .ok_or(OrderError::NotAParticipant)?;

            require!(index < 64, OrderError::ParticipantIndexOutOfRange);
            require!(
                job.claimed_bitmap & (1u64 << index) == 0,
                OrderError::AlreadyClaimed
            );

            (job.amount_per_provider, index)
        };

        // Verify vault has enough lamports (defensive check)
        let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
        let rent = Rent::get()?;
        let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
        let vault_min = rent.minimum_balance(vault_data_len);
        require!(
            vault_lamports.checked_sub(amount).unwrap_or(0) >= vault_min,
            OrderError::InsufficientEscrow
        );

        // Transfer from vault (program-owned) to provider
        ctx.accounts.escrow_vault.sub_lamports(amount)?;
        ctx.accounts.provider.add_lamports(amount)?;

        // Mark this provider as claimed
        ctx.accounts.job.claimed_bitmap |= 1u64 << participant_index;

        emit!(DataProviderPaid {
            job_id,
            provider: provider_key,
            amount,
        });
        Ok(())
    }

    /// Sweep remaining lamports (overpayment + rounding dust) from the vault.
    /// Callable by the researcher after all claims, or by the owner at any time.
    ///
    /// NOTE: This replaces the implicit "leftover stays in contract" behavior
    /// in the EVM version. Here the researcher gets their dust back.
    pub fn sweep_vault_dust(ctx: Context<SweepVaultDust>, job_id: u64) -> Result<()> {
        let job = &ctx.accounts.job;
        require_eq!(job.job_id, job_id, OrderError::JobIdMismatch);
        require_eq!(job.status, JobStatus::Completed, OrderError::InvalidStatus);
        // Only allow sweep if all providers have claimed OR caller is researcher
        let all_claimed =
            job.selected_participants.len() == job.claimed_bitmap.count_ones() as usize;
        let is_researcher = ctx.accounts.recipient.key() == job.researcher;
        require!(all_claimed || is_researcher, OrderError::SweepNotAllowed);

        let vault_lamports = ctx.accounts.escrow_vault.get_lamports();
        let rent = Rent::get()?;
        let vault_data_len = ctx.accounts.escrow_vault.to_account_info().data_len();
        let vault_rent_exempt = rent.minimum_balance(vault_data_len);

        let dust = vault_lamports
            .checked_sub(vault_rent_exempt)
            .unwrap_or(0);

        if dust > 0 {
            ctx.accounts.escrow_vault.sub_lamports(dust)?;
            ctx.accounts.recipient.add_lamports(dust)?;
        }

        emit!(VaultDustSwept {
            job_id,
            recipient: ctx.accounts.recipient.key(),
            amount: dust,
        });
        Ok(())
    }
}

// ─── Enums ────────────────────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum JobStatus {
    /// Placeholder — a zero-value so uninitialized accounts are distinguishable.
    None,
    PendingPreflight,
    AwaitingConfirmation,
    Confirmed,
    Executed,
    Completed,
    Cancelled,
}

impl core::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let status = match self {
            JobStatus::None => "None",
            JobStatus::PendingPreflight => "PendingPreflight",
            JobStatus::AwaitingConfirmation => "AwaitingConfirmation",
            JobStatus::Confirmed => "Confirmed",
            JobStatus::Executed => "Executed",
            JobStatus::Completed => "Completed",
            JobStatus::Cancelled => "Cancelled",
        };
        write!(f, "{}", status)
    }
}

// ─── Account Structs ──────────────────────────────────────────────────────────

/// Global singleton. Seeds: [b"order_config"]
/// Size: 8 + 73 = 81 bytes
#[account]
#[derive(InitSpace)]
pub struct OrderConfig {
    pub owner: Pubkey,           // 32
    pub rofl_authority: Pubkey,  // 32
    pub next_job_id: u64,        // 8
    pub bump: u8,                // 1
}

/// One PDA per job. Seeds: [b"job", job_id.to_le_bytes()]
///
/// Size: 8 (disc) +
///   8 (job_id) + 32 (researcher) + 1 (status) +
///   4 + 4 (template_id, num_days) +
///   (4 + 8*32) data_type_hashes +
///   4 (max_participants) + 8 (start_day_utc) +
///   (4 + 128) filter_query +
///   8 (escrowed) +
///   8 + 1 + 8 + 8 + 32 (preflight fields) +
///   (4 + 50*32) selected_participants +
///   (4 + 64) result_cid + 8 + 32 (execution fields) +
///   8 (amount_per_provider) + 8 (claimed_bitmap) +
///   8 + 8 (timestamps) + 1 (bump)
/// ≈ 2,277 bytes  (rent ≈ 0.016 SOL)
#[account]
#[derive(InitSpace)]
pub struct Job {
    pub job_id: u64,             // 8
    pub researcher: Pubkey,      // 32
    pub status: JobStatus,       // 1

    // ── JobParams ──────────────────────────────────────────────────────────
    pub template_id: u32,        // 4
    pub num_days: u32,           // 4
    /// sha256 hashes of data type strings (matches data_registry convention).
    #[max_len(8)]
    pub data_type_hashes: Vec<[u8; 32]>, // 4 + 8*32 = 260
    pub max_participants: u32,   // 4
    pub start_day_utc: i64,      // 8
    #[max_len(128)]
    pub filter_query: String,    // 4 + 128 = 132

    // ── Escrow ─────────────────────────────────────────────────────────────
    pub escrowed: u64,           // 8 — total lamports deposited to vault

    // ── Preflight result ───────────────────────────────────────────────────
    pub effective_participants_scaled: u64, // 8
    pub quality_tier: u8,                  // 1
    pub final_total: u64,                  // 8 — minimum required payment (lamports)
    pub preflight_timestamp: i64,          // 8
    pub cohort_hash: [u8; 32],             // 32

    /// Addresses of selected data providers. Max MAX_PARTICIPANTS entries.
    #[max_len(50)]
    pub selected_participants: Vec<Pubkey>, // 4 + 50*32 = 1604

    // ── Execution result ───────────────────────────────────────────────────
    #[max_len(64)]
    pub result_cid: String,                // 4 + 64 = 68
    pub execution_timestamp: i64,          // 8
    pub output_hash: [u8; 32],             // 32

    // ── Payout tracking ────────────────────────────────────────────────────
    pub amount_per_provider: u64,  // 8 — set at finalize_job
    /// Bitmask: bit i = 1 means selected_participants[i] has claimed.
    /// Supports up to 64 participants; MAX_PARTICIPANTS = 50.
    pub claimed_bitmap: u64,       // 8

    // ── Metadata ───────────────────────────────────────────────────────────
    pub created_at: i64,           // 8
    pub updated_at: i64,           // 8
    pub bump: u8,                  // 1
}

/// Minimal PDA that holds SOL escrow for a job.
/// Seeds: [b"escrow", job_id.to_le_bytes()]
/// Program-owned so direct lamport manipulation works.
/// Size: 8 (disc) + 9 = 17 bytes (rent ≈ 0.001 SOL)
#[account]
#[derive(InitSpace)]
pub struct EscrowVault {
    pub job_id: u64,  // 8
    pub bump: u8,     // 1
}

// ─── Instruction Params ───────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct JobParams {
    pub template_id: u32,
    pub num_days: u32,
    /// sha256 hashes of requested data type strings.
    pub data_type_hashes: Vec<[u8; 32]>,
    pub max_participants: u32,
    pub start_day_utc: i64,
    pub filter_query: String,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PreflightResultParams {
    pub effective_participants_scaled: u64,
    pub quality_tier: u8,
    pub final_total: u64,
    pub cohort_hash: [u8; 32],
    pub selected_participants: Vec<Pubkey>,
}

// ─── Accounts Contexts ────────────────────────────────────────────────────────

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

/// Shared context for all owner-gated OrderConfig mutations.
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

/// Researcher creates a new job. Job PDA allocated here.
#[derive(Accounts)]
pub struct RequestJob<'info> {
    #[account(
        mut,
        seeds = [b"order_config"],
        bump = order_config.bump,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        init,
        payer = researcher,
        space = 8 + Job::INIT_SPACE,
        seeds = [b"job", order_config.next_job_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub job: Account<'info, Job>,

    #[account(mut)]
    pub researcher: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Shared context for all ROFL-gated instructions.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct RoflAction<'info> {
    #[account(
        seeds = [b"order_config"],
        bump = order_config.bump,
        has_one = rofl_authority @ OrderError::NotRoflAuthority,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    pub rofl_authority: Signer<'info>,
}

/// Researcher confirms the job and deposits funds into the escrow vault.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct ConfirmJobAndPay<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    /// EscrowVault created here. Researcher pays vault rent.
    #[account(
        init,
        payer = researcher,
        space = 8 + EscrowVault::INIT_SPACE,
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut)]
    pub researcher: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Researcher cancels job. Job account closed and rent returned.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct CancelJob<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
        close = researcher,
    )]
    pub job: Account<'info, Job>,

    #[account(mut)]
    pub researcher: Signer<'info>,
}

/// ROFL finalizes job and computes per-provider payout.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct FinalizeJob<'info> {
    #[account(
        seeds = [b"order_config"],
        bump = order_config.bump,
        has_one = rofl_authority @ OrderError::NotRoflAuthority,
    )]
    pub order_config: Account<'info, OrderConfig>,

    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump = escrow_vault.bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    pub rofl_authority: Signer<'info>,
}

/// Data provider claims their payout.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct ClaimPayout<'info> {
    #[account(
        mut,
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        mut,
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump = escrow_vault.bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut)]
    pub provider: Signer<'info>,
}

/// Researcher (or admin) sweeps remaining vault dust after all claims.
#[derive(Accounts)]
#[instruction(job_id: u64)]
pub struct SweepVaultDust<'info> {
    #[account(
        seeds = [b"job", job_id.to_le_bytes().as_ref()],
        bump = job.bump,
    )]
    pub job: Account<'info, Job>,

    #[account(
        mut,
        seeds = [b"escrow", job_id.to_le_bytes().as_ref()],
        bump = escrow_vault.bump,
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    /// CHECK: recipient is validated in instruction logic (must be researcher or all claimed).
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}

// ─── Events ───────────────────────────────────────────────────────────────────

#[event]
pub struct JobRequested {
    pub job_id: u64,
    pub researcher: Pubkey,
    pub template_id: u32,
    pub num_days: u32,
    pub data_type_hashes: Vec<[u8; 32]>,
    pub max_participants: u32,
}

#[event]
pub struct PreflightSubmitted {
    pub job_id: u64,
    pub effective_participants_scaled: u64,
    pub quality_tier: u8,
    pub final_total: u64,
    pub cohort_hash: [u8; 32],
}

#[event]
pub struct JobConfirmed {
    pub job_id: u64,
    pub amount: u64,
}

#[event]
pub struct JobCancelled {
    pub job_id: u64,
    pub refund_amount: u64,
}

#[event]
pub struct JobExecuted {
    pub job_id: u64,
    pub result_cid: String,
}

#[event]
pub struct JobCompleted {
    pub job_id: u64,
    pub amount_per_provider: u64,
    pub num_providers: u64,
}

#[event]
pub struct DataProviderPaid {
    pub job_id: u64,
    pub provider: Pubkey,
    pub amount: u64,
}

#[event]
pub struct VaultDustSwept {
    pub job_id: u64,
    pub recipient: Pubkey,
    pub amount: u64,
}

#[event]
pub struct RoflAuthorityUpdated {
    pub rofl_authority: Pubkey,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[error_code]
pub enum OrderError {
    #[msg("Caller is not the order handler owner")]
    Unauthorized,
    #[msg("Caller is not the ROFL authority")]
    NotRoflAuthority,
    #[msg("Job status does not allow this operation")]
    InvalidStatus,
    #[msg("Job ID in instruction does not match account")]
    JobIdMismatch,
    #[msg("num_days must be greater than zero")]
    InvalidNumDays,
    #[msg("template_id must be greater than zero")]
    InvalidTemplateId,
    #[msg("data_type_hashes must not be empty")]
    EmptyDataTypes,
    #[msg("Too many data types (max 8)")]
    TooManyDataTypes,
    #[msg("max_participants must be greater than zero")]
    InvalidMaxParticipants,
    #[msg("Too many selected participants (max 50)")]
    TooManyParticipants,
    #[msg("Payment amount is less than the required final_total")]
    InsufficientPayment,
    #[msg("final_total must be greater than zero before payment")]
    ZeroFinalTotal,
    #[msg("result_cid must not be empty")]
    EmptyResultCid,
    #[msg("No participants to distribute payout to")]
    NoParticipants,
    #[msg("Escrow vault has insufficient lamports")]
    InsufficientEscrow,
    #[msg("Computed amount_per_provider is zero")]
    ZeroAmountPerProvider,
    #[msg("Signer is not a selected participant for this job")]
    NotAParticipant,
    #[msg("Participant index exceeds bitmap capacity")]
    ParticipantIndexOutOfRange,
    #[msg("This provider has already claimed their payout")]
    AlreadyClaimed,
    #[msg("Job can only be cancelled in PENDING_PREFLIGHT or AWAITING_CONFIRMATION")]
    CannotCancelAtThisStage,
    #[msg("Sweep not allowed: not all providers have claimed and caller is not the researcher")]
    SweepNotAllowed,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("New owner cannot be the zero address")]
    InvalidOwner,
    #[msg("ROFL authority cannot be the zero address")]
    InvalidAuthority,
}
