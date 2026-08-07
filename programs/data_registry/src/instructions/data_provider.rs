use anchor_lang::prelude::*;

use crate::constants::{
    MAX_CHRONIC_CONDITIONS, MAX_DATA_TYPE_LEN, MAX_DATA_TYPES, MAX_DIGEST_VERSION_LEN,
    MAX_QUALITY_SIGNALS, MAX_SIGNAL_NAME_LEN,
};
use crate::contexts::{
    CloseDataEntryMeta, CloseUploadUnit, RegisterRawUpload, UpdateMetaDataTypes, UpdateUploadUnit,
    UploadNewMeta,
};
use crate::errors::RegistryError;
use crate::events::{
    DataEntryDeleted, DataEntryVersionUpdated, DataStored, MetaAttributes, MetaDataTypesUpdated,
    MetaDeviceInfo, MetaEntryCreated, SignalQuality, UploadUnitClosed, UploadUnitCreated,
};
use crate::params::UploadNewMetaParams;
use crate::state::UploadUnit;

fn initialize_upload_unit(
    unit: &mut UploadUnit,
    meta_id: u64,
    unit_index: u32,
    raw_cid: String,
    day_start_timestamp: i64,
    day_end_timestamp: i64,
    date_of_creation: i64,
    bump: u8,
) {
    unit.meta_id = meta_id;
    unit.unit_index = unit_index;
    unit.raw_cid = raw_cid;
    unit.day_start_timestamp = day_start_timestamp;
    unit.day_end_timestamp = day_end_timestamp;
    unit.feat_cid = String::new();
    unit.date_of_creation = date_of_creation;
    unit.bump = bump;
}

/// Bounds the `data_types` vector: non-empty, at most `MAX_DATA_TYPES` entries,
/// and every entry a non-empty string of at most `MAX_DATA_TYPE_LEN` bytes.
///
/// Kept as a free function (rather than inline `require!`s) so the cap boundary
/// is unit-testable without spinning up a validator — the account SPACE math in
/// `DataEntryMeta` mirrors these same constants and must stay in lockstep.
pub fn validate_data_types(data_types: &[String]) -> Result<()> {
    require!(!data_types.is_empty(), RegistryError::EmptyDataTypes);
    require!(
        data_types.len() <= MAX_DATA_TYPES,
        RegistryError::TooManyDataTypes
    );
    require!(
        data_types
            .iter()
            .all(|data_type| !data_type.is_empty() && data_type.len() <= MAX_DATA_TYPE_LEN),
        RegistryError::TooManyDataTypes
    );
    Ok(())
}

pub fn upload_new_meta(ctx: Context<UploadNewMeta>, params: UploadNewMetaParams) -> Result<()> {
    let state = &mut ctx.accounts.registry_state;
    require!(!state.paused, RegistryError::Paused);
    validate_data_types(&params.data_types)?;
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
    meta.data_types = params.data_types.clone();
    meta.total_duration = total_duration;
    meta.unit_count = 1;
    meta.open_unit_count = 1;
    meta.date_of_creation = clock.unix_timestamp;
    meta.date_of_modification = clock.unix_timestamp;
    meta.bump = ctx.bumps.data_entry_meta;

    let unit = &mut ctx.accounts.upload_unit;
    initialize_upload_unit(
        unit,
        meta_id,
        0,
        params.raw_cid.clone(),
        params.day_start_timestamp,
        params.day_end_timestamp,
        clock.unix_timestamp,
        ctx.bumps.upload_unit,
    );

    state.next_meta_id = state
        .next_meta_id
        .checked_add(1)
        .ok_or(RegistryError::Overflow)?;

    emit!(MetaEntryCreated {
        meta_id,
        owner: meta.owner,
        data_types: params.data_types.clone(),
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

/// Grow-only union of a meta's declared `data_types` with a caller-supplied set
/// (issue #44: `dataTypes` is the OBSERVED metric set, so it must be able to
/// grow in place when the app first sees rows for an undeclared metric).
///
/// Retraction is unrepresentable by construction rather than merely rejected:
/// the result *starts* as the existing vector and is only ever pushed to, so no
/// argument — a strict subset, a permutation, a disjoint set — can drop a
/// declared entry. There is no code path that writes a shorter `data_types`.
///
/// Existing entries keep their original index order; genuinely new entries
/// append in caller order, deduplicated against the union built so far (so a
/// caller repeating a name within one call still adds it once).
///
/// The merged result is re-validated with `validate_data_types`, so growth is
/// bounded by exactly the caps that bound `upload_new_meta` — never a looser
/// set. Per-entry byte bounds are also checked on the input first, so a caller
/// that submits an oversized name is told so even when the union happens to sit
/// at the cap.
///
/// Kept as a free function so the invariant is unit-testable without a
/// validator.
pub fn merge_data_types(existing: &[String], new_data_types: &[String]) -> Result<Vec<String>> {
    require!(!new_data_types.is_empty(), RegistryError::EmptyDataTypes);
    require!(
        new_data_types
            .iter()
            .all(|data_type| !data_type.is_empty() && data_type.len() <= MAX_DATA_TYPE_LEN),
        RegistryError::TooManyDataTypes
    );

    let mut merged = existing.to_vec();
    for data_type in new_data_types.iter() {
        if !merged.iter().any(|declared| declared == data_type) {
            merged.push(data_type.clone());
        }
    }

    validate_data_types(&merged)?;
    Ok(merged)
}

/// Extends a meta's declared `data_types` in place. Owner-signed, grow-only,
/// and capped identically to creation (see `merge_data_types`).
///
/// No-op calls — every submitted name already declared — succeed idempotently:
/// the account is left byte-identical (not even `date_of_modification` moves,
/// since nothing about the dataset changed) and the event carries an empty
/// `added`. The Expo app derives the observed set on every backfill, so
/// re-submitting an unchanged set is the common case, not an error; erroring
/// would force the client to keep a chain-accurate mirror just to stay quiet.
/// An EMPTY `new_data_types` vector is still rejected (`EmptyDataTypes`),
/// matching `upload_new_meta` — it carries no intent.
pub fn update_meta_data_types(
    ctx: Context<UpdateMetaDataTypes>,
    _meta_id: u64,
    new_data_types: Vec<String>,
) -> Result<()> {
    require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);

    let clock = Clock::get()?;
    let meta = &mut ctx.accounts.data_entry_meta;

    let previous_len = meta.data_types.len();
    let merged = merge_data_types(&meta.data_types, &new_data_types)?;
    // `merged` is `existing` followed by the appended entries, so the tail past
    // the original length is exactly this call's delta.
    let added = merged[previous_len..].to_vec();

    if !added.is_empty() {
        meta.data_types = merged.clone();
        meta.date_of_modification = clock.unix_timestamp;
    }

    emit!(MetaDataTypesUpdated {
        meta_id: meta.meta_id,
        owner: meta.owner,
        data_types: merged,
        added,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

pub fn register_raw_upload(
    ctx: Context<RegisterRawUpload>,
    _meta_id: u64,
    raw_cid: String,
    day_start_timestamp: i64,
    day_end_timestamp: i64,
) -> Result<()> {
    require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);
    require!(
        day_end_timestamp > day_start_timestamp,
        RegistryError::InvalidTimestampRange
    );

    let added_duration = (day_end_timestamp - day_start_timestamp) as u64;
    let clock = Clock::get()?;
    let meta = &mut ctx.accounts.data_entry_meta;
    let unit_index = meta.unit_count;

    let unit = &mut ctx.accounts.upload_unit;
    initialize_upload_unit(
        unit,
        meta.meta_id,
        unit_index,
        raw_cid.clone(),
        day_start_timestamp,
        day_end_timestamp,
        clock.unix_timestamp,
        ctx.bumps.upload_unit,
    );

    meta.unit_count = meta
        .unit_count
        .checked_add(1)
        .ok_or(RegistryError::Overflow)?;
    meta.open_unit_count = meta
        .open_unit_count
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

/// Validates the event-carried quality digest (QUALITY_DIGEST_DESIGN.md §4/§5,
/// COHORT_IMPL_CONTRACT.md §3). The digest is emitted only; no state write, so
/// its ceiling is the transaction size limit rather than account space.
///
/// Kept as a free function so the cap boundary is unit-testable without a
/// validator.
pub fn validate_quality_digest(
    quality: &[SignalQuality],
    signal_table_version: &str,
    extraction_version: &str,
) -> Result<()> {
    require!(
        quality.len() <= MAX_QUALITY_SIGNALS,
        RegistryError::TooManyQualitySignals
    );
    require!(
        signal_table_version.len() <= MAX_DIGEST_VERSION_LEN
            && extraction_version.len() <= MAX_DIGEST_VERSION_LEN,
        RegistryError::VersionStringTooLong
    );
    for sq in quality.iter() {
        require!(
            !sq.signal.is_empty() && sq.signal.len() <= MAX_SIGNAL_NAME_LEN,
            RegistryError::InvalidSignalName
        );
        // Independent counts (see contract §3): do NOT require
        // total_samples >= valid_samples.
        require!(
            sq.outlier_count <= sq.total_samples,
            RegistryError::InvalidOutlierCount
        );
        // Only enforce ordering when both timestamps are set.
        require!(
            sq.first_ts == 0 || sq.last_ts == 0 || sq.first_ts <= sq.last_ts,
            RegistryError::InvalidDigestTimestamps
        );
    }
    Ok(())
}

pub fn update_upload_unit(
    ctx: Context<UpdateUploadUnit>,
    _meta_id: u64,
    _unit_index: u32,
    feat_cid: String,
    quality: Vec<SignalQuality>,
    signal_table_version: String,
    extraction_version: String,
    digest_schema_version: u8,
) -> Result<()> {
    require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);
    require!(!feat_cid.is_empty(), RegistryError::EmptyFeatCid);

    validate_quality_digest(&quality, &signal_table_version, &extraction_version)?;

    let clock = Clock::get()?;
    let unit = &mut ctx.accounts.upload_unit;
    unit.feat_cid = feat_cid.clone();

    emit!(DataEntryVersionUpdated {
        meta_id: unit.meta_id,
        unit_index: unit.unit_index,
        feat_cid,
        updater: ctx.accounts.tee_authority.key(),
        timestamp: clock.unix_timestamp,
        quality,
        signal_table_version,
        extraction_version,
        digest_schema_version,
    });
    Ok(())
}

/// Withdraw a dataset (issue #4): the meta can only close once every one of
/// its upload units is closed, so withdrawn data can never linger as orphaned
/// on-chain units.
///
/// Unit PDAs of this meta may be passed as remaining accounts (writable) and
/// are cascade-closed here — rent refunded to the provider, UploadUnitClosed
/// emitted per unit, `open_unit_count` decremented. When the meta has more
/// units than fit one transaction (~25 account keys), pre-close batches with
/// close_upload_unit across as many transactions as needed; the final call
/// then closes the meta. If any unit is still open, the close is refused
/// (UnitsStillOpen) — completeness is enforced by the counter, not by the
/// caller's account list.
pub fn close_data_entry_meta<'info>(
    ctx: Context<'_, '_, 'info, 'info, CloseDataEntryMeta<'info>>,
    meta_id: u64,
) -> Result<()> {
    require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);

    let provider_info = ctx.accounts.provider.to_account_info();
    let mut open = ctx.accounts.data_entry_meta.open_unit_count;

    for acc_info in ctx.remaining_accounts.iter() {
        // Deserialization enforces program ownership + the UploadUnit
        // discriminator; meta_id enforces it is THIS meta's unit. Duplicates
        // can't slip through — a closed account fails deserialization.
        let unit: Account<'info, UploadUnit> = Account::try_from(acc_info)?;
        require!(unit.meta_id == meta_id, RegistryError::WrongUnitAccount);

        emit!(UploadUnitClosed {
            meta_id,
            unit_index: unit.unit_index,
        });
        unit.close(provider_info.clone())?;
        open = open.checked_sub(1).ok_or(RegistryError::Overflow)?;
    }

    require!(open == 0, RegistryError::UnitsStillOpen);
    ctx.accounts.data_entry_meta.open_unit_count = 0;

    emit!(DataEntryDeleted {
        meta_id: ctx.accounts.data_entry_meta.meta_id,
        owner: ctx.accounts.provider.key(),
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

pub fn close_upload_unit(
    ctx: Context<CloseUploadUnit>,
    _meta_id: u64,
    _unit_index: u32,
) -> Result<()> {
    let meta = &mut ctx.accounts.data_entry_meta;
    meta.open_unit_count = meta
        .open_unit_count
        .checked_sub(1)
        .ok_or(RegistryError::Overflow)?;

    emit!(UploadUnitClosed {
        meta_id: ctx.accounts.upload_unit.meta_id,
        unit_index: ctx.accounts.upload_unit.unit_index,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DataEntryMeta;
    use anchor_lang::Space;

    fn types(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("metric_{i}")).collect()
    }

    fn error_code(err: anchor_lang::error::Error) -> u32 {
        match err {
            anchor_lang::error::Error::AnchorError(inner) => inner.error_code_number,
            other => panic!("expected an AnchorError, got {other:?}"),
        }
    }

    /// Explicit `u32` conversion. Under the `idl-build` feature `serde_json` is
    /// in scope and a bare `.into()` becomes ambiguous, so pin the target type.
    fn expected_code(err: RegistryError) -> u32 {
        err.into()
    }

    #[test]
    fn cap_is_eighteen() {
        assert_eq!(MAX_DATA_TYPES, 18);
        assert_eq!(MAX_DATA_TYPE_LEN, 32);
    }

    #[test]
    fn accepts_up_to_the_cap() {
        for n in 1..=MAX_DATA_TYPES {
            assert!(
                validate_data_types(&types(n)).is_ok(),
                "{n} data types should be accepted"
            );
        }
    }

    #[test]
    fn rejects_one_over_the_cap() {
        let err = validate_data_types(&types(MAX_DATA_TYPES + 1))
            .expect_err("19 data types must be rejected");
        assert_eq!(
            error_code(err),
            expected_code(RegistryError::TooManyDataTypes),
            "over-cap vectors must fail with TooManyDataTypes"
        );
    }

    #[test]
    fn rejects_empty_vector() {
        let err = validate_data_types(&[]).expect_err("empty data_types must be rejected");
        assert_eq!(error_code(err), expected_code(RegistryError::EmptyDataTypes));
    }

    #[test]
    fn enforces_per_string_byte_limit() {
        let at_limit = vec!["a".repeat(MAX_DATA_TYPE_LEN)];
        assert!(validate_data_types(&at_limit).is_ok(), "32 bytes is allowed");

        let over_limit = vec!["a".repeat(MAX_DATA_TYPE_LEN + 1)];
        let err = validate_data_types(&over_limit).expect_err("33 bytes must be rejected");
        assert_eq!(error_code(err), expected_code(RegistryError::TooManyDataTypes));

        let empty_entry = vec![String::new()];
        assert!(
            validate_data_types(&empty_entry).is_err(),
            "empty strings must be rejected"
        );
    }

    /// Guards the classic footgun: bumping `MAX_DATA_TYPES` but forgetting the
    /// `#[max_len(18, 32)]` on `DataEntryMeta::data_types`, which would let
    /// validation accept 18 entries that then fail to serialize into the
    /// account allocated at `init`.
    #[test]
    fn account_space_covers_the_cap() {
        // 4-byte Vec length prefix + N * (4-byte String length prefix + bytes).
        let data_types_space = 4 + MAX_DATA_TYPES * (4 + MAX_DATA_TYPE_LEN);
        assert_eq!(data_types_space, 652);

        let fixed = 8  // meta_id
            + 32       // owner
            + (4 + 32) // device_type
            + (4 + 48) // device_model
            + (4 + 48) // service_provider
            + 8        // age..diet, eight u8 attributes
            + (4 + 16) // chronic_conditions
            + 8        // total_duration
            + 4        // unit_count
            + 4        // open_unit_count
            + 8        // date_of_creation
            + 8        // date_of_modification
            + 1; // bump

        assert_eq!(
            DataEntryMeta::INIT_SPACE,
            fixed + data_types_space,
            "DataEntryMeta::INIT_SPACE must budget {MAX_DATA_TYPES} data types"
        );
    }
}

/// Issue #44 — `update_meta_data_types`. The invariant under test is that a
/// declared metric can never be retracted, so most of these assert on what the
/// merge REFUSES to lose rather than on what it adds.
#[cfg(test)]
mod merge_tests {
    use super::*;

    fn set(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn error_code(err: anchor_lang::error::Error) -> u32 {
        match err {
            anchor_lang::error::Error::AnchorError(inner) => inner.error_code_number,
            other => panic!("expected an AnchorError, got {other:?}"),
        }
    }

    /// Explicit `u32` conversion. Under the `idl-build` feature `serde_json` is
    /// in scope and a bare `.into()` becomes ambiguous, so pin the target type.
    fn expected_code(err: RegistryError) -> u32 {
        err.into()
    }

    #[test]
    fn unions_new_entries_after_the_existing_ones() {
        let merged = merge_data_types(&set(&["steps"]), &set(&["heart_rate", "sleep"]))
            .expect("union must be accepted");
        assert_eq!(merged, set(&["steps", "heart_rate", "sleep"]));
    }

    #[test]
    fn existing_entries_keep_their_index_order() {
        // Downstream diffs the declared set positionally; a reordering caller
        // must not permute what is already on chain.
        let existing = set(&["steps", "sleep", "glucose"]);
        let merged = merge_data_types(&existing, &set(&["glucose", "sleep", "spo2", "steps"]))
            .expect("reordered input must be accepted");
        assert_eq!(merged, set(&["steps", "sleep", "glucose", "spo2"]));
        assert_eq!(
            merged[..existing.len()],
            existing[..],
            "the existing prefix must survive verbatim"
        );
    }

    #[test]
    fn duplicate_calls_are_idempotent() {
        let existing = set(&["steps", "heart_rate"]);
        let once = merge_data_types(&existing, &set(&["sleep"])).unwrap();
        let twice = merge_data_types(&once, &set(&["sleep"])).unwrap();
        assert_eq!(once, twice);

        // A no-op call (everything already declared) succeeds and changes
        // nothing — it must not error.
        let noop = merge_data_types(&existing, &set(&["heart_rate", "steps"]))
            .expect("a fully redundant call must succeed idempotently");
        assert_eq!(noop, existing);
    }

    #[test]
    fn repeated_names_inside_one_call_are_deduplicated() {
        let merged = merge_data_types(&set(&["steps"]), &set(&["sleep", "sleep", "sleep"])).unwrap();
        assert_eq!(merged, set(&["steps", "sleep"]));
    }

    /// The core grow-only guarantee. A caller cannot express retraction: a
    /// strict subset, a disjoint set, and a single-entry set all leave every
    /// previously declared metric in place.
    #[test]
    fn retraction_is_unrepresentable() {
        let existing = set(&["steps", "heart_rate", "sleep", "glucose"]);

        for attempt in [
            set(&["steps"]),                    // strict subset
            set(&["steps", "sleep"]),           // strict subset, reordered
            set(&["spo2"]),                     // disjoint, shorter
            set(&["heart_rate", "heart_rate"]), // duplicated subset
        ] {
            let merged = merge_data_types(&existing, &attempt)
                .expect("a shrinking argument is not an error, it just cannot shrink");
            for declared in existing.iter() {
                assert!(
                    merged.contains(declared),
                    "{declared} was dropped by {attempt:?}"
                );
            }
            assert!(
                merged.len() >= existing.len(),
                "the declared set must never get smaller"
            );
            assert_eq!(
                merged[..existing.len()],
                existing[..],
                "the existing prefix must survive verbatim"
            );
        }
    }

    #[test]
    fn rejects_an_empty_argument() {
        let err = merge_data_types(&set(&["steps"]), &[])
            .expect_err("an empty new_data_types carries no intent");
        assert_eq!(error_code(err), expected_code(RegistryError::EmptyDataTypes));
    }

    #[test]
    fn rejects_a_union_over_the_cap() {
        let existing: Vec<String> = (0..MAX_DATA_TYPES).map(|i| format!("metric_{i}")).collect();

        // Already at the cap: a redundant call still succeeds...
        assert!(
            merge_data_types(&existing, &set(&["metric_0"])).is_ok(),
            "a no-op call at the cap must not be rejected"
        );

        // ...but one genuinely new entry pushes the union to 19.
        let err = merge_data_types(&existing, &set(&["one_too_many"]))
            .expect_err("a 19-entry union must be rejected");
        assert_eq!(
            error_code(err),
            expected_code(RegistryError::TooManyDataTypes)
        );

        // The cap bounds the RESULT, not the argument: 17 declared + 2 new = 19.
        let seventeen: Vec<String> = (0..MAX_DATA_TYPES - 1)
            .map(|i| format!("metric_{i}"))
            .collect();
        assert!(
            merge_data_types(&seventeen, &set(&["extra_a"])).is_ok(),
            "17 + 1 = 18 is exactly at the cap"
        );
        assert_eq!(
            error_code(merge_data_types(&seventeen, &set(&["extra_a", "extra_b"])).unwrap_err()),
            expected_code(RegistryError::TooManyDataTypes),
            "17 + 2 = 19 must be rejected"
        );
    }

    #[test]
    fn enforces_the_per_name_byte_limit() {
        let at_limit = "a".repeat(MAX_DATA_TYPE_LEN);
        assert!(
            merge_data_types(&set(&["steps"]), &[at_limit.clone()]).is_ok(),
            "32 bytes is allowed"
        );

        let over_limit = "a".repeat(MAX_DATA_TYPE_LEN + 1);
        assert_eq!(
            error_code(merge_data_types(&set(&["steps"]), &[over_limit]).unwrap_err()),
            expected_code(RegistryError::TooManyDataTypes),
            "33 bytes must be rejected"
        );

        assert_eq!(
            error_code(merge_data_types(&set(&["steps"]), &[String::new()]).unwrap_err()),
            expected_code(RegistryError::TooManyDataTypes),
            "empty names must be rejected"
        );
    }

    /// A grown meta must still fit the account it lives in. `DataEntryMeta` is
    /// allocated at `INIT_SPACE`, which budgets the cap, and the
    /// `UpdateMetaDataTypes` context reallocs legacy (pre-PR-#9) metas up to
    /// that same size — so any union the merge accepts is serializable.
    #[test]
    fn a_capped_union_fits_the_meta_account() {
        let existing = set(&["steps"]);
        // Distinct names at exactly MAX_DATA_TYPE_LEN bytes — the worst case a
        // union can reach: a 2-digit prefix plus 30 filler bytes.
        let new: Vec<String> = (0..MAX_DATA_TYPES - 1)
            .map(|i| format!("{i:02}{}", "a".repeat(MAX_DATA_TYPE_LEN - 2)))
            .collect();

        let merged = merge_data_types(&existing, &new).expect("18 entries is at the cap");
        assert_eq!(merged.len(), MAX_DATA_TYPES);

        let serialized = 4 + merged.iter().map(|s| 4 + s.len()).sum::<usize>();
        let budget = 4 + MAX_DATA_TYPES * (4 + MAX_DATA_TYPE_LEN);
        assert!(
            serialized <= budget,
            "worst-case union ({serialized} B) must fit the {budget} B budget in INIT_SPACE"
        );
    }

    /// `new_data_types` rides in instruction data, so — like the quality digest
    /// — its real ceiling is the 1232-byte transaction limit, not account
    /// space. Unlike the digest, a full-cap argument DOES fit at maximum name
    /// length, because this instruction carries nothing else of size.
    #[test]
    fn a_full_cap_argument_fits_in_a_transaction() {
        // 1 signature (65) + header (3) + 5 account keys (161: registry_state,
        // data_entry_meta, provider, system_program, program id) + blockhash
        // (32) + instruction framing (8) + discriminator (8) + meta_id (8).
        const NON_ARG_BYTES: usize = 65 + 3 + 161 + 32 + 8 + 8 + 8;
        const TX_LIMIT: usize = 1232;

        // Borsh: 4-byte Vec prefix + per entry (4-byte prefix + name bytes).
        let arg_bytes = 4 + MAX_DATA_TYPES * (4 + MAX_DATA_TYPE_LEN);
        let worst = NON_ARG_BYTES + arg_bytes;
        assert!(
            worst <= TX_LIMIT,
            "a full {MAX_DATA_TYPES}-entry argument at {MAX_DATA_TYPE_LEN} bytes ({worst} B) \
             must fit the {TX_LIMIT}-byte transaction limit"
        );
    }
}

#[cfg(test)]
mod digest_tests {
    use super::*;

    /// Explicit `u32` conversion. Under the `idl-build` feature `serde_json` is
    /// in scope and a bare `.into()` becomes ambiguous, so pin the target type.
    fn expected_code(err: RegistryError) -> u32 {
        err.into()
    }

    fn error_code(err: anchor_lang::error::Error) -> u32 {
        match err {
            anchor_lang::error::Error::AnchorError(inner) => inner.error_code_number,
            other => panic!("expected an AnchorError, got {other:?}"),
        }
    }

    fn signal(name: &str) -> SignalQuality {
        SignalQuality {
            signal: name.to_string(),
            valid_samples: 100,
            total_samples: 120,
            outlier_count: 5,
            longest_gap_seconds: 60,
            first_ts: 1_713_916_800,
            last_ts: 1_714_003_200,
        }
    }

    fn signals(n: usize) -> Vec<SignalQuality> {
        (0..n).map(|i| signal(&format!("signal_{i}"))).collect()
    }

    fn validate(quality: &[SignalQuality]) -> Result<()> {
        validate_quality_digest(quality, "v1", "v1")
    }

    #[test]
    fn cap_is_eighteen() {
        assert_eq!(MAX_QUALITY_SIGNALS, 18);
    }

    #[test]
    fn accepts_up_to_the_cap() {
        for n in 0..=MAX_QUALITY_SIGNALS {
            assert!(validate(&signals(n)).is_ok(), "{n} signals should pass");
        }
    }

    #[test]
    fn rejects_one_over_the_cap() {
        let err = validate(&signals(MAX_QUALITY_SIGNALS + 1))
            .expect_err("19 signals must be rejected");
        assert_eq!(
            error_code(err),
            expected_code(RegistryError::TooManyQualitySignals)
        );
    }

    #[test]
    fn still_validates_each_entry() {
        assert_eq!(
            error_code(validate(&[signal("")]).unwrap_err()),
            expected_code(RegistryError::InvalidSignalName)
        );

        let long_name = "s".repeat(MAX_SIGNAL_NAME_LEN + 1);
        assert_eq!(
            error_code(validate(&[signal(&long_name)]).unwrap_err()),
            expected_code(RegistryError::InvalidSignalName)
        );

        let mut bad_outliers = signal("heart_rate");
        bad_outliers.outlier_count = bad_outliers.total_samples + 1;
        assert_eq!(
            error_code(validate(&[bad_outliers]).unwrap_err()),
            expected_code(RegistryError::InvalidOutlierCount)
        );

        let mut reversed = signal("heart_rate");
        reversed.first_ts = 200;
        reversed.last_ts = 100;
        assert_eq!(
            error_code(validate(&[reversed]).unwrap_err()),
            expected_code(RegistryError::InvalidDigestTimestamps)
        );

        let too_long_version = "v".repeat(MAX_DIGEST_VERSION_LEN + 1);
        assert_eq!(
            error_code(
                validate_quality_digest(&signals(1), &too_long_version, "v1").unwrap_err()
            ),
            expected_code(RegistryError::VersionStringTooLong)
        );
    }

    /// The 18 canonical metric names the vocabulary is growing to. Kept here
    /// only to size the worst realistic digest.
    const CANONICAL_SIGNALS: [&str; 18] = [
        "heart_rate",
        "sleep",
        "steps",
        "glucose",
        "calories_burned",
        "blood_pressure",
        "spo2",
        "ecg",
        "respiration_rate",
        "temperature",
        "hydration",
        "stress",
        "hrv",
        "vo2max",
        "body_fat",
        "weight_kg",
        "active_minutes",
        "floors_climbed",
    ];

    /// The digest rides in instruction data, so unlike `data_types` it is
    /// bounded by the 1232-byte transaction limit and NOT by account space.
    ///
    /// This pins down the real ceiling, which is tighter than the cap:
    /// `MAX_QUALITY_SIGNALS` is NOT reachable at `MAX_SIGNAL_NAME_LEN`. A full
    /// 18 signals only fits because the canonical names are short (~9 chars
    /// average). Anything that lengthens those names eats the headroom, so this
    /// test fails loudly rather than producing oversized transactions at
    /// runtime.
    #[test]
    fn serialized_digest_fits_in_a_transaction() {
        // Borsh: 4-byte Vec prefix + per signal (4-byte name prefix + name
        // bytes + four u32 + two i64 = name + 32).
        let digest_bytes = |names: &[&str]| -> usize {
            4 + names.iter().map(|n| 4 + n.len() + 32).sum::<usize>()
        };

        // Everything in the transaction that is not the digest: 1 signature
        // (65) + header (3) + 4 account keys (129) + blockhash (32) +
        // instruction framing (8) + discriminator (8) + meta_id (8) +
        // unit_index (4) + feat_cid (4 + 59) + two version strings (2 * 20) +
        // schema version (1).
        const NON_DIGEST_BYTES: usize = 65 + 3 + 129 + 32 + 8 + 8 + 8 + 4 + 63 + 40 + 1;
        const TX_LIMIT: usize = 1232;

        // The cap is NOT reachable at maximum name length.
        let max_name = "s".repeat(MAX_SIGNAL_NAME_LEN);
        let worst_names = vec![max_name.as_str(); MAX_QUALITY_SIGNALS];
        let worst = NON_DIGEST_BYTES + digest_bytes(&worst_names);
        assert!(
            worst > TX_LIMIT,
            "expected {MAX_QUALITY_SIGNALS} signals at {MAX_SIGNAL_NAME_LEN} bytes \
             ({worst}) to exceed the {TX_LIMIT}-byte transaction limit; if this now \
             fits, the warning on MAX_QUALITY_SIGNALS is stale"
        );

        // The actual vocabulary does fit, but only just.
        assert_eq!(CANONICAL_SIGNALS.len(), MAX_QUALITY_SIGNALS);
        let realistic = NON_DIGEST_BYTES + digest_bytes(&CANONICAL_SIGNALS);
        assert!(
            realistic <= TX_LIMIT,
            "the 18 canonical signals must fit in one transaction, got {realistic} bytes"
        );

        // Headroom is thin enough to be worth knowing about: if a future rename
        // pushes the digest over, this fires before anything hits devnet.
        let headroom = TX_LIMIT - realistic;
        assert!(
            headroom < 128,
            "headroom grew to {headroom} bytes - re-check whether the transaction-size \
             warning on MAX_QUALITY_SIGNALS still applies"
        );
    }
}
