import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { expect } from "chai";

import { DataRegistry } from "../target/types/data_registry";

// Prefer a workspace-local wallet when present so WSL/Windows paths are stable.
const workspaceWalletPath = path.resolve(__dirname, "../.anchor/wsl-id.json");

// Default to local validator + local wallet to keep tests deterministic.
process.env.ANCHOR_PROVIDER_URL ??= "http://127.0.0.1:8899";
process.env.ANCHOR_WALLET ??=
  fs.existsSync(workspaceWalletPath)
    ? workspaceWalletPath
    : `${process.env.HOME ?? ""}/.config/solana/id.json`;

// Anchor provider and generated typed program client.
const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.DataRegistry as Program<DataRegistry>;

// PDA seeds must match on-chain account constraints.
const registrySeed = Buffer.from("registry_state");
const metaSeed = Buffer.from("meta");
const unitSeed = Buffer.from("unit");

// Fixed demographic values keep assertions stable across runs.
const attributes = {
  age: 1,
  gender: 1,
  height: 170,
  weight: 65,
  region: 1,
  physicalActivityLevel: 1,
  smoker: 0,
  diet: 1,
  chronicConditions: [2, 1, 4],
};

const chronicConditionsBuffer = Buffer.from(attributes.chronicConditions);

// Fixed device metadata used in upload_new_meta.
const device = {
  deviceType: "Smartwatch",
  deviceModel: "Fitbit Charge 5",
  serviceProvider: "ConfiState Health",
};

// PDA seed helpers: convert JS numbers/BN into canonical little-endian bytes.
function metaIdBuffer(metaId: anchor.BN): Buffer {
  return metaId.toArrayLike(Buffer, "le", 8);
}

function unitIndexBuffer(unitIndex: number): Buffer {
  const bn = new anchor.BN(unitIndex);
  return bn.toArrayLike(Buffer, "le", 4);
}

// Registry is a singleton PDA at seed "registry_state".
function deriveRegistryStatePda(): PublicKey {
  return PublicKey.findProgramAddressSync([registrySeed], program.programId)[0];
}

// Meta account PDA: ["meta", meta_id_le_bytes].
function deriveMetaPda(metaId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [metaSeed, metaIdBuffer(metaId)],
    program.programId
  )[0];
}

// Upload unit PDA: ["unit", meta_id_le_bytes, unit_index_le_bytes].
function deriveUnitPda(metaId: anchor.BN, unitIndex: number): PublicKey {
  return PublicKey.findProgramAddressSync(
    [unitSeed, metaIdBuffer(metaId), unitIndexBuffer(unitIndex)],
    program.programId
  )[0];
}

// Test helpers use fresh keypairs that need SOL for fees/rent.
async function fundKeypair(keypair: anchor.web3.Keypair) {
  const signature = await provider.connection.requestAirdrop(
    keypair.publicKey,
    2 * anchor.web3.LAMPORTS_PER_SOL
  );

  const latestBlockhash = await provider.connection.getLatestBlockhash();
  await provider.connection.confirmTransaction(
    {
      signature,
      ...latestBlockhash,
    },
    "confirmed"
  );
}

// Shared helper for negative-path assertions on Anchor errors.
async function expectAnchorError(
  fn: () => Promise<unknown>,
  message: string
): Promise<void> {
  try {
    await fn();
    expect.fail(`Expected error containing "${message}"`);
  } catch (error) {
    const rendered =
      error instanceof Error ? error.message : JSON.stringify(error);
    expect(rendered).to.include(message);
  }
}

// Ensure test preconditions: registry exists, tee authority is expected, pause is off.
async function ensureRegistryInitialized(
  teeAuthority: PublicKey
): Promise<PublicKey> {
  const registryState = deriveRegistryStatePda();
  const existing = await program.account.registryState.fetchNullable(
    registryState
  );

  if (!existing) {
    await program.methods
      .initializeRegistry(teeAuthority)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return registryState;
  }

  const currentTee = new PublicKey(existing.teeAuthority);
  if (!currentTee.equals(teeAuthority)) {
    await program.methods
      .setTeeAuthority(teeAuthority)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  }

  const isPaused = Boolean(existing.paused);
  if (isPaused) {
    await program.methods
      .setPaused(false)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  }

  return registryState;
}

// Creates one meta entry + its initial upload unit by calling upload_new_meta on-chain.
async function createMetaEntry(
  dataProvider: anchor.web3.Keypair,
  registryState: PublicKey,
  durationSeconds = 86_400
) {
  // Read current counter from on-chain state; this becomes meta_id for this tx.
  const registryBefore = await program.account.registryState.fetch(registryState);
  const metaId = registryBefore.nextMetaId as anchor.BN;

  // Derive deterministic addresses expected by account constraints.
  const dataEntryMeta = deriveMetaPda(metaId);
  const uploadUnit = deriveUnitPda(metaId, 0);
  const now = Math.floor(Date.now() / 1000);

  // Client args mirror UploadNewMetaParams in the Rust program.
  const params = {
    rawCid: "QmY7z5f8b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1",
    dataTypes: ["sleep", "heart_rate"],
    deviceType: device.deviceType,
    deviceModel: device.deviceModel,
    serviceProvider: device.serviceProvider,
    dayStartTimestamp: new anchor.BN(now),
    dayEndTimestamp: new anchor.BN(now + durationSeconds),
    age: attributes.age,
    gender: attributes.gender,
    height: attributes.height,
    weight: attributes.weight,
    region: attributes.region,
    physicalActivityLevel: attributes.physicalActivityLevel,
    smoker: attributes.smoker,
    diet: attributes.diet,
    chronicConditions: chronicConditionsBuffer,
  };

  // Real transaction to local validator. This executes the Rust program.
  await program.methods
    .uploadNewMeta(params)
    .accountsStrict({
      registryState,
      dataEntryMeta,
      uploadUnit,
      provider: dataProvider.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([dataProvider])
    .rpc();

  // Return derived addresses + args so assertions can cross-check persisted state.
  return {
    metaId,
    dataEntryMeta,
    uploadUnit,
    params,
  };
}

describe("data_registry migration parity", () => {
  // Actors used throughout scenarios.
  const teeAuthority = anchor.web3.Keypair.generate();
  const providerOne = anchor.web3.Keypair.generate();
  const providerTwo = anchor.web3.Keypair.generate();
  let registryState: PublicKey;

  before(async () => {
    // Fund all participants once, then initialize/normalize registry state.
    await Promise.all([
      fundKeypair(teeAuthority),
      fundKeypair(providerOne),
      fundKeypair(providerTwo),
    ]);
    registryState = await ensureRegistryInitialized(teeAuthority.publicKey);
  });

  beforeEach(async () => {
    // Keep each test independent from pause/authority drift.
    await ensureRegistryInitialized(teeAuthority.publicKey);
  });

  it("creates a new metadata entry with the first upload unit", async () => {
    // Act: execute upload_new_meta.
    const created = await createMetaEntry(providerOne, registryState);

    // Observe: fetch accounts mutated by the instruction.
    const meta = await program.account.dataEntryMeta.fetch(created.dataEntryMeta);
    const unit = await program.account.uploadUnit.fetch(created.uploadUnit);
    const registry = await program.account.registryState.fetch(registryState);

    // Assert: meta account fields and counters are initialized correctly.
    expect(meta.metaId.toString()).to.equal(created.metaId.toString());
    expect(new PublicKey(meta.owner).equals(providerOne.publicKey)).to.equal(true);
    expect(meta.deviceType).to.equal(device.deviceType);
    expect(meta.deviceModel).to.equal(device.deviceModel);
    expect(meta.serviceProvider).to.equal(device.serviceProvider);
    expect(meta.totalDuration.toNumber()).to.equal(86_400);
    expect(meta.unitCount).to.equal(1);
    expect(meta.dataTypes).to.deep.equal(created.params.dataTypes);

    // Assert: first upload unit is created as index 0.
    expect(unit.metaId.toString()).to.equal(created.metaId.toString());
    expect(unit.unitIndex).to.equal(0);
    expect(unit.rawCid).to.equal(created.params.rawCid);
    expect(unit.featCid).to.equal("");

    // Assert: global counter advanced for the next meta entry.
    expect(registry.nextMetaId.toNumber()).to.equal(
      created.metaId.toNumber() + 1
    );
  });

  it("appends raw uploads and keeps total duration and unit count in sync", async () => {
    // Seed with initial unit 0.
    const created = await createMetaEntry(providerOne, registryState);
    const secondUnit = deriveUnitPda(created.metaId, 1);
    const thirdUnit = deriveUnitPda(created.metaId, 2);

    // Append unit 1.
    await program.methods
      .registerRawUpload(
        created.metaId,
        "QmAppendOne111111111111111111111111111111111111111",
        new anchor.BN(10_000),
        new anchor.BN(10_000 + 86_400)
      )
      .accountsStrict({
        registryState,
        dataEntryMeta: created.dataEntryMeta,
        uploadUnit: secondUnit,
        provider: providerOne.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([providerOne])
      .rpc();

    // Append unit 2.
    await program.methods
      .registerRawUpload(
        created.metaId,
        "QmAppendTwo222222222222222222222222222222222222222",
        new anchor.BN(20_000),
        new anchor.BN(20_000 + 2 * 86_400)
      )
      .accountsStrict({
        registryState,
        dataEntryMeta: created.dataEntryMeta,
        uploadUnit: thirdUnit,
        provider: providerOne.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([providerOne])
      .rpc();

    // Fetch and validate aggregate counters plus per-unit indexes.
    const meta = await program.account.dataEntryMeta.fetch(created.dataEntryMeta);
    const unitOne = await program.account.uploadUnit.fetch(secondUnit);
    const unitTwo = await program.account.uploadUnit.fetch(thirdUnit);

    expect(meta.unitCount).to.equal(3);
    expect(meta.totalDuration.toNumber()).to.equal(4 * 86_400);
    expect(unitOne.unitIndex).to.equal(1);
    expect(unitTwo.unitIndex).to.equal(2);
    expect(unitOne.rawCid).to.contain("AppendOne");
    expect(unitTwo.rawCid).to.contain("AppendTwo");
  });

  it("lets the configured TEE authority attach feat CIDs to upload units", async () => {
    const created = await createMetaEntry(providerOne, registryState);

    // Only tee_authority can enrich a raw upload with feature CID.
    await program.methods
      .updateUploadUnit(
        created.metaId,
        0,
        "bafybeigdyrzt4processedfeaturecid0000000000000000000000"
      )
      .accountsStrict({
        registryState,
        uploadUnit: created.uploadUnit,
        teeAuthority: teeAuthority.publicKey,
      })
      .signers([teeAuthority])
      .rpc();

    // Persisted feature CID proves the on-chain update succeeded.
    const unit = await program.account.uploadUnit.fetch(created.uploadUnit);
    expect(unit.featCid).to.equal(
      "bafybeigdyrzt4processedfeaturecid0000000000000000000000"
    );
  });

  it("enforces owner and paused restrictions like the MVP flow expects", async () => {
    const created = await createMetaEntry(providerOne, registryState);
    const unauthorizedUnit = deriveUnitPda(created.metaId, 1);

    // Non-owner provider cannot append raw data to someone else's meta entry.
    await expectAnchorError(
      () =>
        program.methods
          .registerRawUpload(
            created.metaId,
            "QmWrongOwner11111111111111111111111111111111111111",
            new anchor.BN(1),
            new anchor.BN(2)
          )
          .accountsStrict({
            registryState,
            dataEntryMeta: created.dataEntryMeta,
            uploadUnit: unauthorizedUnit,
            provider: providerTwo.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([providerTwo])
          .rpc(),
      "NotOwner"
    );

    // Pause the protocol, then verify all mutating paths are blocked.
    await program.methods
      .setPaused(true)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();

    const registryDuringPause = await program.account.registryState.fetch(
      registryState
    );
    const pausedMetaId = registryDuringPause.nextMetaId as anchor.BN;

    // New meta creation should fail while paused.
    await expectAnchorError(
      () =>
        program.methods
          .uploadNewMeta({
            rawCid: "QmPausedCreate1111111111111111111111111111111111111",
            dataTypes: ["steps"],
            deviceType: device.deviceType,
            deviceModel: device.deviceModel,
            serviceProvider: device.serviceProvider,
            dayStartTimestamp: new anchor.BN(1_000),
            dayEndTimestamp: new anchor.BN(2_000),
            age: attributes.age,
            gender: attributes.gender,
            height: attributes.height,
            weight: attributes.weight,
            region: attributes.region,
            physicalActivityLevel: attributes.physicalActivityLevel,
            smoker: attributes.smoker,
            diet: attributes.diet,
            chronicConditions: chronicConditionsBuffer,
          })
          .accountsStrict({
            registryState,
            dataEntryMeta: deriveMetaPda(pausedMetaId),
            uploadUnit: deriveUnitPda(pausedMetaId, 0),
            provider: providerOne.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([providerOne])
          .rpc(),
      "Paused"
    );

    // Raw append should fail while paused.
    await expectAnchorError(
      () =>
        program.methods
          .registerRawUpload(
            created.metaId,
            "QmPausedAppend1111111111111111111111111111111111111",
            new anchor.BN(100),
            new anchor.BN(200)
          )
          .accountsStrict({
            registryState,
            dataEntryMeta: created.dataEntryMeta,
            uploadUnit: deriveUnitPda(created.metaId, 1),
            provider: providerOne.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([providerOne])
          .rpc(),
      "Paused"
    );

    // TEE update should fail while paused.
    await expectAnchorError(
      () =>
        program.methods
          .updateUploadUnit(
            created.metaId,
            0,
            "bafybeipausedfeaturecid00000000000000000000000000000"
          )
          .accountsStrict({
            registryState,
            uploadUnit: created.uploadUnit,
            teeAuthority: teeAuthority.publicKey,
          })
          .signers([teeAuthority])
          .rpc(),
      "Paused"
    );

    // Close operation should fail while paused.
    await expectAnchorError(
      () =>
        program.methods
          .closeDataEntryMeta(created.metaId)
          .accountsStrict({
            registryState,
            dataEntryMeta: created.dataEntryMeta,
            provider: providerOne.publicKey,
          })
          .signers([providerOne])
          .rpc(),
      "Paused"
    );

    // Unpause to avoid leaking state into later tests.
    await program.methods
      .setPaused(false)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  });

  it("closes the metadata account while leaving existing upload units fetchable", async () => {
    const created = await createMetaEntry(providerOne, registryState);

    // Wrong owner cannot close the meta account.
    await expectAnchorError(
      () =>
        program.methods
          .closeDataEntryMeta(created.metaId)
          .accountsStrict({
            registryState,
            dataEntryMeta: created.dataEntryMeta,
            provider: providerTwo.publicKey,
          })
          .signers([providerTwo])
          .rpc(),
      "NotOwner"
    );

    // Correct owner closes the meta account.
    await program.methods
      .closeDataEntryMeta(created.metaId)
      .accountsStrict({
        registryState,
        dataEntryMeta: created.dataEntryMeta,
        provider: providerOne.publicKey,
      })
      .signers([providerOne])
      .rpc();

    // Meta should be gone, but upload unit account remains readable.
    const closedMeta = await program.account.dataEntryMeta.fetchNullable(
      created.dataEntryMeta
    );
    const originalUnit = await program.account.uploadUnit.fetch(created.uploadUnit);

    expect(closedMeta).to.equal(null);
    expect(originalUnit.rawCid).to.equal(created.params.rawCid);
  });
});
