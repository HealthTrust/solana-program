// ============================================================================
// data_registry — HealthTrust Solana Migration
// ============================================================================
// Migrated from: DataStorage.sol
//
// Key changes vs EVM:
//
// 1. O(n²) scan ELIMINATED
//    EVM's updateUploadUnit scanned all metas and unitHistory arrays to find
//    which meta owned a given unitId. Here, UploadUnit PDA seeds embed meta_id
//    and unit_index, so the TEE caller simply passes both values and Anchor
//    validates the correct PDA is being accessed. Zero scanning.
//
// 2. Dynamic arrays replaced with bounded Vecs + fixed-size hashes
//    - string[] dataTypes  →  Vec<[u8;32]> data_type_hashes (sha256 of type string)
//    - uint8[] chronicConditions  →  Vec<u8> with max 16 entries
//    - Dynamic device/model/provider strings → #[max_len(N)] Anchor strings
//
// 3. string-keyed PricingConfig mapping replaced with PDA per sha256 hash
//    (see pricing_config program)
//
// 4. Pricing updates are NOT done via CPI in this version.
//    The data provider is expected to include pricing_config::update_multiplier_for_scarcity
//    instructions in the same transaction for each data type. This is idiomatic
//    Solana (composable instruction batching). A future version may use CPI.
//
// 5. EVM's Ownable/Pausable/ReentrancyGuard mapped to:
//    - owner field in RegistryState + has_one constraints
//    - paused flag checked at the top of mutating instructions
//    - Solana's single-writer account model eliminates reentrancy risk
//
// 6. tee_authority added to RegistryState for update_upload_unit.
//    EVM had a TODO comment with no auth check. Fixed here.
//
// 7. emergencyWithdrawERC20 removed — no ERC20 on Solana.
//    emergencyWithdrawEther → emergency_withdraw_sol (for accidentally-sent SOL only).
//
// 8. deleteMetaEntry → close_data_entry_meta (closes the account and reclaims rent).
// ============================================================================

use anchor_lang::prelude::*;

declare_id!("DREGxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
// Replace with: `anchor keys list` after running `anchor build`

// ─── Constants ───────────────────────────────────────────────────────────────

/// Max data types per metadata entry (bounded Vec of sha256 hashes).
pub const MAX_DATA_TYPES: usize = 8;

/// Max chronic condition codes per metadata entry.
pub const MAX_CHRONIC_CONDITIONS: usize = 16;

/// Max byte length for device_type string.
pub const MAX_DEVICE_TYPE_LEN: usize = 32;

/// Max byte length for device_model string.
pub const MAX_DEVICE_MODEL_LEN: usize = 48;

/// Max byte length for service_provider string.
pub const MAX_SERVICE_PROVIDER_LEN: usize = 48;

/// Max byte length for IPFS CID strings (CIDv1 base32 ≈ 59 chars).
pub const MAX_CID_LEN: usize = 64;

// ─── Program ─────────────────────────────────────────────────────────────────

#[program]
pub mod data_registry {
    use super::*;

    // ── Admin ──────────────────────────────────────────────────────────────

    /// One-time initialization. Caller becomes the owner.
    pub fn initialize_registry(
        ctx: Context<InitializeRegistry>,
        tee_authority: Pubkey,
    ) -> Result<()> {
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

    /// Update the pricing_config program address stored in RegistryState.
    /// Used for off-chain awareness and future CPI integration.
    pub fn set_pricing_program(
        ctx: Context<AuthorizedRegistryAction>,
        pricing_program: Pubkey,
    ) -> Result<()> {
        ctx.accounts.registry_state.pricing_program = pricing_program;
        Ok(())
    }

    /// Update the TEE/ROFL authority that is allowed to set feat_cid on upload units.
    pub fn set_tee_authority(
        ctx: Context<AuthorizedRegistryAction>,
        tee_authority: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(tee_authority, Pubkey::default(), RegistryError::InvalidAuthority);
        ctx.accounts.registry_state.tee_authority = tee_authority;
        Ok(())
    }

    /// Pause or unpause the registry. Mirrors Pausable.pause() / unpause().
    pub fn set_paused(ctx: Context<AuthorizedRegistryAction>, paused: bool) -> Result<()> {
        ctx.accounts.registry_state.paused = paused;
        emit!(PausedStateChanged { paused });
        Ok(())
    }

    /// Transfer registry ownership.
    pub fn transfer_registry_ownership(
        ctx: Context<AuthorizedRegistryAction>,
        new_owner: Pubkey,
    ) -> Result<()> {
        require_keys_neq!(new_owner, Pubkey::default(), RegistryError::InvalidOwner);
        let previous = ctx.accounts.registry_state.owner;
        ctx.accounts.registry_state.owner = new_owner;
        emit!(OwnershipTransferred { previous, new: new_owner });
        Ok(())
    }

    /// Emergency SOL withdrawal from the RegistryState account.
    /// Mirrors emergencyWithdrawEther. In practice the registry holds no SOL
    /// unless lamports were accidentally sent to its PDA.
    pub fn emergency_withdraw_sol(
        ctx: Context<EmergencyWithdrawSol>,
        amount: u64,
    ) -> Result<()> {
        ctx.accounts.registry_state.sub_lamports(amount)?;
        ctx.accounts.recipient.add_lamports(amount)?;
        emit!(EmergencyWithdrawal {
            recipient: ctx.accounts.recipient.key(),
            amount,
        });
        Ok(())
    }

    // ── Data Provider ──────────────────────────────────────────────────────

    /// Create a new DataEntryMeta + its first UploadUnit in one instruction.
    ///
    /// EVM equivalent: uploadNewMeta
    ///
    /// IMPORTANT CROSS-PROGRAM PRICING UPDATE:
    /// After this instruction, the client MUST include one
    /// pricing_config::update_multiplier_for_scarcity instruction per data type
    /// (using the total_duration value emitted in MetaEntryCreated) in the same
    /// transaction. This preserves atomicity without requiring CPI.
    ///
    /// Account sizing:
    ///   DataEntryMeta: 8 + ~497 bytes (see struct comment)
    ///   UploadUnit:    8 + ~173 bytes
    pub fn upload_new_meta(
        ctx: Context<UploadNewMeta>,
        params: UploadNewMetaParams,
    ) -> Result<()> {
        let state = &mut ctx.accounts.registry_state;
        require!(!state.paused, RegistryError::Paused);
        require!(!params.data_type_hashes.is_empty(), RegistryError::EmptyDataTypes);
        require!(
            params.data_type_hashes.len() <= MAX_DATA_TYPES,
            RegistryError::TooManyDataTypes
        );
        require!(
            params.chronic_conditions.len() <= MAX_CHRONIC_CONDITIONS,
            RegistryError::TooManyConditions
        );
        require!(
            params.day_end_timestamp > params.day_start_timestamp,
            RegistryError::InvalidTimestampRange
        );

        let meta_id = state.next_meta_id;
        let total_duration = params
            .day_end_timestamp
            .checked_sub(params.day_start_timestamp)
            .ok_or(RegistryError::Overflow)? as u64;

        let clock = Clock::get()?;

        // Populate DataEntryMeta
        let meta = &mut ctx.accounts.data_entry_meta;
        meta.meta_id = meta_id;
        meta.owner = ctx.accounts.provider.key();
        meta.device_type = params.device_type.clone();
        meta.device_model = params.device_model.clone();
        meta.service_provider = params.service_provider.clone();
        meta.age = params.age;
        meta.gender = params.gender;
        meta.height = params.height;
        meta.weight = params.weight;
        meta.region = params.region;
        meta.physical_activity_level = params.physical_activity_level;
        meta.smoker = params.smoker;
        meta.diet = params.diet;
        meta.chronic_conditions = params.chronic_conditions.clone();
        meta.data_type_hashes = params.data_type_hashes.clone();
        meta.total_duration = total_duration;
        meta.unit_count = 1; // first unit is index 0
        meta.date_of_creation = clock.unix_timestamp;
        meta.date_of_modification = clock.unix_timestamp;
        meta.bump = ctx.bumps.data_entry_meta;

        // Populate first UploadUnit (index 0)
        let unit = &mut ctx.accounts.upload_unit;
        unit.meta_id = meta_id;
        unit.unit_index = 0;
        unit.raw_cid = params.raw_cid.clone();
        unit.day_start_timestamp = params.day_start_timestamp;
        unit.day_end_timestamp = params.day_end_timestamp;
        unit.feat_cid = String::new();
        unit.date_of_creation = clock.unix_timestamp;
        unit.bump = ctx.bumps.upload_unit;

        // Advance global counter
        state.next_meta_id = state
            .next_meta_id
            .checked_add(1)
            .ok_or(RegistryError::Overflow)?;

        emit!(MetaEntryCreated {
            meta_id,
            owner: meta.owner,
            data_type_hashes: params.data_type_hashes.clone(),
            total_duration,
            date_of_creation: clock.unix_timestamp,
        });

        emit!(MetaAttributes {
            meta_id,
            age: params.age,
            gender: params.gender,
            height: params.height,
            weight: params.weight,
            region: params.region,
            physical_activity_level: params.physical_activity_level,
            smoker: params.smoker,
            diet: params.diet,
            chronic_conditions: params.chronic_conditions,
        });

        emit!(MetaDeviceInfo {
            meta_id,
            device_type: params.device_type,
            device_model: params.device_model,
            service_provider: params.service_provider,
        });

        emit!(UploadUnitCreated {
            meta_id,
            unit_index: 0,
            raw_cid: params.raw_cid,
            day_start_timestamp: params.day_start_timestamp,
            day_end_timestamp: params.day_end_timestamp,
            date_of_creation: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Append a new raw upload to an existing DataEntryMeta.
    ///
    /// EVM equivalent: registerRawUpload
    ///
    /// The new UploadUnit PDA is deterministically derived from
    /// (meta_id, meta.unit_count), so no global unit ID counter is needed.
    /// Caller must pass meta_id as instruction arg for seed derivation.
    pub fn register_raw_upload(
        ctx: Context<RegisterRawUpload>,
        _meta_id: u64,
        raw_cid: String,
        day_start_timestamp: i64,
        day_end_timestamp: i64,
    ) -> Result<()> {
        require!(
            !ctx.accounts.registry_state.paused,
            RegistryError::Paused
        );
        require!(
            day_end_timestamp > day_start_timestamp,
            RegistryError::InvalidTimestampRange
        );

        let added_duration = (day_end_timestamp - day_start_timestamp) as u64;
        let clock = Clock::get()?;
        let meta = &mut ctx.accounts.data_entry_meta;
        let unit_index = meta.unit_count; // next available index

        // Populate new UploadUnit
        let unit = &mut ctx.accounts.upload_unit;
        unit.meta_id = meta.meta_id;
        unit.unit_index = unit_index;
        unit.raw_cid = raw_cid.clone();
        unit.day_start_timestamp = day_start_timestamp;
        unit.day_end_timestamp = day_end_timestamp;
        unit.feat_cid = String::new();
        unit.date_of_creation = clock.unix_timestamp;
        unit.bump = ctx.bumps.upload_unit;

        // Update meta aggregates
        meta.unit_count = meta
            .unit_count
            .checked_add(1)
            .ok_or(RegistryError::Overflow)?;
        meta.total_duration = meta
            .total_duration
            .checked_add(added_duration)
            .ok_or(RegistryError::Overflow)?;
        meta.date_of_modification = clock.unix_timestamp;

        emit!(UploadUnitCreated {
            meta_id: meta.meta_id,
            unit_index,
            raw_cid: raw_cid.clone(),
            day_start_timestamp,
            day_end_timestamp,
            date_of_creation: clock.unix_timestamp,
        });

        emit!(DataStored {
            meta_id: meta.meta_id,
            unit_index,
            owner: meta.owner,
            raw_cid,
            day_start_timestamp,
            day_end_timestamp,
            total_duration: meta.total_duration,
            added_duration,
        });

        Ok(())
    }

    /// Set the feat_cid (processed/cleaned data CID) on an UploadUnit.
    ///
    /// EVM equivalent: updateUploadUnit
    ///
    /// EVM BUG FIXED: The EVM version had an O(n²) loop scanning all metas and
    /// unitHistory arrays to find which meta owned a given unitId. Here the caller
    /// passes (meta_id, unit_index) directly, and Anchor derives + validates the
    /// exact PDA. Zero scanning. The TEE is expected to know both values from its
    /// own job bookkeeping.
    ///
    /// ACCESS CONTROL FIX: EVM had a TODO with no auth check. Here only the
    /// tee_authority stored in RegistryState may call this.
    pub fn update_upload_unit(
        ctx: Context<UpdateUploadUnit>,
        _meta_id: u64,
        _unit_index: u32,
        feat_cid: String,
    ) -> Result<()> {
        require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);
        require!(!feat_cid.is_empty(), RegistryError::EmptyFeatCid);

        let clock = Clock::get()?;
        let unit = &mut ctx.accounts.upload_unit;
        unit.feat_cid = feat_cid.clone();

        emit!(DataEntryVersionUpdated {
            meta_id: unit.meta_id,
            unit_index: unit.unit_index,
            feat_cid,
            updater: ctx.accounts.tee_authority.key(),
            timestamp: clock.unix_timestamp,
        });
        Ok(())
    }

    /// Close (delete) a DataEntryMeta account and reclaim rent.
    /// Only the meta owner can call this. Does NOT close child UploadUnit accounts
    /// — those can be closed individually or in a follow-up sweep.
    ///
    /// EVM equivalent: deleteMetaEntry
    pub fn close_data_entry_meta(
        ctx: Context<CloseDataEntryMeta>,
        _meta_id: u64,
    ) -> Result<()> {
        require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);

        emit!(DataEntryDeleted {
            meta_id: ctx.accounts.data_entry_meta.meta_id,
            owner: ctx.accounts.provider.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        // Anchor's `close = provider` in the account constraint handles lamport return.
        Ok(())
    }

    /// Close a specific UploadUnit account and reclaim rent. Meta owner only.
    pub fn close_upload_unit(
        ctx: Context<CloseUploadUnit>,
        _meta_id: u64,
        _unit_index: u32,
    ) -> Result<()> {
        emit!(UploadUnitClosed {
            meta_id: ctx.accounts.upload_unit.meta_id,
            unit_index: ctx.accounts.upload_unit.unit_index,
        });
        Ok(())
    }
}

// ─── Account Structs ──────────────────────────────────────────────────────────

/// Global registry singleton.
/// Seeds: [b"registry_state"]
/// Size: 8 + 106 = 114 bytes
#[account]
#[derive(InitSpace)]
pub struct RegistryState {
    pub owner: Pubkey,           // 32
    /// Address of the pricing_config program (stored for client reference and future CPI).
    pub pricing_program: Pubkey, // 32
    /// Only this address can call update_upload_unit (sets feat_cid).
    pub tee_authority: Pubkey,   // 32
    pub next_meta_id: u64,       // 8 — monotonically increasing, starts at 1
    pub paused: bool,            // 1
    pub bump: u8,                // 1
}

/// One PDA per data provider's dataset grouping.
/// Seeds: [b"meta", meta_id.to_le_bytes()]
///
/// Size: 8 (disc) +
///   8 (meta_id) + 32 (owner) +
///   (4+32) device_type + (4+48) device_model + (4+48) service_provider +
///   1+1+1+1+1+1+1+1 (attributes) +
///   (4+16) chronic_conditions +
///   (4 + 8*32) data_type_hashes +
///   8 (total_duration) + 4 (unit_count) + 8 + 8 (timestamps) + 1 (bump)
/// = 8 + 497 = 505 bytes
#[account]
#[derive(InitSpace)]
pub struct DataEntryMeta {
    pub meta_id: u64,                    // 8
    pub owner: Pubkey,                   // 32

    #[max_len(32)]
    pub device_type: String,             // 4 + 32
    #[max_len(48)]
    pub device_model: String,            // 4 + 48
    #[max_len(48)]
    pub service_provider: String,        // 4 + 48

    // Attributes (all u8 enum codes — matches EVM Attributes struct)
    pub age: u8,
    pub gender: u8,
    pub height: u8,
    pub weight: u8,
    pub region: u8,
    pub physical_activity_level: u8,
    pub smoker: u8,
    pub diet: u8,

    /// Chronic condition codes. EVM used uint8[]; bounded to MAX_CHRONIC_CONDITIONS.
    #[max_len(16)]
    pub chronic_conditions: Vec<u8>,     // 4 + 16

    /// sha256 hashes of data type strings. EVM used string[]; replaced for PDA safety.
    /// The human-readable strings live in TypeMultiplier accounts on pricing_config.
    #[max_len(8)]
    pub data_type_hashes: Vec<[u8; 32]>, // 4 + 8*32 = 260

    /// Cumulative seconds of data in this meta (sum of all unit durations).
    pub total_duration: u64,             // 8
    /// Number of UploadUnit PDAs created under this meta.
    /// The next unit will have unit_index = unit_count before the increment.
    pub unit_count: u32,                 // 4

    pub date_of_creation: i64,           // 8
    pub date_of_modification: i64,       // 8
    pub bump: u8,                        // 1
}

/// One PDA per upload, scoped to a meta. Seeds embed meta_id + unit_index.
/// Seeds: [b"unit", meta_id.to_le_bytes(), unit_index.to_le_bytes()]
///
/// DESIGN NOTE: Embedding meta_id in the seed means the TEE always knows
/// exactly which account to write to (it knows both meta_id and unit_index
/// from job bookkeeping). No scan required. This eliminates the EVM O(n²) bug.
///
/// Size: 8 (disc) + 8 + 4 + (4+64) + 8 + 8 + (4+64) + 8 + 1 = 181 bytes
#[account]
#[derive(InitSpace)]
pub struct UploadUnit {
    pub meta_id: u64,              // 8
    pub unit_index: u32,           // 4

    #[max_len(64)]
    pub raw_cid: String,           // 4 + 64 — IPFS CID of raw data blob

    pub day_start_timestamp: i64,  // 8
    pub day_end_timestamp: i64,    // 8

    /// Set later by the TEE after processing. Empty string until then.
    #[max_len(64)]
    pub feat_cid: String,          // 4 + 64 — IPFS CID of processed feature pack

    pub date_of_creation: i64,     // 8
    pub bump: u8,                  // 1
}

// ─── Instruction Params ───────────────────────────────────────────────────────

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UploadNewMetaParams {
    pub raw_cid: String,
    /// sha256 hashes of data type strings (computed off-chain)
    pub data_type_hashes: Vec<[u8; 32]>,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    // Attributes
    pub age: u8,
    pub gender: u8,
    pub height: u8,
    pub weight: u8,
    pub region: u8,
    pub physical_activity_level: u8,
    pub smoker: u8,
    pub diet: u8,
    pub chronic_conditions: Vec<u8>,
}

// ─── Accounts Contexts ────────────────────────────────────────────────────────

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

/// Shared context for all owner-gated RegistryState mutations.
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

/// Create a new DataEntryMeta and its first UploadUnit.
///
/// PDA seeds use registry_state.next_meta_id at the time the instruction
/// executes. Anchor resolves the field value when building the instruction
/// discriminant, so the account is created at the correct address.
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

    /// First unit always has index 0.
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

/// Append a new raw upload to an existing meta. Caller passes meta_id as arg.
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

    /// New unit at index = data_entry_meta.unit_count (before increment).
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

/// Set feat_cid on an existing UploadUnit. TEE authority only.
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

/// Close a DataEntryMeta account and return rent to the provider.
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

/// Close a single UploadUnit account. Meta owner only.
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

/// Emergency SOL withdrawal from the RegistryState PDA. Owner only.
#[derive(Accounts)]
pub struct EmergencyWithdrawSol<'info> {
    #[account(
        mut,
        seeds = [b"registry_state"],
        bump = registry_state.bump,
        has_one = owner @ RegistryError::Unauthorized,
    )]
    pub registry_state: Account<'info, RegistryState>,

    /// CHECK: arbitrary recipient — owner's responsibility to specify correctly.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,

    pub owner: Signer<'info>,
}

// ─── Events ───────────────────────────────────────────────────────────────────
// Split across multiple events (same strategy as EVM to avoid stack-too-deep).

#[event]
pub struct MetaEntryCreated {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub data_type_hashes: Vec<[u8; 32]>,
    pub total_duration: u64,
    pub date_of_creation: i64,
}

#[event]
pub struct MetaAttributes {
    pub meta_id: u64,
    pub age: u8,
    pub gender: u8,
    pub height: u8,
    pub weight: u8,
    pub region: u8,
    pub physical_activity_level: u8,
    pub smoker: u8,
    pub diet: u8,
    pub chronic_conditions: Vec<u8>,
}

#[event]
pub struct MetaDeviceInfo {
    pub meta_id: u64,
    pub device_type: String,
    pub device_model: String,
    pub service_provider: String,
}

#[event]
pub struct UploadUnitCreated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub date_of_creation: i64,
}

#[event]
pub struct DataStored {
    pub meta_id: u64,
    pub unit_index: u32,
    pub owner: Pubkey,
    pub raw_cid: String,
    pub day_start_timestamp: i64,
    pub day_end_timestamp: i64,
    pub total_duration: u64,
    pub added_duration: u64,
}

#[event]
pub struct DataEntryVersionUpdated {
    pub meta_id: u64,
    pub unit_index: u32,
    pub feat_cid: String,
    pub updater: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct DataEntryDeleted {
    pub meta_id: u64,
    pub owner: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct UploadUnitClosed {
    pub meta_id: u64,
    pub unit_index: u32,
}

#[event]
pub struct PausedStateChanged {
    pub paused: bool,
}

#[event]
pub struct OwnershipTransferred {
    pub previous: Pubkey,
    pub new: Pubkey,
}

#[event]
pub struct EmergencyWithdrawal {
    pub recipient: Pubkey,
    pub amount: u64,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[error_code]
pub enum RegistryError {
    #[msg("Registry is paused")]
    Paused,
    #[msg("Caller is not the registry owner")]
    Unauthorized,
    #[msg("Caller is not the meta entry owner")]
    NotOwner,
    #[msg("Caller is not the TEE authority")]
    NotTeeAuthority,
    #[msg("data_type_hashes must not be empty")]
    EmptyDataTypes,
    #[msg("Too many data types (max 8)")]
    TooManyDataTypes,
    #[msg("Too many chronic conditions (max 16)")]
    TooManyConditions,
    #[msg("day_end_timestamp must be greater than day_start_timestamp")]
    InvalidTimestampRange,
    #[msg("feat_cid must not be empty")]
    EmptyFeatCid,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("New owner cannot be the zero address")]
    InvalidOwner,
    #[msg("TEE authority cannot be the zero address")]
    InvalidAuthority,
}
