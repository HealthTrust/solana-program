# HealthTrust Solana Programs — Reference Documentation

This document describes the three on-chain Anchor programs that form the HealthTrust
Solana back-end. It covers every instruction, every account, every event, and how to
call each instruction from a TypeScript/JavaScript web application using
`@coral-xyz/anchor`.

---

## Table of Contents

1. [Overview](#overview)
2. [Universal Conventions](#universal-conventions)
3. [Program: pricing_config](#program-pricing_config)
4. [Program: data_registry](#program-data_registry)
5. [Program: order_handler](#program-order_handler)
6. [Calling from a Web App (TypeScript)](#calling-from-a-web-app-typescript)
7. [Event Indexing Reference](#event-indexing-reference)
8. [Enum / Code Reference](#enum--code-reference)
9. [Error Reference](#error-reference)

---

## Overview

| Program | Responsibility | Source file |
|---|---|---|
| `pricing_config` | Global pricing parameters; per-data-type scarcity multipliers | `programs/pricing_config/src/lib.rs` |
| `data_registry` | Data provider uploads; metadata + upload unit lifecycle | `programs/data_registry/src/lib.rs` |
| `order_handler` | Research job lifecycle; SOL escrow; provider payouts | `programs/order_handler/src/lib.rs` |

The three programs are **independent** — they do not call each other via CPI in the
current version. Cross-program state updates that must be atomic (e.g., upload + pricing
multiplier update) are achieved by composing multiple instructions from different programs
into a single Solana transaction on the client side.

Program IDs are placeholders until `anchor build` + `anchor keys list` are run.
After that, update:
- Each `declare_id!()` at the top of every `lib.rs`
- The `[programs.localnet]` section in `Anchor.toml`

---

## Universal Conventions

### Data type hashes

All data type identifiers are passed as `[u8; 32]` arrays that are the
**sha256 hash of the data type string**, computed off-chain:

```typescript
import { createHash } from "crypto";

function dataTypeHash(name: string): number[] {
  return Array.from(createHash("sha256").update(name).digest());
}

// e.g.
const heartRateHash = dataTypeHash("heart_rate");  // number[32]
```

This convention is used in:
- `pricing_config`: TypeMultiplier PDA seeds
- `data_registry`: `DataEntryMeta.data_type_hashes` field
- `order_handler`: `Job.data_type_hashes` field

### Lamports

All monetary values are in **lamports** (1 SOL = 1,000,000,000 lamports).

### Timestamps

All timestamps are Unix seconds (`i64`).

### PDA derivation helper (TypeScript)

```typescript
import { PublicKey } from "@solana/web3.js";

// pricing_config
const [pricingParams]    = PublicKey.findProgramAddressSync([Buffer.from("pricing_params")], PRICING_CONFIG_PROGRAM_ID);
const [typeMultiplier]   = PublicKey.findProgramAddressSync([Buffer.from("type_multiplier"), Buffer.from(dataTypeHash)], PRICING_CONFIG_PROGRAM_ID);

// data_registry
const [registryState]    = PublicKey.findProgramAddressSync([Buffer.from("registry_state")], DATA_REGISTRY_PROGRAM_ID);
const [dataEntryMeta]    = PublicKey.findProgramAddressSync([Buffer.from("meta"), metaIdBuf], DATA_REGISTRY_PROGRAM_ID);
const [uploadUnit]       = PublicKey.findProgramAddressSync([Buffer.from("unit"), metaIdBuf, unitIndexBuf], DATA_REGISTRY_PROGRAM_ID);

// order_handler
const [orderConfig]      = PublicKey.findProgramAddressSync([Buffer.from("order_config")], ORDER_HANDLER_PROGRAM_ID);
const [job]              = PublicKey.findProgramAddressSync([Buffer.from("job"), jobIdBuf], ORDER_HANDLER_PROGRAM_ID);
const [escrowVault]      = PublicKey.findProgramAddressSync([Buffer.from("escrow"), jobIdBuf], ORDER_HANDLER_PROGRAM_ID);

// u64 → 8-byte little-endian Buffer
function u64ToLEBuf(n: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(n));
  return buf;
}
// u32 → 4-byte little-endian Buffer
function u32ToLEBuf(n: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(n);
  return buf;
}
```

---

## Program: pricing_config

Manages the global pricing parameters and per-data-type scarcity multipliers.

### Accounts

#### `PricingParams`
**PDA seeds:** `[b"pricing_params"]`  
**Size:** 82 bytes  
**One per deployment (singleton).**

| Field | Type | Description |
|---|---|---|
| `owner` | `Pubkey` | Admin wallet; required for all write operations |
| `base_price` | `u64` | Per-participant base charge (lamports) |
| `duration_factor` | `u64` | Per-participant per-day multiplier (lamports) |
| `platform_fee_bps` | `u16` | Platform fee in basis points (max 3000 = 30%) |
| `min_total_charge` | `u64` | Minimum total job charge (lamports) |
| `preflight_fee` | `u64` | Flat fee charged for running a preflight (lamports) |
| `bump` | `u8` | PDA canonical bump |

#### `TypeMultiplier`
**PDA seeds:** `[b"type_multiplier", data_type_hash[..32]]`  
**Size:** 65 bytes  
**One per data type.**

| Field | Type | Description |
|---|---|---|
| `data_type_hash` | `[u8; 32]` | sha256 of the data type string |
| `total_duration` | `u64` | Cumulative seconds of data for this type (ever uploaded) |
| `multiplier` | `u64` | Scarcity multiplier in fixed-point (denominator = 1,000,000,000). Formula: `1e9 / total_duration`. Higher = more scarce. |
| `bump` | `u8` | PDA canonical bump |

---

### Instructions

#### `initialize_pricing`
**Who calls it:** Protocol deployer / admin (one time)  
**Signer:** `owner`

Creates the `PricingParams` singleton. Fails if already initialized.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `pricing_params` | ✓ | — | PDA being initialized |
| `owner` | ✓ | ✓ | Payer and future admin |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `params.base_price` | `u64` | lamports per participant |
| `params.duration_factor` | `u64` | lamports per participant per day |
| `params.platform_fee_bps` | `u16` | platform fee (0–3000) |
| `params.min_total_charge` | `u64` | minimum total job charge (lamports) |
| `params.preflight_fee` | `u64` | flat preflight fee (lamports) |

**Emits:** `PricingInitialized`

---

#### `update_pricing_params`
**Who calls it:** Admin only  
**Signer:** `owner` (must match `pricing_params.owner`)

Atomically updates all five pricing parameters.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `pricing_params` | ✓ | — | Singleton PDA |
| `owner` | — | ✓ | Must equal `pricing_params.owner` |

**Instruction arguments:** Same `params` struct as `initialize_pricing`.

**Emits:** `PricingUpdated`

---

#### `transfer_pricing_ownership`
**Who calls it:** Current admin  
**Signer:** `owner`

Transfers admin rights to a new pubkey.

**Accounts required:** Same as `update_pricing_params`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `new_owner` | `Pubkey` | New admin wallet |

**Emits:** `OwnershipTransferred`

---

#### `initialize_type_multiplier`
**Who calls it:** Admin only (must be done once per data type before uploads use it)  
**Signer:** `owner`

Creates the `TypeMultiplier` PDA for a new data type. Only the admin can do this, which
gates which data types are allowed in the system.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `type_multiplier` | ✓ | — | PDA being initialized |
| `pricing_params` | — | — | Read to verify `owner` |
| `owner` | ✓ | ✓ | Payer and admin check |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `data_type_hash` | `[u8; 32]` | sha256 of the data type string |

> **No event emitted.** Monitor account creation on-chain instead.

---

#### `update_multiplier_for_scarcity`
**Who calls it:** Any signer (open — same as original EVM behavior)  
**Signer:** `caller`

Adds `added_duration` seconds to a `TypeMultiplier`'s total and recomputes its
multiplier. Called once per data type per upload transaction.

**To update N types atomically:** include N separate `update_multiplier_for_scarcity`
instructions in the same transaction, one per data type PDA.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `type_multiplier` | ✓ | — | The PDA for the specific data type hash |
| `caller` | — | ✓ | Any wallet |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `data_type_hash` | `[u8; 32]` | Identifies the TypeMultiplier PDA (must match seeds) |
| `added_duration` | `u64` | Seconds of new data being reported |

**Emits:** `MultiplierUpdated`

---

### Events — pricing_config

#### `PricingInitialized`
Fired once when the PricingParams singleton is first created.
```
owner:              Pubkey
base_price:         u64
duration_factor:    u64
platform_fee_bps:   u16
min_total_charge:   u64
preflight_fee:      u64
```

#### `PricingUpdated`
Fired every time pricing params are changed.
```
base_price:         u64
duration_factor:    u64
platform_fee_bps:   u16
min_total_charge:   u64
preflight_fee:      u64
```

#### `MultiplierUpdated`
Fired every time a TypeMultiplier is updated. Key event for indexing scarcity over time.
```
data_type_hash:   [u8; 32]  — sha256 of type string; reverse-lookup in your DB
total_duration:   u64        — cumulative seconds of this data type ever uploaded
multiplier:       u64        — new multiplier (fixed-point, denominator 1e9)
```

#### `OwnershipTransferred` (pricing_config)
```
previous:  Pubkey
new:       Pubkey
```

---

## Program: data_registry

Manages the data provider lifecycle: creating metadata entries, appending uploads,
and the TEE processing them into feature CIDs.

### Accounts

#### `RegistryState`
**PDA seeds:** `[b"registry_state"]`  
**Size:** 114 bytes  
**Singleton.**

| Field | Type | Description |
|---|---|---|
| `owner` | `Pubkey` | Admin wallet |
| `pricing_program` | `Pubkey` | Address of the pricing_config program (informational) |
| `tee_authority` | `Pubkey` | The only wallet allowed to call `update_upload_unit` |
| `next_meta_id` | `u64` | Counter for the next `DataEntryMeta` ID (starts at 1) |
| `paused` | `bool` | If true, all data-provider instructions revert |
| `bump` | `u8` | PDA canonical bump |

#### `DataEntryMeta`
**PDA seeds:** `[b"meta", meta_id.to_le_bytes()]`  
**Size:** 505 bytes  
**One per data provider dataset grouping.**

| Field | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Unique identifier (from registry counter) |
| `owner` | `Pubkey` | Data provider wallet |
| `device_type` | `String` | e.g. "smartphone", "wearable" (max 32 bytes) |
| `device_model` | `String` | e.g. "Galaxy Watch 6" (max 48 bytes) |
| `service_provider` | `String` | e.g. "Apple Health" (max 48 bytes) |
| `age` | `u8` | Age bracket code (see enum table below) |
| `gender` | `u8` | Gender code |
| `height` | `u8` | Height code |
| `weight` | `u8` | Weight code |
| `region` | `u8` | Region code |
| `physical_activity_level` | `u8` | Activity level code |
| `smoker` | `u8` | 0 = no, 1 = yes |
| `diet` | `u8` | Diet code |
| `chronic_conditions` | `Vec<u8>` | Array of condition codes (max 16) |
| `data_type_hashes` | `Vec<[u8;32]>` | sha256 hashes of data type strings (max 8) |
| `total_duration` | `u64` | Cumulative seconds of data across all units |
| `unit_count` | `u32` | Number of UploadUnit PDAs under this meta |
| `date_of_creation` | `i64` | Unix timestamp |
| `date_of_modification` | `i64` | Unix timestamp of last unit append |
| `bump` | `u8` | PDA canonical bump |

#### `UploadUnit`
**PDA seeds:** `[b"unit", meta_id.to_le_bytes(), unit_index.to_le_bytes()]`  
**Size:** 181 bytes  
**One per upload. unit_index is zero-based and per-meta.**

| Field | Type | Description |
|---|---|---|
| `meta_id` | `u64` | Parent meta ID |
| `unit_index` | `u32` | Index within this meta (0, 1, 2, …) |
| `raw_cid` | `String` | IPFS CID of the raw data blob (max 64 bytes) |
| `day_start_timestamp` | `i64` | Unix start of the data collection window |
| `day_end_timestamp` | `i64` | Unix end of the data collection window |
| `feat_cid` | `String` | IPFS CID of the TEE-processed feature pack (empty until set by TEE, max 64 bytes) |
| `date_of_creation` | `i64` | Unix timestamp |
| `bump` | `u8` | PDA canonical bump |

To enumerate all units for a given meta: iterate `unit_index` from `0` to
`data_entry_meta.unit_count - 1` and derive each PDA.

---

### Instructions

#### `initialize_registry`
**Who calls it:** Protocol deployer (one time)  
**Signer:** `owner`

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Singleton PDA being created |
| `owner` | ✓ | ✓ | Payer and admin |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `tee_authority` | `Pubkey` | Wallet of the TEE/ROFL worker allowed to set feat_cids |

---

#### `set_pricing_program`
**Who calls it:** Admin  
**Signer:** `owner`

Stores the pricing_config program ID in RegistryState for off-chain reference.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | ✓ | — | Singleton |
| `owner` | — | ✓ | Must equal `registry_state.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `pricing_program` | `Pubkey` | Address of the deployed pricing_config program |

---

#### `set_tee_authority`
**Who calls it:** Admin  
**Signer:** `owner`

Updates the address that is allowed to call `update_upload_unit`.

**Accounts required:** Same as `set_pricing_program`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `tee_authority` | `Pubkey` | New TEE/ROFL authority wallet |

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
| `new_owner` | `Pubkey` | New admin |

**Emits:** `OwnershipTransferred`

---

#### `emergency_withdraw_sol`
**Who calls it:** Admin only  
**Signer:** `owner`

Withdraws accidentally-sent SOL from the RegistryState PDA.

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

**After calling this instruction**, the client MUST also include one
`pricing_config::update_multiplier_for_scarcity` instruction per data type in the
**same transaction** to keep pricing multipliers accurate.

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
| `data_type_hashes` | `Vec<[u8;32]>` | 8 entries | sha256 hashes of data type strings |
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
| `smoker` | `u8` | — | 0 = non-smoker, 1 = smoker |
| `diet` | `u8` | — | Diet code |
| `chronic_conditions` | `Vec<u8>` | 16 entries | Chronic condition codes |

**Emits (in order):** `MetaEntryCreated`, `MetaAttributes`, `MetaDeviceInfo`, `UploadUnitCreated`

---

#### `register_raw_upload`
**Who calls it:** Data provider  
**Signer:** `provider`

Appends a new `UploadUnit` to an existing `DataEntryMeta`. The provider must own the meta.

**After calling this instruction**, include one `pricing_config::update_multiplier_for_scarcity`
per data type in the **same transaction**.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `registry_state` | — | — | Read to check paused flag |
| `data_entry_meta` | ✓ | — | PDA: seeds `["meta", meta_id_le8]` |
| `upload_unit` | ✓ | — | New PDA: seeds `["unit", meta_id_le8, unit_count_le4]` |
| `provider` | ✓ | ✓ | Must equal `data_entry_meta.owner`; pays rent |
| `system_program` | — | — | `11111111111111111111111111111111` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `meta_id` | `u64` | ID of the existing DataEntryMeta (used in PDA seeds) |
| `raw_cid` | `String` | IPFS CID of the new raw data blob (max 64 bytes) |
| `day_start_timestamp` | `i64` | Start of new data window |
| `day_end_timestamp` | `i64` | End of new data window (must be > start) |

**Emits:** `UploadUnitCreated`, `DataStored`

---

#### `update_upload_unit`
**Who calls it:** TEE/ROFL worker only (must match `registry_state.tee_authority`)  
**Signer:** `tee_authority`

Sets the `feat_cid` on an existing `UploadUnit` after the TEE has processed the raw data.
Caller must pass both `meta_id` and `unit_index` — the program derives the exact PDA
and validates it (no scanning).

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
| `feat_cid` | `String` | IPFS CID of the processed feature pack (max 64 bytes) |

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

Closes a single `UploadUnit` PDA and returns rent.

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

### Events — data_registry

#### `MetaEntryCreated`
Fired when a new DataEntryMeta is created. Index this to build a dataset catalog.
```
meta_id:            u64
owner:              Pubkey   — data provider wallet
data_type_hashes:   [u8;32][] — list of sha256 data type hashes
total_duration:     u64       — seconds (day_end - day_start of first unit)
date_of_creation:   i64       — Unix timestamp
```

#### `MetaAttributes`
Fired alongside MetaEntryCreated. Contains demographic/health attributes.
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
Fired alongside MetaEntryCreated. Contains device details.
```
meta_id:          u64
device_type:      String
device_model:     String
service_provider: String
```

#### `UploadUnitCreated`
Fired when any UploadUnit is created (on `upload_new_meta` and `register_raw_upload`).
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
total_duration:        u64   — meta's total after this upload
added_duration:        u64   — seconds added by this upload
```

#### `DataEntryVersionUpdated`
Fired when TEE sets feat_cid. Key event: indicates data is ready for researcher access.
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

#### `PausedStateChanged` (data_registry)
```
paused:  bool
```

#### `OwnershipTransferred` (data_registry)
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

## Program: order_handler

Manages the full research job lifecycle including SOL escrow and provider payouts.

### Job State Machine

```
request_job
    │
    ▼
PENDING_PREFLIGHT
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
    └──► sweep_vault_dust  (researcher, after all claims)
```

### Accounts

#### `OrderConfig`
**PDA seeds:** `[b"order_config"]`  
**Size:** 81 bytes  
**Singleton.**

| Field | Type | Description |
|---|---|---|
| `owner` | `Pubkey` | Admin wallet |
| `rofl_authority` | `Pubkey` | The ROFL/TEE worker address; only it can submit preflight/result/finalize |
| `next_job_id` | `u64` | Monotonically increasing job counter (starts at 1) |
| `bump` | `u8` | PDA canonical bump |

#### `Job`
**PDA seeds:** `[b"job", job_id.to_le_bytes()]`  
**Size:** ~2,277 bytes (~0.016 SOL rent)

| Field | Type | Description |
|---|---|---|
| `job_id` | `u64` | Unique job identifier |
| `researcher` | `Pubkey` | Wallet that created the job |
| `status` | `JobStatus` | Current lifecycle state (see enum below) |
| `template_id` | `u32` | Job template ID (> 0) |
| `num_days` | `u32` | Duration of the data window requested |
| `data_type_hashes` | `Vec<[u8;32]>` | sha256 hashes of requested data types (max 8) |
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
| `claimed_bitmap` | `u64` | Bit i = 1 means `selected_participants[i]` has claimed |
| `created_at` | `i64` | Unix timestamp |
| `updated_at` | `i64` | Unix timestamp of last state change |
| `bump` | `u8` | PDA canonical bump |

#### `EscrowVault`
**PDA seeds:** `[b"escrow", job_id.to_le_bytes()]`  
**Size:** 17 bytes (program-owned; holds job payment lamports)

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
| `rofl_authority` | `Pubkey` | ROFL/TEE worker wallet |

---

#### `set_rofl_authority`
**Who calls it:** Admin  
**Signer:** `owner`

Updates the ROFL authority address.

**Accounts required:**

| Account | Writable | Signer | Description |
|---|---|---|---|
| `order_config` | ✓ | — | Singleton |
| `owner` | — | ✓ | Must equal `order_config.owner` |

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `rofl_authority` | `Pubkey` | New ROFL worker wallet |

**Emits:** `RoflAuthorityUpdated`

---

#### `transfer_order_ownership`
**Who calls it:** Current admin  
**Signer:** `owner`

**Accounts required:** Same as `set_rofl_authority`.

**Instruction arguments:**

| Argument | Type | Description |
|---|---|---|
| `new_owner` | `Pubkey` | New admin wallet |

**Emits:** `OwnershipTransferred`

---

#### `request_job`
**Who calls it:** Researcher  
**Signer:** `researcher`

Phase 1 of the job lifecycle. Allocates the Job PDA (researcher pays ~0.016 SOL rent).
No payment at this stage.

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
| `data_type_hashes` | `Vec<[u8;32]>` | 1–8 entries | sha256 hashes of requested data types |
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
Status transitions: `PENDING_PREFLIGHT` → `AWAITING_CONFIRMATION`.

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

Phase 3. Researcher reviews the preflight and sends payment. Creates the EscrowVault.
Status transitions: `AWAITING_CONFIRMATION` → `CONFIRMED`.

Researcher pays:
- EscrowVault rent (~0.001 SOL, one time)
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
payment). Closes the Job PDA and returns rent to the researcher.

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
Status transitions: `CONFIRMED` → `EXECUTED`.

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
| `result_cid` | `String` | IPFS CID of the result (max 64 bytes) |
| `output_hash` | `[u8; 32]` | sha256 of plaintext output or attestation |

**Emits:** `JobExecuted`

---

#### `finalize_job`
**Who calls it:** ROFL/TEE worker only  
**Signer:** `rofl_authority`

Phase 5. Computes `amount_per_provider = (vault_balance - vault_rent) / num_providers`
and sets status to COMPLETED. Does **not** push payments — providers must call
`claim_payout` individually.

Status transitions: `EXECUTED` → `COMPLETED`.

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
Callable any time after job is COMPLETED.

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
**Who calls it:** Researcher (or after all providers have claimed)  
**Signer:** not required — validated by logic

Sweeps leftover lamports (overpayment + integer division rounding) from the vault to a
recipient. Allowed if: all providers have claimed, OR the recipient is the researcher.

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

### Events — order_handler

#### `JobRequested`
Fired on every new job. Index this to build a jobs table.
```
job_id:             u64
researcher:         Pubkey
template_id:        u32
num_days:           u32
data_type_hashes:   [u8;32][]
max_participants:   u32
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

#### `OwnershipTransferred` (order_handler)
```
previous:  Pubkey
new:       Pubkey
```

---

## Calling from a Web App (TypeScript)

### Setup

```typescript
import { AnchorProvider, Program, setProvider, web3 } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram } from "@solana/web3.js";
import { createHash } from "crypto";

// Replace with real program IDs from `anchor keys list`
const PRICING_CONFIG_ID  = new PublicKey("PCFGxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
const DATA_REGISTRY_ID   = new PublicKey("DREGxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
const ORDER_HANDLER_ID   = new PublicKey("ORDRxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");

const connection = new Connection("https://api.devnet.solana.com");
const provider   = AnchorProvider.env();
setProvider(provider);

// Load IDLs (generated by `anchor build`, found in target/idl/)
import pricingIdl   from "./target/idl/pricing_config.json";
import registryIdl  from "./target/idl/data_registry.json";
import orderIdl     from "./target/idl/order_handler.json";

const pricingProgram  = new Program(pricingIdl  as any, provider);
const registryProgram = new Program(registryIdl as any, provider);
const orderProgram    = new Program(orderIdl    as any, provider);

// ─── PDA helpers ─────────────────────────────────────────────────────────────

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
function dataTypeHash(typeName: string): Buffer {
  return createHash("sha256").update(typeName).digest();
}

function pricingParamsPDA() {
  return PublicKey.findProgramAddressSync([Buffer.from("pricing_params")], PRICING_CONFIG_ID);
}
function typeMultiplierPDA(hash: Buffer) {
  return PublicKey.findProgramAddressSync([Buffer.from("type_multiplier"), hash], PRICING_CONFIG_ID);
}
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

### Example: Upload new meta + update multipliers atomically

```typescript
async function uploadNewMeta(provider: AnchorProvider, params: {
  rawCid: string;
  dataTypes: string[];       // human-readable, e.g. ["heart_rate", "steps"]
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
  const hashes = params.dataTypes.map(t => Array.from(dataTypeHash(t)));
  const duration = BigInt(params.dayEnd - params.dayStart);

  // Read current next_meta_id to derive PDAs
  const state = await registryProgram.account.registryState.fetch(registryState);
  const metaId = state.nextMetaId as bigint;
  const [metaPDA]     = dataEntryMetaPDA(metaId);
  const [unitPDA]     = uploadUnitPDA(metaId, 0);
  const [pricingParams] = pricingParamsPDA();

  // Build all instructions for a single atomic transaction
  const ixs = [];

  // 1. Create meta + first unit
  ixs.push(await registryProgram.methods
    .uploadNewMeta({
      rawCid: params.rawCid,
      dataTypeHashes: hashes,
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
    .instruction());

  // 2. Update multiplier for each data type in the same tx
  for (const typeName of params.dataTypes) {
    const hash = dataTypeHash(typeName);
    const [tmPDA] = typeMultiplierPDA(hash);
    ixs.push(await pricingProgram.methods
      .updateMultiplierForScarcity(Array.from(hash), new BN(duration.toString()))
      .accounts({
        typeMultiplier: tmPDA,
        caller: provider.wallet.publicKey,
      })
      .instruction());
  }

  const tx = new web3.Transaction().add(...ixs);
  const sig = await provider.sendAndConfirm(tx);
  return { sig, metaId };
}
```

---

### Example: Researcher requests a job

```typescript
async function requestJob(provider: AnchorProvider, params: {
  templateId: number;
  numDays: number;
  dataTypes: string[];
  maxParticipants: number;
  startDayUtc: number;
  filterQuery: string;
}) {
  const [orderConfig] = orderConfigPDA();
  const cfg = await orderProgram.account.orderConfig.fetch(orderConfig);
  const jobId = cfg.nextJobId as bigint;
  const [jobPubkey] = jobPDA(jobId);

  const hashes = params.dataTypes.map(t => Array.from(dataTypeHash(t)));

  const sig = await orderProgram.methods
    .requestJob({
      templateId: params.templateId,
      numDays: params.numDays,
      dataTypeHashes: hashes,
      maxParticipants: params.maxParticipants,
      startDayUtc: new BN(params.startDayUtc),
      filterQuery: params.filterQuery,
    })
    .accounts({
      orderConfig,
      job: jobPubkey,
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
async function confirmAndPay(provider: AnchorProvider, jobId: bigint, paymentLamports: bigint) {
  const [jobPubkey]   = jobPDA(jobId);
  const [vaultPubkey] = escrowVaultPDA(jobId);

  const sig = await orderProgram.methods
    .confirmJobAndPay(new BN(jobId.toString()), new BN(paymentLamports.toString()))
    .accounts({
      job:          jobPubkey,
      escrowVault:  vaultPubkey,
      researcher:   provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();

  return sig;
}
```

---

### Example: Data provider claims payout

```typescript
async function claimPayout(provider: AnchorProvider, jobId: bigint) {
  const [jobPubkey]   = jobPDA(jobId);
  const [vaultPubkey] = escrowVaultPDA(jobId);

  const sig = await orderProgram.methods
    .claimPayout(new BN(jobId.toString()))
    .accounts({
      job:          jobPubkey,
      escrowVault:  vaultPubkey,
      provider:     provider.wallet.publicKey,
    })
    .rpc();

  return sig;
}
```

---

### Example: Fetch account data

```typescript
// Read pricing params
const [pp] = pricingParamsPDA();
const pricing = await pricingProgram.account.pricingParams.fetch(pp);
console.log("base_price:", pricing.basePrice.toString(), "lamports");

// Read a TypeMultiplier
const hash = dataTypeHash("heart_rate");
const [tm] = typeMultiplierPDA(hash);
const mult = await pricingProgram.account.typeMultiplier.fetch(tm);
console.log("heart_rate multiplier:", mult.multiplier.toString(), "/ 1e9");

// Read a DataEntryMeta
const [meta] = dataEntryMetaPDA(1n);
const metaData = await registryProgram.account.dataEntryMeta.fetch(meta);
console.log("meta owner:", metaData.owner.toBase58());
console.log("unit_count:", metaData.unitCount);

// Read a Job
const [j] = jobPDA(1n);
const job = await orderProgram.account.job.fetch(j);
console.log("job status:", job.status);
console.log("selected participants:", job.selectedParticipants.map(p => p.toBase58()));
```

---

## Event Indexing Reference

Anchor emits events as log messages in transaction logs. To subscribe and index:

```typescript
// Subscribe to a program's events (Anchor 0.30+)
registryProgram.addEventListener("MetaEntryCreated", (event, slot, sig) => {
  console.log("New meta:", event.metaId.toString());
  // Persist to your database here
});

orderProgram.addEventListener("JobCompleted", (event, slot, sig) => {
  console.log("Job completed:", event.jobId.toString());
  console.log("Amount per provider:", event.amountPerProvider.toString(), "lamports");
});
```

### Recommended indexing table → event mapping

| Table | Primary events | Secondary events |
|---|---|---|
| `datasets` | `MetaEntryCreated` | `DataEntryDeleted` |
| `dataset_attributes` | `MetaAttributes` | — |
| `dataset_devices` | `MetaDeviceInfo` | — |
| `upload_units` | `UploadUnitCreated` | `DataEntryVersionUpdated`, `UploadUnitClosed` |
| `data_activity` | `DataStored` | — |
| `pricing_params` | `PricingInitialized`, `PricingUpdated` | — |
| `type_multipliers` | `MultiplierUpdated` | — |
| `jobs` | `JobRequested` | `JobConfirmed`, `JobCancelled`, `JobExecuted`, `JobCompleted` |
| `job_payouts` | `DataProviderPaid` | `VaultDustSwept` |
| `admin_actions` | `OwnershipTransferred` (all programs), `PausedStateChanged`, `RoflAuthorityUpdated`, `EmergencyWithdrawal` | — |

### Reading historical events (without websocket)

```typescript
// Fetch and parse transaction logs for past events
const sigs = await connection.getSignaturesForAddress(DATA_REGISTRY_ID, { limit: 100 });
for (const { signature } of sigs) {
  const tx = await connection.getParsedTransaction(signature, { commitment: "confirmed" });
  // Parse logs using the Anchor event coder from the IDL
  const eventCoder = new anchor.BorshEventCoder(registryIdl as any);
  for (const log of tx?.meta?.logMessages ?? []) {
    if (log.startsWith("Program data:")) {
      const base64Data = log.slice("Program data: ".length);
      try {
        const event = eventCoder.decode(base64Data);
        if (event) console.log(event.name, event.data);
      } catch (_) {}
    }
  }
}
```

---

## Enum / Code Reference

### `JobStatus` (order_handler)

| Value | Name | Description |
|---|---|---|
| 0 | `None` | Uninitialized (should never appear in a live account) |
| 1 | `PendingPreflight` | Job created, waiting for ROFL preflight |
| 2 | `AwaitingConfirmation` | Preflight done, waiting for researcher payment |
| 3 | `Confirmed` | Paid and locked, ROFL running computation |
| 4 | `Executed` | Computation done, result CID submitted |
| 5 | `Completed` | Finalized, providers may claim payout |
| 6 | `Cancelled` | Cancelled by researcher (no payment taken) |

### Attribute codes (data_registry)

These mirror the original EVM Attributes struct. Extend as needed in your front-end.

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

**physical_activity_level**
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

### pricing_config errors

| Code | Name | Message |
|---|---|---|
| 6000 | `FeeTooHigh` | Platform fee exceeds maximum of 30% |
| 6001 | `Unauthorized` | Caller is not the pricing owner |
| 6002 | `ZeroDuration` | Added duration must be greater than zero |
| 6003 | `Overflow` | Arithmetic overflow |
| 6004 | `InvalidOwner` | New owner cannot be the zero address |

### data_registry errors

| Code | Name | Message |
|---|---|---|
| 6000 | `Paused` | Registry is paused |
| 6001 | `Unauthorized` | Caller is not the registry owner |
| 6002 | `NotOwner` | Caller is not the meta entry owner |
| 6003 | `NotTeeAuthority` | Caller is not the TEE authority |
| 6004 | `EmptyDataTypes` | data_type_hashes must not be empty |
| 6005 | `TooManyDataTypes` | Too many data types (max 8) |
| 6006 | `TooManyConditions` | Too many chronic conditions (max 16) |
| 6007 | `InvalidTimestampRange` | day_end_timestamp must be greater than day_start_timestamp |
| 6008 | `EmptyFeatCid` | feat_cid must not be empty |
| 6009 | `Overflow` | Arithmetic overflow |
| 6010 | `InvalidOwner` | New owner cannot be the zero address |
| 6011 | `InvalidAuthority` | TEE authority cannot be the zero address |

### order_handler errors

| Code | Name | Message |
|---|---|---|
| 6000 | `Unauthorized` | Caller is not the order handler owner |
| 6001 | `NotRoflAuthority` | Caller is not the ROFL authority |
| 6002 | `InvalidStatus` | Job status does not allow this operation |
| 6003 | `JobIdMismatch` | Job ID in instruction does not match account |
| 6004 | `InvalidNumDays` | num_days must be greater than zero |
| 6005 | `InvalidTemplateId` | template_id must be greater than zero |
| 6006 | `EmptyDataTypes` | data_type_hashes must not be empty |
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
| 6017 | `ParticipantIndexOutOfRange` | Participant index exceeds bitmap capacity |
| 6018 | `AlreadyClaimed` | This provider has already claimed their payout |
| 6019 | `CannotCancelAtThisStage` | Job can only be cancelled in PENDING_PREFLIGHT or AWAITING_CONFIRMATION |
| 6020 | `SweepNotAllowed` | Sweep not allowed: not all providers have claimed and caller is not the researcher |
| 6021 | `Overflow` | Arithmetic overflow |
| 6022 | `InvalidOwner` | New owner cannot be the zero address |
| 6023 | `InvalidAuthority` | ROFL authority cannot be the zero address |
