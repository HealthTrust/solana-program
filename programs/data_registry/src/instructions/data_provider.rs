use anchor_lang::prelude::*;

use crate::constants::{
    MAX_CHRONIC_CONDITIONS, MAX_DATA_TYPE_LEN, MAX_DATA_TYPES, MAX_DIGEST_VERSION_LEN,
    MAX_QUALITY_SIGNALS, MAX_SIGNAL_NAME_LEN,
};
use crate::contexts::{
    CloseDataEntryMeta, CloseUploadUnit, RegisterRawUpload, UpdateUploadUnit, UploadNewMeta,
};
use crate::errors::RegistryError;
use crate::events::{
    DataEntryDeleted, DataEntryVersionUpdated, DataStored, MetaAttributes, MetaDeviceInfo,
    MetaEntryCreated, SignalQuality, UploadUnitClosed, UploadUnitCreated,
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
