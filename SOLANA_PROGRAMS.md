# HealthTrust Solana Programs — Reference Documentation

This document covers every instruction, every account, and every event for the two
active Anchor programs used in the HealthTrust MVP. It is written for agents and
developers building TypeScript/JavaScript dApps against these programs using
`@coral-xyz/anchor`.

> **MVP scope:** `pricing_config` exists in the repo but is **not used**. Prices are
> static and set off-chain. Do not reference or call `pricing_config` from any dApp.

---

## Table of Contents

1. [Overview](#overview)
2. [Universal Conventions](#universal-conventions)
3. [Program: data\_registry](#program-data_registry)
4. [Program: order\_handler](#program-order_handler)
5. [Calling from a Web App (TypeScript)](#calling-from-a-web-app-typescript)
6. [Event Indexing Reference](#event-indexing-reference)
7. [Enum / Code Reference](#enum--code-reference)
8. [Error Reference](#error-reference)

---

## Overview

| Program | Program ID | Responsibility |
|---|---|---|
| `data_registry` | `3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT` | Data provider uploads; metadata + upload unit lifecycle |
| `order_handler` | `GVUZtHZHr1tDxw3Pt142BxqgkS3dfDpPbqEznsFT9jV4` | Research job lifecycle; SOL escrow; provider payouts |

The two programs are **independent** — they do not call each other via CPI. Cross-program
state that must be coordinated (e.g., verifying a provider is registered before selecting
them for a job) is handled off-chain by the ROFL/TEE worker.

---

## Universal Conventions

### Data types

All data type identifiers are passed as plain **strings** (e.g. `"heart_rate"`, `"steps"`).
Each string is max **32 bytes** and at most **8** types may be supplied per entry.

Used in:
- `data_registry`: `DataEntryMeta.data_types` field
- `order_handler`: `Job.data_types` field

### Lamports

All monetary values are in **lamports** (1 SOL = 1,000,000,000 lamports).

### Timestamps

All timestamps are Unix seconds (`i64`).

### PDA derivation helper (TypeScript)

```typescript
import { PublicKey } from "@solana/web3.js";

const DATA_REGISTRY_ID  = new PublicKey("3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT");
const ORDER_HANDLER_ID  = new PublicKey("GVUZtHZHr1tDxw3Pt142BxqgkS3dfDpPbqEznsFT9jV4");

// u64 → 8-byte little-endian Buffer
function u64LE(n: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(n));
  return buf;
}
// u32 → 4-byte little-endian Buffer
function u32LE(n: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(n);
  return buf;
}

// data_registry PDAs
function registryStatePDA() {
  return PublicKey.findProgramAddressSync([Buffer.from("registry_state")], DATA_REGISTRY_ID);
}
function dataEntryMetaPDA(metaId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("meta"), u64LE(metaId)], DATA_REGISTRY_ID);
}
function uploadUnitPDA(metaId: bigint, unitIndex: number) {
  return PublicKey.findProgramAddressSync([Buffer.from("unit"), u64LE(metaId), u32LE(unitIndex)], DATA_REGISTRY_ID);
}

// order_handler PDAs
function orderConfigPDA() {
  return PublicKey.findProgramAddressSync([Buffer.from("order_config")], ORDER_HANDLER_ID);
}
function jobPDA(jobId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("job"), u64LE(jobId)], ORDER_HANDLER_ID);
}
function escrowVaultPDA(jobId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("escrow"), u64LE(jobId)], ORDER_HANDLER_ID);
}
```

---

## Program: data\_registry

Manages the data provider lifecycle: creating metadata entries, appending raw upload
units, and the TEE processing them into feature CIDs.

### Accounts

#### `RegistryState`
**PDA seeds:** `[b"registry_state"]`  
**Singleton.**

| Field | Type | Description |
|---|---|---|
| `owner` | `Pubkey` | Admin wallet |
| `pricing_program` | `Pubkey` | Unused in MVP; can be zero |
| `tee_authority` | `Pubkey` | The only wallet allowed to call `update_upload_unit` |
| `next_meta_id` | `u64` | Counter for the next `DataEntryMeta` ID (starts at 1) |
| `paused` | `bool` | If `true`, all data-provider instructions revert |
| `bump` | `u8` | PDA canonical bump |

#### `DataEntryMeta`
**PDA seeds:** `[b"meta", meta_id.to_le_bytes()]`  
**One per data provider dataset grouping.**

| Field | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Unique identifier (from registry counter) |
| `owner` | `Pubkey` | Data provider wallet |
| `device_type` | `String` | Device category, e.g. `"smartwatch"` (max 32 bytes) |
| `device_model` | `String` | Model name, e.g. `"Apple Watch Series 7"` (max 48 bytes) |
| `service_provider` | `String` | Health platform, e.g. `"Apple Health"` (max 48 bytes) |
| `age` | `u8` | Age bracket code |
| `gender` | `u8` | Gender code |
| `height` | `u8` | Height code |
| `weight` | `u8` | Weight code |
| `region` | `u8` | Geographic region code |
| `physical_activity_level` | `u8` | Activity level code |
| `smoker` | `u8` | `0` = non-smoker, `1` = smoker |
| `diet` | `u8` | Diet code |
| `chronic_conditions` | `Vec<u8>` | Condition codes (max 16) |
| `data_types` | `Vec<String>` | Data type strings in this dataset (max 8, each max 32 bytes) |
| `total_duration` | `u64` | Cumulative seconds of data across all upload units |
| `unit_count` | `u32` | Number of `UploadUnit` PDAs under this meta |
| `date_of_creation` | `i64` | Unix timestamp |
| `date_of_modification` | `i64` | Unix timestamp of last unit append |
| `bump` | `u8` | PDA canonical bump |

#### `UploadUnit`
**PDA seeds:** `[b"unit", meta_id.to_le_bytes(), unit_index.to_le_bytes()]`  
**One per upload. `unit_index` is zero-based and scoped per meta.**

| Field | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Parent meta ID |
| `unit_index` | `u32` | Index within this meta (0, 1, 2, …) |
| `raw_cid` | `String` | IPFS CID of raw data blob (max 64 bytes) |
| `day_start_timestamp` | `i64` | Unix start of the data collection window |
| `day_end_timestamp` | `i64` | Unix end of the data collection window |
| `feat_cid` | `String` | IPFS CID of TEE-processed feature pack (empty until set by TEE, max 64 bytes) |
| `date_of_creation` | `i64` | Unix timestamp |
| `bump` | `u8` | PDA canonical bump |

To enumerate all units for a given meta: iterate `unit_index` from `0` to
`data_entry_meta.unit_count - 1` and derive each PDA.

---

### Instructions

#### `initialize_registry`
**Who calls it:** Protocol deployer (one time)  
**Signer:** `owner`

Creates the `RegistryState` singleton. Fails if already initialized.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Singleton PDA being created |
| `owner` | ✓ | ✓ | Payer and admin |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `tee_authority` | `Pubkey` | Wallet of the TEE/ROFL worker allowed to set `feat_cid`s |

---

#### `set_pricing_program`
**Who calls it:** Admin  
**Signer:** `owner`

Stores a program ID in `RegistryState.pricing_program`. Unused in MVP.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Singleton |
| `owner` | — | ✓ | Must equal `registry_state.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `pricing_program` | `Pubkey` | Program ID to store (can be any value) |

---

#### `set_tee_authority`
**Who calls it:** Admin  
**Signer:** `owner`

Updates the address allowed to call `update_upload_unit`.

**Accounts required:** Same as `set_pricing_program`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `tee_authority` | `Pubkey` | New TEE/ROFL authority wallet (cannot be zero) |

---

#### `set_paused`
**Who calls it:** Admin  
**Signer:** `owner`

Pauses or unpauses the registry. When paused, all data-provider instructions fail.

**Accounts required:** Same as `set_pricing_program`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `paused` | `bool` | `true` to pause, `false` to unpause |

**Emits:** `PausedStateChanged`

---

#### `transfer_registry_ownership`
**Who calls it:** Current admin  
**Signer:** `owner`

**Accounts required:** Same as `set_pricing_program`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `new_owner` | `Pubkey` | New admin wallet (cannot be zero) |

**Emits:** `OwnershipTransferred`

---

#### `emergency_withdraw_sol`
**Who calls it:** Admin only  
**Signer:** `owner`

Withdraws accidentally-sent SOL from the `RegistryState` PDA.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Source of lamports |
| `recipient` | ✓ | — | Destination (unchecked — admin's responsibility) |
| `owner` | — | ✓ | Must equal `registry_state.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `amount` | `u64` | Lamports to withdraw |

**Emits:** `EmergencyWithdrawal`

---

#### `upload_new_meta`
**Who calls it:** Data provider  
**Signer:** `provider`

Creates a new `DataEntryMeta` AND its first `UploadUnit` (index 0) in one instruction.
The `meta_id` is assigned by the program from `registry_state.next_meta_id`.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Counter incremented after creation |
| `data_entry_meta` | ✓ | — | New PDA: seeds `["meta", next_meta_id_le8]` |
| `upload_unit` | ✓ | — | New PDA: seeds `["unit", next_meta_id_le8, 0_u32_le4]` |
| `provider` | ✓ | ✓ | Payer for both accounts' rent |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments (`UploadNewMetaParams` struct):**

| Field | Type | Max | Description |
|---|---|---|---|
| `raw_cid` | `String` | 64 bytes | IPFS CID of the raw data blob |
| `data_types` | `Vec<String>` | 8 entries, each max 32 bytes | Data type strings (at least 1, e.g. `"heart_rate"`) |
| `device_type` | `String` | 32 bytes | Device category |
| `device_model` | `String` | 48 bytes | Device model name |
| `service_provider` | `String` | 48 bytes | Health platform name |
| `day_start_timestamp` | `i64` | — | Start of data window (Unix seconds) |
| `day_end_timestamp` | `i64` | — | End of data window (Unix seconds, must be > start) |
| `age` | `u8` | — | Age bracket code |
| `gender` | `u8` | — | Gender code |
| `height` | `u8` | — | Height code |
| `weight` | `u8` | — | Weight code |
| `region` | `u8` | — | Region code |
| `physical_activity_level` | `u8` | — | Activity level code |
| `smoker` | `u8` | — | `0` = non-smoker, `1` = smoker |
| `diet` | `u8` | — | Diet code |
| `chronic_conditions` | `Vec<u8>` | 16 entries | Chronic condition codes |

**Emits (in order):** `MetaEntryCreated`, `MetaAttributes`, `MetaDeviceInfo`, `UploadUnitCreated`

---

#### `register_raw_upload`
**Who calls it:** Data provider  
**Signer:** `provider`

Appends a new `UploadUnit` to an existing `DataEntryMeta`. The provider must own the meta.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | — | — | Read to check paused flag |
| `data_entry_meta` | ✓ | — | PDA: seeds `["meta", meta_id_le8]` |
| `upload_unit` | ✓ | — | New PDA: seeds `["unit", meta_id_le8, unit_count_le4]` |
| `provider` | ✓ | ✓ | Must equal `data_entry_meta.owner`; pays rent |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Note on PDA seed for `upload_unit`:** use `data_entry_meta.unit_count` (before this call)
as the `unit_index` seed — that is the index the new unit will receive.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `meta_id` | `u64` | ID of the existing `DataEntryMeta` (used in PDA seeds) |
| `raw_cid` | `String` | IPFS CID of the new raw data blob (max 64 bytes) |
| `day_start_timestamp` | `i64` | Start of new data window |
| `day_end_timestamp` | `i64` | End of new data window (must be > start) |

**Emits:** `UploadUnitCreated`, `DataStored`

---

#### `update_upload_unit`
**Who calls it:** TEE/ROFL worker only (must match `registry_state.tee_authority`)  
**Signer:** `tee_authority`

Sets the `feat_cid` on an existing `UploadUnit` after the TEE has processed the raw data.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | — | — | Validates `tee_authority` |
| `upload_unit` | ✓ | — | PDA: seeds `["unit", meta_id_le8, unit_index_le4]` |
| `tee_authority` | — | ✓ | Must equal `registry_state.tee_authority` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Parent meta ID (part of PDA seed) |
| `unit_index` | `u32` | Unit index within that meta (part of PDA seed) |
| `feat_cid` | `String` | IPFS CID of the processed feature pack (max 64 bytes, must not be empty) |

**Emits:** `DataEntryVersionUpdated`

---

#### `close_data_entry_meta`
**Who calls it:** Data provider (meta owner)  
**Signer:** `provider`

Closes (deletes) a `DataEntryMeta` PDA and returns rent to the provider. Does **not**
close child `UploadUnit` accounts — call `close_upload_unit` separately for each.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | — | — | Paused check |
| `data_entry_meta` | ✓ | — | PDA being closed; rent → provider |
| `provider` | ✓ | ✓ | Must equal `data_entry_meta.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `meta_id` | `u64` | ID of the meta to close (used in PDA seed) |

**Emits:** `DataEntryDeleted`

---

#### `close_upload_unit`
**Who calls it:** Data provider (meta owner)  
**Signer:** `provider`

Closes a single `UploadUnit` PDA and returns rent. Can be called even when registry is paused.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `data_entry_meta` | — | — | Read to verify provider ownership |
| `upload_unit` | ✓ | — | PDA being closed; rent → provider |
| `provider` | ✓ | ✓ | Must equal `data_entry_meta.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Parent meta ID (PDA seed) |
| `unit_index` | `u32` | Unit index (PDA seed) |

**Emits:** `UploadUnitClosed`

---

### Events — data\_registry

#### `MetaEntryCreated`
Fired when a new `DataEntryMeta` is created. Index this to build a dataset catalog.
```
meta_id:            u64
owner:              Pubkey   — data provider wallet
data_types:         String[] — list of data type strings
total_duration:     u64      — seconds (day_end - day_start of first unit)
date_of_creation:   i64      — Unix timestamp
```

#### `MetaAttributes`
Fired alongside `MetaEntryCreated`. Contains demographic/health attributes.
```
meta_id:                    u64
age:                        u8
gender:                     u8
height:                     u8
weight:                     u8
region:                     u8
physical_activity_level:    u8
smoker:                     u8
diet:                       u8
chronic_conditions:         u8[]  — array of condition codes
```

#### `MetaDeviceInfo`
Fired alongside `MetaEntryCreated`. Contains device details.
```
meta_id:          u64
device_type:      String
device_model:     String
service_provider: String
```

#### `UploadUnitCreated`
Fired when any `UploadUnit` is created (on both `upload_new_meta` and `register_raw_upload`).
```
meta_id:               u64
unit_index:            u32
raw_cid:               String  — IPFS CID of raw data
day_start_timestamp:   i64
day_end_timestamp:     i64
date_of_creation:      i64
```

#### `DataStored`
Fired only on `register_raw_upload` (not on first upload). Contains aggregated meta stats.
```
meta_id:               u64
unit_index:            u32
owner:                 Pubkey
raw_cid:               String
day_start_timestamp:   i64
day_end_timestamp:     i64
total_duration:        u64   — meta's cumulative total after this upload
added_duration:        u64   — seconds added by this upload
```

#### `DataEntryVersionUpdated`
Fired when TEE sets `feat_cid`. Key event: indicates data is ready for researcher access.
```
meta_id:      u64
unit_index:   u32
feat_cid:     String  — IPFS CID of processed feature pack
updater:      Pubkey  — tee_authority wallet
timestamp:    i64
```

#### `DataEntryDeleted`
```
meta_id:    u64
owner:      Pubkey
timestamp:  i64
```

#### `UploadUnitClosed`
```
meta_id:     u64
unit_index:  u32
```

#### `PausedStateChanged`
```
paused:  bool
```

#### `OwnershipTransferred` (data\_registry)
```
previous:  Pubkey
new:       Pubkey
```

#### `EmergencyWithdrawal`
```
recipient:  Pubkey
amount:     u64   — lamports
```

---

## Program: order\_handler

Manages the full research job lifecycle including SOL escrow and provider payouts.

### Job State Machine

```
request_job
    │
    ▼
PENDING_PREFLIGHT  ──── cancel_job ──► CANCELLED
    │
    │  submit_preflight_result  (ROFL only)
    ▼
AWAITING_CONFIRMATION  ──── cancel_job ──► CANCELLED
    │
    │  confirm_job_and_pay  (researcher)
    ▼
CONFIRMED
    │
    │  submit_result  (ROFL only)
    ▼
EXECUTED
    │
    │  finalize_job  (ROFL only)
    ▼
COMPLETED
    │
    ├──► claim_payout  (each selected data provider)
    └──► sweep_vault_dust  (researcher or after all providers claimed)
```

### Accounts

#### `OrderConfig`
**PDA seeds:** `[b"order_config"]`  
**Singleton.**

| Field | Type | Description |
|---|---|---|
| `owner` | `Pubkey` | Admin wallet |
| `rofl_authority` | `Pubkey` | The ROFL/TEE worker address; only it can submit preflight/result/finalize |
| `next_job_id` | `u64` | Monotonically increasing job counter (starts at 1) |
| `bump` | `u8` | PDA canonical bump |

#### `Job`
**PDA seeds:** `[b"job", job_id.to_le_bytes()]`

| Field | Type | Description |
|---|---|---|
| `job_id` | `u64` | Unique job identifier |
| `researcher` | `Pubkey` | Wallet that created the job |
| `status` | `JobStatus` | Current lifecycle state (see enum below) |
| `template_id` | `u32` | Job template ID (> 0) |
| `num_days` | `u32` | Duration of the data window requested |
| `data_types` | `Vec<String>` | Data type strings requested (max 8, each max 32 bytes) |
| `max_participants` | `u32` | Maximum number of data providers to include |
| `start_day_utc` | `i64` | Fixed window start (Unix seconds) |
| `filter_query` | `String` | Optional cohort filter string (max 128 bytes) |
| `escrowed` | `u64` | Lamports deposited in vault |
| `effective_participants_scaled` | `u64` | Σq_i × 1e18, from ROFL preflight |
| `quality_tier` | `u8` | Overall quality tier from preflight |
| `final_total` | `u64` | Required payment in lamports (set by ROFL in preflight) |
| `preflight_timestamp` | `i64` | When preflight was submitted |
| `cohort_hash` | `[u8; 32]` | Hash of selected unit IDs / feat CIDs (audit trail) |
| `selected_participants` | `Vec<Pubkey>` | Wallets of selected data providers (max 50) |
| `result_cid` | `String` | IPFS CID of job result (set by ROFL, max 64 bytes) |
| `execution_timestamp` | `i64` | When result was submitted |
| `output_hash` | `[u8; 32]` | sha256 of attestation / plaintext output |
| `amount_per_provider` | `u64` | Equal payout per provider (computed at finalize) |
| `claimed_bitmap` | `u64` | Bit i = 1 means `selected_participants[i]` has claimed (supports up to 64 providers) |
| `created_at` | `i64` | Unix timestamp |
| `updated_at` | `i64` | Unix timestamp of last state change |
| `bump` | `u8` | PDA canonical bump |

#### `EscrowVault`
**PDA seeds:** `[b"escrow", job_id.to_le_bytes()]`  
**Holds SOL lamports for paying providers.**

| Field | Type | Description |
|---|---|---|
| `job_id` | `u64` | Parent job |
| `bump` | `u8` | PDA canonical bump |

To read the current escrow balance: `connection.getBalance(escrowVaultPubkey)`.

---

### Instructions

#### `initialize_order_config`
**Who calls it:** Protocol deployer (one time)  
**Signer:** `owner`

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | ✓ | — | Singleton PDA being created |
| `owner` | ✓ | ✓ | Payer and future admin |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `rofl_authority` | `Pubkey` | ROFL/TEE worker wallet (cannot be zero) |

---

#### `set_rofl_authority`
**Who calls it:** Admin  
**Signer:** `owner`

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | ✓ | — | Singleton |
| `owner` | — | ✓ | Must equal `order_config.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `rofl_authority` | `Pubkey` | New ROFL worker wallet (cannot be zero) |

**Emits:** `RoflAuthorityUpdated`

---

#### `transfer_order_ownership`
**Who calls it:** Current admin  
**Signer:** `owner`

**Accounts required:** Same as `set_rofl_authority`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `new_owner` | `Pubkey` | New admin wallet (cannot be zero) |

**Emits:** `OwnershipTransferred`

---

#### `request_job`
**Who calls it:** Researcher  
**Signer:** `researcher`

Phase 1 of the job lifecycle. Allocates the `Job` PDA (researcher pays rent ~0.016 SOL).
No escrow payment at this stage.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | ✓ | — | Counter incremented; seeds new Job PDA |
| `job` | ✓ | — | New PDA: seeds `["job", next_job_id_le8]` |
| `researcher` | ✓ | ✓ | Payer for Job account rent |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments (`JobParams` struct):**

| Field | Type | Constraints | Description |
|---|---|---|---|
| `template_id` | `u32` | > 0 | Job template identifier |
| `num_days` | `u32` | > 0 | Number of days of data requested |
| `data_types` | `Vec<String>` | 1–8 entries, each max 32 bytes | Data type strings, e.g. `"heart_rate"` |
| `max_participants` | `u32` | > 0 | Maximum number of data providers |
| `start_day_utc` | `i64` | — | Fixed window start time |
| `filter_query` | `String` | max 128 bytes | Optional cohort filter expression |

**Returns** (read from event): `job_id` — the assigned job ID.

**Emits:** `JobRequested`

---

#### `submit_preflight_result`
**Who calls it:** ROFL/TEE worker only  
**Signer:** `rofl_authority`

Phase 2. ROFL submits the preflight analysis result and the list of selected participants.
Transitions: `PENDING_PREFLIGHT` → `AWAITING_CONFIRMATION`.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | — | — | Validates `rofl_authority` |
| `job` | ✓ | — | PDA: seeds `["job", job_id_le8]` |
| `rofl_authority` | — | ✓ | Must equal `order_config.rofl_authority` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to update |
| `preflight.effective_participants_scaled` | `u64` | Σq_i × 1e18 |
| `preflight.quality_tier` | `u8` | Aggregate quality tier |
| `preflight.final_total` | `u64` | Required payment in lamports |
| `preflight.cohort_hash` | `[u8; 32]` | Hash of selected unit IDs / feat CIDs |
| `preflight.selected_participants` | `Vec<Pubkey>` | Wallets of selected providers (max 50) |

**Emits:** `PreflightSubmitted`

---

#### `confirm_job_and_pay`
**Who calls it:** Researcher  
**Signer:** `researcher`

Phase 3. Researcher reviews the preflight result and sends payment. Creates `EscrowVault`.
Transitions: `AWAITING_CONFIRMATION` → `CONFIRMED`.

Researcher pays:
- `EscrowVault` rent (~0.001 SOL, one time)
- `payment_amount` lamports (must be ≥ `job.final_total`)

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `job` | ✓ | — | PDA: seeds `["job", job_id_le8]` |
| `escrow_vault` | ✓ | — | New PDA: seeds `["escrow", job_id_le8]` |
| `researcher` | ✓ | ✓ | Must equal `job.researcher`; pays vault rent + payment |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to confirm (PDA seed) |
| `payment_amount` | `u64` | Lamports to deposit (must be ≥ `job.final_total`) |

**Emits:** `JobConfirmed`

---

#### `cancel_job`
**Who calls it:** Researcher  
**Signer:** `researcher`

Cancels a job. Only allowed in `PENDING_PREFLIGHT` or `AWAITING_CONFIRMATION` (before
payment is made). Closes the `Job` PDA and returns rent to the researcher. No escrow
refund is needed because escrow has not been created yet.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `job` | ✓ | — | PDA being closed; rent → researcher |
| `researcher` | ✓ | ✓ | Must equal `job.researcher` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to cancel |

**Emits:** `JobCancelled`

---

#### `submit_result`
**Who calls it:** ROFL/TEE worker only  
**Signer:** `rofl_authority`

Phase 4. ROFL submits the encrypted result CID and output hash.
Transitions: `CONFIRMED` → `EXECUTED`.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | — | — | Validates `rofl_authority` |
| `job` | ✓ | — | PDA: seeds `["job", job_id_le8]` |
| `rofl_authority` | — | ✓ | Must equal `order_config.rofl_authority` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to update |
| `result_cid` | `String` | IPFS CID of the result (max 64 bytes, must not be empty) |
| `output_hash` | `[u8; 32]` | sha256 of plaintext output or attestation |

**Emits:** `JobExecuted`

---

#### `finalize_job`
**Who calls it:** ROFL/TEE worker only  
**Signer:** `rofl_authority`

Phase 5. Computes `amount_per_provider = (vault_balance - vault_rent) / num_providers`
and sets status to `COMPLETED`. Does **not** push payments — providers must pull via
`claim_payout` individually.  
Transitions: `EXECUTED` → `COMPLETED`.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | — | — | Validates `rofl_authority` |
| `job` | ✓ | — | PDA: seeds `["job", job_id_le8]` |
| `escrow_vault` | — | — | Read to compute distributable lamports |
| `rofl_authority` | — | ✓ | Must equal `order_config.rofl_authority` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to finalize |

**Emits:** `JobCompleted`

---

#### `claim_payout`
**Who calls it:** Any selected data provider  
**Signer:** `provider`

Provider pulls their share of the escrow. Uses a bitmap to prevent double-claiming.
Callable any time after job is `COMPLETED`.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `job` | ✓ | — | PDA: seeds `["job", job_id_le8]` — bitmap updated |
| `escrow_vault` | ✓ | — | PDA: seeds `["escrow", job_id_le8]` — lamports deducted |
| `provider` | ✓ | ✓ | Must appear in `job.selected_participants`; receives lamports |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job to claim from |

**Emits:** `DataProviderPaid`

---

#### `sweep_vault_dust`
**Who calls it:** Researcher (or anyone after all providers claimed)  
**Signer:** not required — validated by on-chain logic

Sweeps leftover lamports (overpayment + integer division rounding) from the vault.
Allowed if: **all** providers have claimed (all bits set in `claimed_bitmap`), OR the
`recipient` account is the researcher.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `job` | — | — | Read to verify status and all-claimed condition |
| `escrow_vault` | ✓ | — | PDA: lamports swept from here |
| `recipient` | ✓ | — | Unchecked destination (must pass logic check) |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `job_id` | `u64` | Job whose vault to sweep |

**Emits:** `VaultDustSwept`

---

### Events — order\_handler

#### `JobRequested`
Fired on every new job. Index this to build a jobs table.
```
job_id:           u64
researcher:       Pubkey
template_id:      u32
num_days:         u32
data_types:       String[]
max_participants: u32
```

#### `PreflightSubmitted`
Fired when ROFL submits preflight results. Shows required payment.
```
job_id:                             u64
effective_participants_scaled:      u64
quality_tier:                       u8
final_total:                        u64   — lamports required from researcher
cohort_hash:                        [u8;32]
```

#### `JobConfirmed`
Fired when researcher pays.
```
job_id:   u64
amount:   u64   — lamports deposited
```

#### `JobCancelled`
```
job_id:          u64
refund_amount:   u64   — always 0 (no payment at cancel stages)
```

#### `JobExecuted`
Fired when ROFL submits the result. Researcher can now access the result CID.
```
job_id:      u64
result_cid:  String
```

#### `JobCompleted`
Fired when job is finalized. Data providers can now claim.
```
job_id:               u64
amount_per_provider:  u64   — lamports each provider will receive
num_providers:        u64   — number of selected participants
```

#### `DataProviderPaid`
Fired on each individual claim.
```
job_id:     u64
provider:   Pubkey
amount:     u64   — lamports paid
```

#### `VaultDustSwept`
```
job_id:      u64
recipient:   Pubkey
amount:      u64   — lamports swept
```

#### `RoflAuthorityUpdated`
```
rofl_authority:  Pubkey
```

#### `OwnershipTransferred` (order\_handler)
```
previous:  Pubkey
new:       Pubkey
```

---

## Calling from a Web App (TypeScript)

### Setup

```typescript
import { AnchorProvider, BN, Program, setProvider, web3 } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram } from "@solana/web3.js";

const DATA_REGISTRY_ID  = new PublicKey("3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT");
const ORDER_HANDLER_ID  = new PublicKey("GVUZtHZHr1tDxw3Pt142BxqgkS3dfDpPbqEznsFT9jV4");

const connection = new Connection("https://api.devnet.solana.com");
const provider   = AnchorProvider.env();
setProvider(provider);

// Load IDLs (generated by `anchor build`, found in target/idl/)
import registryIdl  from "./target/idl/data_registry.json";
import orderIdl     from "./target/idl/order_handler.json";

const registryProgram = new Program(registryIdl as any, provider);
const orderProgram    = new Program(orderIdl    as any, provider);

// ─── Helpers ─────────────────────────────────────────────────────────────────

function u64LE(n: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(n));
  return buf;
}
function u32LE(n: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(n);
  return buf;
}

// PDA derivations
function registryStatePDA() {
  return PublicKey.findProgramAddressSync([Buffer.from("registry_state")], DATA_REGISTRY_ID);
}
function dataEntryMetaPDA(metaId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("meta"), u64LE(metaId)], DATA_REGISTRY_ID);
}
function uploadUnitPDA(metaId: bigint, unitIndex: number) {
  return PublicKey.findProgramAddressSync([Buffer.from("unit"), u64LE(metaId), u32LE(unitIndex)], DATA_REGISTRY_ID);
}
function orderConfigPDA() {
  return PublicKey.findProgramAddressSync([Buffer.from("order_config")], ORDER_HANDLER_ID);
}
function jobPDA(jobId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("job"), u64LE(jobId)], ORDER_HANDLER_ID);
}
function escrowVaultPDA(jobId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("escrow"), u64LE(jobId)], ORDER_HANDLER_ID);
}
```

---

### Example: Upload new meta (first dataset)

```typescript
async function uploadNewMeta(params: {
  rawCid: string;
  dataTypes: string[];       // e.g. ["heart_rate", "steps"] — plain strings, max 32 chars each
  deviceType: string;
  deviceModel: string;
  serviceProvider: string;
  dayStart: number;          // Unix seconds
  dayEnd: number;
  attributes: {
    age: number; gender: number; height: number; weight: number;
    region: number; physicalActivityLevel: number; smoker: number;
    diet: number; chronicConditions: number[];
  };
}) {
  const [registryState] = registryStatePDA();

  const state = await registryProgram.account.registryState.fetch(registryState);
  const metaId = state.nextMetaId as bigint;
  const [metaPDA]  = dataEntryMetaPDA(metaId);
  const [unitPDA]  = uploadUnitPDA(metaId, 0);

  const sig = await registryProgram.methods
    .uploadNewMeta({
      rawCid: params.rawCid,
      dataTypes: params.dataTypes,
      deviceType: params.deviceType,
      deviceModel: params.deviceModel,
      serviceProvider: params.serviceProvider,
      dayStartTimestamp: new BN(params.dayStart),
      dayEndTimestamp: new BN(params.dayEnd),
      age: params.attributes.age,
      gender: params.attributes.gender,
      height: params.attributes.height,
      weight: params.attributes.weight,
      region: params.attributes.region,
      physicalActivityLevel: params.attributes.physicalActivityLevel,
      smoker: params.attributes.smoker,
      diet: params.attributes.diet,
      chronicConditions: params.attributes.chronicConditions,
    })
    .accounts({
      registryState,
      dataEntryMeta: metaPDA,
      uploadUnit: unitPDA,
      provider: provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return { sig, metaId };
}
```

---

### Example: Append a new upload unit

```typescript
async function registerRawUpload(metaId: bigint, rawCid: string, dayStart: number, dayEnd: number) {
  const [registryState] = registryStatePDA();
  const [metaPDA]       = dataEntryMetaPDA(metaId);

  const meta = await registryProgram.account.dataEntryMeta.fetch(metaPDA);
  const nextUnitIndex = meta.unitCount as number;
  const [unitPDA]     = uploadUnitPDA(metaId, nextUnitIndex);

  const sig = await registryProgram.methods
    .registerRawUpload(new BN(metaId.toString()), rawCid, new BN(dayStart), new BN(dayEnd))
    .accounts({
      registryState,
      dataEntryMeta: metaPDA,
      uploadUnit: unitPDA,
      provider: provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return { sig, unitIndex: nextUnitIndex };
}
```

---

### Example: Researcher requests a job

```typescript
async function requestJob(params: {
  templateId: number;
  numDays: number;
  dataTypes: string[];       // plain strings, e.g. ["heart_rate", "steps"]
  maxParticipants: number;
  startDayUtc: number;
  filterQuery: string;
}) {
  const [orderConfig] = orderConfigPDA();
  const cfg    = await orderProgram.account.orderConfig.fetch(orderConfig);
  const jobId  = cfg.nextJobId as bigint;
  const [job]  = jobPDA(jobId);

  const sig = await orderProgram.methods
    .requestJob({
      templateId: params.templateId,
      numDays: params.numDays,
      dataTypes: params.dataTypes,
      maxParticipants: params.maxParticipants,
      startDayUtc: new BN(params.startDayUtc),
      filterQuery: params.filterQuery,
    })
    .accounts({
      orderConfig,
      job,
      researcher: provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return { sig, jobId };
}
```

---

### Example: Researcher confirms and pays

```typescript
async function confirmAndPay(jobId: bigint, paymentLamports: bigint) {
  const [job]   = jobPDA(jobId);
  const [vault] = escrowVaultPDA(jobId);

  return orderProgram.methods
    .confirmJobAndPay(new BN(jobId.toString()), new BN(paymentLamports.toString()))
    .accounts({
      job,
      escrowVault:  vault,
      researcher:   provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
}
```

---

### Example: Data provider claims payout

```typescript
async function claimPayout(jobId: bigint) {
  const [job]   = jobPDA(jobId);
  const [vault] = escrowVaultPDA(jobId);

  return orderProgram.methods
    .claimPayout(new BN(jobId.toString()))
    .accounts({
      job,
      escrowVault: vault,
      provider:    provider.wallet.publicKey,
    })
    .rpc();
}
```

---

### Example: Fetch account data

```typescript
// Read RegistryState
const [rs] = registryStatePDA();
const registryState = await registryProgram.account.registryState.fetch(rs);
console.log("next_meta_id:", registryState.nextMetaId.toString());
console.log("paused:", registryState.paused);

// Read a DataEntryMeta
const [meta] = dataEntryMetaPDA(1n);
const metaData = await registryProgram.account.dataEntryMeta.fetch(meta);
console.log("owner:", metaData.owner.toBase58());
console.log("unit_count:", metaData.unitCount);

// Read an UploadUnit
const [unit] = uploadUnitPDA(1n, 0);
const unitData = await registryProgram.account.uploadUnit.fetch(unit);
console.log("raw_cid:", unitData.rawCid);
console.log("feat_cid:", unitData.featCid);  // empty until TEE processes it

// Read OrderConfig
const [oc] = orderConfigPDA();
const orderConfig = await orderProgram.account.orderConfig.fetch(oc);
console.log("next_job_id:", orderConfig.nextJobId.toString());

// Read a Job
const [j] = jobPDA(1n);
const job = await orderProgram.account.job.fetch(j);
console.log("status:", job.status);
console.log("final_total:", job.finalTotal.toString(), "lamports");
console.log("selected participants:", job.selectedParticipants.map((p: PublicKey) => p.toBase58()));
console.log("amount_per_provider:", job.amountPerProvider.toString());
```

---

## Event Indexing Reference

Anchor emits events as log messages in transaction logs. To subscribe and index:

```typescript
// Subscribe to a program's events (Anchor 0.30+)
registryProgram.addEventListener("MetaEntryCreated", (event, slot, sig) => {
  console.log("New meta:", event.metaId.toString());
});

orderProgram.addEventListener("JobCompleted", (event, slot, sig) => {
  console.log("Job completed:", event.jobId.toString());
  console.log("Amount per provider:", event.amountPerProvider.toString(), "lamports");
});

// Parse historical events from transaction logs
async function parseHistoricalEvents(programId: PublicKey, idl: any) {
  const sigs = await connection.getSignaturesForAddress(programId, { limit: 100 });
  const eventCoder = new anchor.BorshEventCoder(idl);
  for (const { signature } of sigs) {
    const tx = await connection.getParsedTransaction(signature, { commitment: "confirmed" });
    for (const log of tx?.meta?.logMessages ?? []) {
      if (log.startsWith("Program data:")) {
        try {
          const event = eventCoder.decode(log.slice("Program data: ".length));
          if (event) console.log(event.name, event.data);
        } catch (_) {}
      }
    }
  }
}
```

### Recommended indexing table → event mapping

| Table | Primary events | Secondary events |
|---|---|---|
| `datasets` | `MetaEntryCreated` | `DataEntryDeleted` |
| `dataset_attributes` | `MetaAttributes` | — |
| `dataset_devices` | `MetaDeviceInfo` | — |
| `upload_units` | `UploadUnitCreated` | `DataEntryVersionUpdated`, `UploadUnitClosed` |
| `data_activity` | `DataStored` | — |
| `jobs` | `JobRequested` | `JobConfirmed`, `JobCancelled`, `JobExecuted`, `JobCompleted` |
| `job_payouts` | `DataProviderPaid` | `VaultDustSwept` |
| `admin_actions` | `OwnershipTransferred`, `PausedStateChanged`, `RoflAuthorityUpdated`, `EmergencyWithdrawal` | — |

---

## Enum / Code Reference

### `JobStatus` (order\_handler)

| Value | Name | Description |
|---|---|---|
| 0 | `None` | Uninitialized (should never appear in a live account) |
| 1 | `PendingPreflight` | Job created, waiting for ROFL preflight |
| 2 | `AwaitingConfirmation` | Preflight done, waiting for researcher payment |
| 3 | `Confirmed` | Paid and locked, ROFL running computation |
| 4 | `Executed` | Computation done, result CID submitted |
| 5 | `Completed` | Finalized, providers may claim payout |
| 6 | `Cancelled` | Cancelled by researcher (no payment taken) |

### Attribute codes (data\_registry)

**age**
| Code | Range |
|---|---|
| 1 | 18–25 |
| 2 | 26–35 |
| 3 | 36–45 |
| 4 | 46–55 |
| 5 | 56+ |

**gender**
| Code | Value |
|---|---|
| 1 | Male |
| 2 | Female |
| 3 | Other / prefer not to say |

**physical\_activity\_level**
| Code | Value |
|---|---|
| 1 | Low |
| 2 | Moderate |
| 3 | High |

**smoker**
| Code | Value |
|---|---|
| 0 | Non-smoker |
| 1 | Smoker |

**diet**
| Code | Value |
|---|---|
| 1 | Omnivore |
| 2 | Vegetarian |
| 3 | Vegan |

---

## Error Reference

### data\_registry errors

| Code | Name | Message |
|---|---|---|
| 6000 | `Paused` | Registry is paused |
| 6001 | `Unauthorized` | Caller is not the registry owner |
| 6002 | `NotOwner` | Caller is not the meta entry owner |
| 6003 | `NotTeeAuthority` | Caller is not the TEE authority |
| 6004 | `EmptyDataTypes` | data_types must not be empty |
| 6005 | `TooManyDataTypes` | Too many data types (max 8) |
| 6006 | `TooManyConditions` | Too many chronic conditions (max 16) |
| 6007 | `InvalidTimestampRange` | day_end_timestamp must be greater than day_start_timestamp |
| 6008 | `EmptyFeatCid` | feat_cid must not be empty |
| 6009 | `Overflow` | Arithmetic overflow |
| 6010 | `InvalidOwner` | New owner cannot be the zero address |
| 6011 | `InvalidAuthority` | TEE authority cannot be the zero address |

### order\_handler errors

| Code | Name | Message |
|---|---|---|
| 6000 | `Unauthorized` | Caller is not the order handler owner |
| 6001 | `NotRoflAuthority` | Caller is not the ROFL authority |
| 6002 | `InvalidStatus` | Job status does not allow this operation |
| 6003 | `JobIdMismatch` | Job ID in instruction does not match account |
| 6004 | `InvalidNumDays` | num_days must be greater than zero |
| 6005 | `InvalidTemplateId` | template_id must be greater than zero |
| 6006 | `EmptyDataTypes` | data_types must not be empty |
| 6007 | `TooManyDataTypes` | Too many data types (max 8) |
| 6008 | `InvalidMaxParticipants` | max_participants must be greater than zero |
| 6009 | `TooManyParticipants` | Too many selected participants (max 50) |
| 6010 | `InsufficientPayment` | Payment amount is less than the required final_total |
| 6011 | `ZeroFinalTotal` | final_total must be greater than zero before payment |
| 6012 | `EmptyResultCid` | result_cid must not be empty |
| 6013 | `NoParticipants` | No participants to distribute payout to |
| 6014 | `InsufficientEscrow` | Escrow vault has insufficient lamports |
| 6015 | `ZeroAmountPerProvider` | Computed amount_per_provider is zero |
| 6016 | `NotAParticipant` | Signer is not a selected participant for this job |
| 6017 | `ParticipantIndexOutOfRange` | Participant index exceeds bitmap capacity (max 64 providers) |
| 6018 | `AlreadyClaimed` | This provider has already claimed their payout |
| 6019 | `CannotCancelAtThisStage` | Job can only be cancelled in PENDING_PREFLIGHT or AWAITING_CONFIRMATION |
| 6020 | `SweepNotAllowed` | Not all providers have claimed and caller is not the researcher |
| 6021 | `Overflow` | Arithmetic overflow |
| 6022 | `InvalidOwner` | New owner cannot be the zero address |
| 6023 | `InvalidAuthority` | ROFL authority cannot be the zero address |
