use anchor_lang::prelude::*;

use crate::constants::{MAX_CHRONIC_CONDITIONS, MAX_DATA_TYPES};
use crate::contexts::{
    CloseDataEntryMeta, CloseUploadUnit, RegisterRawUpload, UpdateUploadUnit, UploadNewMeta,
};
use crate::errors::RegistryError;
use crate::events::{
    DataEntryDeleted, DataEntryVersionUpdated, DataStored, MetaAttributes, MetaDeviceInfo,
    MetaEntryCreated, UploadUnitClosed, UploadUnitCreated,
};
use crate::params::UploadNewMetaParams;

pub fn upload_new_meta(ctx: Context<UploadNewMeta>, params: UploadNewMetaParams) -> Result<()> {
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
    meta.unit_count = 1;
    meta.date_of_creation = clock.unix_timestamp;
    meta.date_of_modification = clock.unix_timestamp;
    meta.bump = ctx.bumps.data_entry_meta;

    let unit = &mut ctx.accounts.upload_unit;
    unit.meta_id = meta_id;
    unit.unit_index = 0;
    unit.raw_cid = params.raw_cid.clone();
    unit.day_start_timestamp = params.day_start_timestamp;
    unit.day_end_timestamp = params.day_end_timestamp;
    unit.feat_cid = String::new();
    unit.date_of_creation = clock.unix_timestamp;
    unit.bump = ctx.bumps.upload_unit;

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
    unit.meta_id = meta.meta_id;
    unit.unit_index = unit_index;
    unit.raw_cid = raw_cid.clone();
    unit.day_start_timestamp = day_start_timestamp;
    unit.day_end_timestamp = day_end_timestamp;
    unit.feat_cid = String::new();
    unit.date_of_creation = clock.unix_timestamp;
    unit.bump = ctx.bumps.upload_unit;

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

pub fn close_data_entry_meta(ctx: Context<CloseDataEntryMeta>, _meta_id: u64) -> Result<()> {
    require!(!ctx.accounts.registry_state.paused, RegistryError::Paused);

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
    emit!(UploadUnitClosed {
        meta_id: ctx.accounts.upload_unit.meta_id,
        unit_index: ctx.accounts.upload_unit.unit_index,
    });
    Ok(())
}
