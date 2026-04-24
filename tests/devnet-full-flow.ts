import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import { expect } from "chai";

import { DataRegistry } from "../target/types/data_registry";
import { OrderHandler } from "../target/types/order_handler";

const workspaceWalletPath = path.resolve(__dirname, "../.anchor/wsl-id.json");

process.env.ANCHOR_PROVIDER_URL ??= "https://api.devnet.solana.com";
process.env.ANCHOR_WALLET ??=
  fs.existsSync(workspaceWalletPath)
    ? workspaceWalletPath
    : `${process.env.HOME ?? ""}/.config/solana/id.json`;

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const dataRegistry = anchor.workspace
  .DataRegistry as Program<DataRegistry>;
const orderHandler = anchor.workspace.OrderHandler as Program<OrderHandler>;

const shouldRun = process.env.RUN_DEVNET_INTEGRATION === "1";
const describeDevnet = shouldRun ? describe : describe.skip;

const registrySeed = Buffer.from("registry_state");
const metaSeed = Buffer.from("meta");
const unitSeed = Buffer.from("unit");
const orderConfigSeed = Buffer.from("order_config");
const jobSeed = Buffer.from("job");
const escrowSeed = Buffer.from("escrow");

type Authority = {
  publicKey: PublicKey;
  signers: Keypair[];
};

type FundingStats = {
  label: string;
  fundedLamports: number;
  reclaimedLamports: number;
};

function readKeypairFromEnv(envName: string): Keypair | null {
  const value = process.env[envName];
  if (!value) {
    return null;
  }

  const resolvedPath = value.replace(/^~/, process.env.HOME ?? "");
  let secretText: string;

  try {
    secretText = fs.readFileSync(resolvedPath, "utf8");
  } catch (error) {
    if (resolvedPath.startsWith("\\\\wsl.localhost\\")) {
      throw new Error(
        `Cannot read ${envName} from WSL UNC path ${resolvedPath}. Copy that keypair to a local Windows-accessible file under the workspace and point ${envName} at the local path instead.`
      );
    }

    throw error;
  }

  const secret = JSON.parse(secretText);
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

function metaIdBuffer(metaId: anchor.BN): Buffer {
  return metaId.toArrayLike(Buffer, "le", 8);
}

function unitIndexBuffer(unitIndex: number): Buffer {
  return new anchor.BN(unitIndex).toArrayLike(Buffer, "le", 4);
}

function jobIdBuffer(jobId: anchor.BN): Buffer {
  return jobId.toArrayLike(Buffer, "le", 8);
}

function deriveRegistryStatePda(): PublicKey {
  return PublicKey.findProgramAddressSync(
    [registrySeed],
    dataRegistry.programId
  )[0];
}

function deriveMetaPda(metaId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [metaSeed, metaIdBuffer(metaId)],
    dataRegistry.programId
  )[0];
}

function deriveUnitPda(metaId: anchor.BN, unitIndex: number): PublicKey {
  return PublicKey.findProgramAddressSync(
    [unitSeed, metaIdBuffer(metaId), unitIndexBuffer(unitIndex)],
    dataRegistry.programId
  )[0];
}

function deriveOrderConfigPda(): PublicKey {
  return PublicKey.findProgramAddressSync(
    [orderConfigSeed],
    orderHandler.programId
  )[0];
}

function deriveJobPda(jobId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [jobSeed, jobIdBuffer(jobId)],
    orderHandler.programId
  )[0];
}

function deriveEscrowVaultPda(jobId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [escrowSeed, jobIdBuffer(jobId)],
    orderHandler.programId
  )[0];
}

function signerFor(authority: Authority): Keypair[] {
  return authority.signers;
}

function logStep(message: string): void {
  // Prefix all logs so progress is easy to scan in noisy RPC output.
  console.log(`[devnet-flow] ${message}`);
}

function logWarn(message: string): void {
  console.warn(`[devnet-flow] WARN: ${message}`);
}

function isProviderSigner(publicKey: PublicKey): boolean {
  return publicKey.equals(provider.publicKey);
}

function assertDevnetEndpoint(): void {
  const endpoint = provider.connection.rpcEndpoint;
  expect(endpoint, "test must target devnet").to.contain("devnet");
}

function formatSol(lamports: number): string {
  return (lamports / anchor.web3.LAMPORTS_PER_SOL).toFixed(4);
}

function actorKey(publicKey: PublicKey): string {
  return publicKey.toBase58();
}

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

async function transferLamports(
  recipient: PublicKey,
  lamports: number
): Promise<void> {
  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: provider.publicKey,
      toPubkey: recipient,
      lamports,
    })
  );

  await provider.sendAndConfirm(tx, []);
}

async function fundActor(
  actor: Keypair,
  lamports: number,
  stats: FundingStats
): Promise<void> {
  const balance = await provider.connection.getBalance(actor.publicKey);
  if (balance >= lamports) {
    logStep(
      `funding skipped for ${stats.label}; balance ${formatSol(
        balance
      )} SOL already covers target ${formatSol(lamports)} SOL`
    );
    return;
  }

  const shortfall = lamports - balance;
  const payerBalance = await provider.connection.getBalance(provider.publicKey);
  if (payerBalance < shortfall) {
    throw new Error(
      `Provider wallet ${provider.publicKey.toBase58()} has ${formatSol(
        payerBalance
      )} SOL, but ${formatSol(
        shortfall
      )} SOL is needed to top up actor ${actor.publicKey.toBase58()} for this test step.`
    );
  }

  logStep(
    `funding ${stats.label} (${actor.publicKey.toBase58()}) with ${formatSol(
      shortfall
    )} SOL to reach ${formatSol(lamports)} SOL`
  );
  await transferLamports(actor.publicKey, shortfall);
  stats.fundedLamports += shortfall;
}

async function reclaimActorLamports(
  actor: Keypair,
  stats: FundingStats,
  reserveLamports = 10_000
): Promise<void> {
  const balance = await provider.connection.getBalance(actor.publicKey);
  if (balance <= reserveLamports) {
    logStep(
      `reclaim skipped for ${stats.label}; remaining balance ${formatSol(
        balance
      )} SOL is within reserve`
    );
    return;
  }

  const reclaimableLamports = balance - reserveLamports;
  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: actor.publicKey,
      toPubkey: provider.publicKey,
      lamports: reclaimableLamports,
    })
  );

  await provider.sendAndConfirm(tx, [actor]);
  stats.reclaimedLamports += reclaimableLamports;
  logStep(
    `reclaimed ${formatSol(reclaimableLamports)} SOL from ${stats.label} (${actor.publicKey.toBase58()})`
  );
}

async function tryReclaimActorLamports(
  actor: Keypair,
  stats: FundingStats,
  reserveLamports = 10_000
): Promise<void> {
  try {
    await reclaimActorLamports(actor, stats, reserveLamports);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? "unknown error");
    logWarn(
      `failed to reclaim SOL from ${stats.label} (${actor.publicKey.toBase58()}): ${message}`
    );
  }
}

function logFundingSummary(stats: FundingStats[]): void {
  let fundedLamports = 0;
  let reclaimedLamports = 0;

  for (const stat of stats) {
    const netLamports = stat.fundedLamports - stat.reclaimedLamports;
    fundedLamports += stat.fundedLamports;
    reclaimedLamports += stat.reclaimedLamports;
    logStep(
      `funding summary ${stat.label}: funded=${formatSol(
        stat.fundedLamports
      )} SOL reclaimed=${formatSol(stat.reclaimedLamports)} SOL net=${formatSol(
        netLamports
      )} SOL`
    );
  }

  logStep(
    `funding summary total: funded=${formatSol(
      fundedLamports
    )} SOL reclaimed=${formatSol(reclaimedLamports)} SOL net=${formatSol(
      fundedLamports - reclaimedLamports
    )} SOL`
  );
}

async function ensureRegistryReady(): Promise<{
  registryState: PublicKey;
  teeAuthority: Authority;
}> {
  const registryState = deriveRegistryStatePda();
  const configuredTee = readKeypairFromEnv("DEVNET_TEE_AUTHORITY_KEYPAIR");
  const existing = await dataRegistry.account.registryState.fetchNullable(
    registryState
  );

  if (!existing) {
    const tee = configuredTee ?? Keypair.generate();
    await dataRegistry.methods
      .initializeRegistry(tee.publicKey)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return {
      registryState,
      teeAuthority: { publicKey: tee.publicKey, signers: [tee] },
    };
  }

  const owner = new PublicKey(existing.owner);
  const currentTee = new PublicKey(existing.teeAuthority);

  async function ensureUnpaused(): Promise<void> {
    const latest = await dataRegistry.account.registryState.fetch(registryState);
    if (!latest.paused) {
      return;
    }

    if (!owner.equals(provider.publicKey)) {
      throw new Error(
        `Data registry is paused and owner is ${owner.toBase58()}. ` +
          "Run this test with the registry owner wallet or unpause the deployed registry first."
      );
    }

    await dataRegistry.methods
      .setPaused(false)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  }

  if (configuredTee) {
    if (!currentTee.equals(configuredTee.publicKey)) {
      if (!owner.equals(provider.publicKey)) {
        throw new Error(
          `Data registry owner is ${owner.toBase58()}, so this wallet cannot rotate tee_authority. ` +
            "Set DEVNET_TEE_AUTHORITY_KEYPAIR to the current deployed tee authority."
        );
      }

      await dataRegistry.methods
        .setTeeAuthority(configuredTee.publicKey)
        .accountsStrict({
          registryState,
          owner: provider.publicKey,
        })
        .rpc();
    }

    await ensureUnpaused();

    return {
      registryState,
      teeAuthority: {
        publicKey: configuredTee.publicKey,
        signers: [configuredTee],
      },
    };
  }

  if (isProviderSigner(currentTee)) {
    await ensureUnpaused();

    return {
      registryState,
      teeAuthority: { publicKey: provider.publicKey, signers: [] },
    };
  }

  if (!owner.equals(provider.publicKey)) {
    throw new Error(
      `Data registry tee_authority is ${currentTee.toBase58()} and owner is ${owner.toBase58()}. ` +
        "Set DEVNET_TEE_AUTHORITY_KEYPAIR to a keypair matching the deployed tee authority."
    );
  }

  const generatedTee = Keypair.generate();
  await dataRegistry.methods
    .setTeeAuthority(generatedTee.publicKey)
    .accountsStrict({
      registryState,
      owner: provider.publicKey,
    })
    .rpc();

  if (existing.paused) {
    await dataRegistry.methods
      .setPaused(false)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  }

  await ensureUnpaused();

  return {
    registryState,
    teeAuthority: { publicKey: generatedTee.publicKey, signers: [generatedTee] },
  };
}

async function ensureOrderConfigReady(): Promise<{
  orderConfig: PublicKey;
  roflAuthority: Authority;
}> {
  const orderConfig = deriveOrderConfigPda();
  const configuredRofl = readKeypairFromEnv("DEVNET_ROFL_AUTHORITY_KEYPAIR");
  const existing = await orderHandler.account.orderConfig.fetchNullable(
    orderConfig
  );

  if (!existing) {
    const rofl = configuredRofl ?? Keypair.generate();
    await orderHandler.methods
      .initializeOrderConfig(rofl.publicKey)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return {
      orderConfig,
      roflAuthority: { publicKey: rofl.publicKey, signers: [rofl] },
    };
  }

  const owner = new PublicKey(existing.owner);
  const currentRofl = new PublicKey(existing.roflAuthority);

  if (configuredRofl) {
    if (!currentRofl.equals(configuredRofl.publicKey)) {
      if (!owner.equals(provider.publicKey)) {
        throw new Error(
          `Order config owner is ${owner.toBase58()}, so this wallet cannot rotate rofl_authority. ` +
            "Set DEVNET_ROFL_AUTHORITY_KEYPAIR to the current deployed rofl authority."
        );
      }

      await orderHandler.methods
        .setRoflAuthority(configuredRofl.publicKey)
        .accountsStrict({
          orderConfig,
          owner: provider.publicKey,
        })
        .rpc();
    }

    return {
      orderConfig,
      roflAuthority: {
        publicKey: configuredRofl.publicKey,
        signers: [configuredRofl],
      },
    };
  }

  if (isProviderSigner(currentRofl)) {
    return {
      orderConfig,
      roflAuthority: { publicKey: provider.publicKey, signers: [] },
    };
  }

  if (!owner.equals(provider.publicKey)) {
    throw new Error(
      `Order rofl_authority is ${currentRofl.toBase58()} and owner is ${owner.toBase58()}. ` +
        "Set DEVNET_ROFL_AUTHORITY_KEYPAIR to a keypair matching the deployed rofl authority."
    );
  }

  const generatedRofl = Keypair.generate();
  await orderHandler.methods
    .setRoflAuthority(generatedRofl.publicKey)
    .accountsStrict({
      orderConfig,
      owner: provider.publicKey,
    })
    .rpc();

  return {
    orderConfig,
    roflAuthority: {
      publicKey: generatedRofl.publicKey,
      signers: [generatedRofl],
    },
  };
}

describeDevnet("devnet deployed contracts full flow", () => {
  const dataProvider = Keypair.generate();
  const researcher = Keypair.generate();
  const payoutProviderOne = Keypair.generate();
  const payoutProviderTwo = Keypair.generate();
  const dataProviderUploadBudgetLamports = 40_000_000;
  const dataProviderAppendBudgetLamports = 15_000_000;
  const researcherRequestBudgetLamports = 20_000_000;
  const researcherConfirmBudgetLamports = 155_000_000;
  const payoutClaimBudgetLamports = 2_000_000;

  let registryState: PublicKey;
  let teeAuthority: Authority;
  let orderConfig: PublicKey;
  let roflAuthority: Authority;
  const fundingStatsByActor = new Map<string, FundingStats>([
    [
      actorKey(dataProvider.publicKey),
      { label: "dataProvider", fundedLamports: 0, reclaimedLamports: 0 },
    ],
    [
      actorKey(researcher.publicKey),
      { label: "researcher", fundedLamports: 0, reclaimedLamports: 0 },
    ],
    [
      actorKey(payoutProviderOne.publicKey),
      {
        label: "payoutProviderOne",
        fundedLamports: 0,
        reclaimedLamports: 0,
      },
    ],
    [
      actorKey(payoutProviderTwo.publicKey),
      {
        label: "payoutProviderTwo",
        fundedLamports: 0,
        reclaimedLamports: 0,
      },
    ],
  ]);

  function fundingStatsFor(actor: Keypair): FundingStats {
    return fundingStatsByActor.get(actorKey(actor.publicKey))!;
  }

  before(async () => {
    logStep("before: validating devnet endpoint");
    assertDevnetEndpoint();
    logStep("before: ensuring data_registry state + tee authority");
    ({ registryState, teeAuthority } = await ensureRegistryReady());
    logStep(
      `before: registry ready (${registryState.toBase58()}), tee=${teeAuthority.publicKey.toBase58()}`
    );

    logStep("before: ensuring order_handler config + rofl authority");
    ({ orderConfig, roflAuthority } = await ensureOrderConfigReady());
    logStep(
      `before: order config ready (${orderConfig.toBase58()}), rofl=${roflAuthority.publicKey.toBase58()}`
    );
  });

  it("uploads provider data, enriches it, creates a paid job, finalizes it, and pays selected providers", async () => {
    const registryBefore = await dataRegistry.account.registryState.fetch(
      registryState
    );
    const metaId = registryBefore.nextMetaId as anchor.BN;
    const dataEntryMeta = deriveMetaPda(metaId);
    const initialUploadUnit = deriveUnitPda(metaId, 0);
    const appendedUploadUnit = deriveUnitPda(metaId, 1);
    const now = Math.floor(Date.now() / 1000);

    logStep(
      `it: starting flow metaId=${metaId.toString()} dataEntryMeta=${dataEntryMeta.toBase58()}`
    );

    await fundActor(
      dataProvider,
      dataProviderUploadBudgetLamports,
      fundingStatsFor(dataProvider)
    );
    await dataRegistry.methods
      .uploadNewMeta({
        rawCid: "QmDevnetRawInitial111111111111111111111111111111111",
        dataTypes: ["heart_rate", "sleep"],
        deviceType: "Smartwatch",
        deviceModel: "Devnet Flow Watch",
        serviceProvider: "HealthTrust Devnet",
        dayStartTimestamp: new anchor.BN(now),
        dayEndTimestamp: new anchor.BN(now + 86_400),
        age: 2,
        gender: 1,
        height: 175,
        weight: 70,
        region: 1,
        physicalActivityLevel: 2,
        smoker: 0,
        diet: 1,
        chronicConditions: Buffer.from([1, 4]),
      })
      .accountsStrict({
        registryState,
        dataEntryMeta,
        uploadUnit: initialUploadUnit,
        provider: dataProvider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([dataProvider])
      .rpc();
    logStep("it: uploadNewMeta succeeded");

    await fundActor(
      dataProvider,
      dataProviderAppendBudgetLamports,
      fundingStatsFor(dataProvider)
    );
    await dataRegistry.methods
      .registerRawUpload(
        metaId,
        "QmDevnetRawAppend2222222222222222222222222222222222",
        new anchor.BN(now + 86_400),
        new anchor.BN(now + 172_800)
      )
      .accountsStrict({
        registryState,
        dataEntryMeta,
        uploadUnit: appendedUploadUnit,
        provider: dataProvider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([dataProvider])
      .rpc();
    logStep("it: registerRawUpload succeeded");

    await dataRegistry.methods
      .updateUploadUnit(
        metaId,
        0,
        "bafybeidevnetfeaturecid111111111111111111111111111111"
      )
      .accountsStrict({
        registryState,
        uploadUnit: initialUploadUnit,
        teeAuthority: teeAuthority.publicKey,
      })
      .signers(signerFor(teeAuthority))
      .rpc();
    logStep("it: updateUploadUnit succeeded");

    const meta = await dataRegistry.account.dataEntryMeta.fetch(dataEntryMeta);
    const enrichedUnit = await dataRegistry.account.uploadUnit.fetch(
      initialUploadUnit
    );
    expect(meta.unitCount).to.equal(2);
    expect(enrichedUnit.featCid).to.equal(
      "bafybeidevnetfeaturecid111111111111111111111111111111"
    );
    logStep("it: meta and upload unit assertions passed");

    const configBefore = await orderHandler.account.orderConfig.fetch(
      orderConfig
    );
    const jobId = configBefore.nextJobId as anchor.BN;
    const job = deriveJobPda(jobId);
    const escrowVault = deriveEscrowVaultPda(jobId);
    logStep(`it: derived jobId=${jobId.toString()} job=${job.toBase58()}`);

    await fundActor(
      researcher,
      researcherRequestBudgetLamports,
      fundingStatsFor(researcher)
    );
    await orderHandler.methods
      .requestJob({
        templateId: 1,
        numDays: 2,
        dataTypes: ["heart_rate", "sleep"],
        maxParticipants: 2,
        startDayUtc: new anchor.BN(0),
        filterQuery: `devnet_meta_id = ${metaId.toString()}`,
      })
      .accountsStrict({
        orderConfig,
        job,
        researcher: researcher.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([researcher])
      .rpc();
    logStep("it: requestJob succeeded");

    await orderHandler.methods
      .submitPreflightResult(jobId, {
        effectiveParticipantsScaled: new anchor.BN(2),
        qualityTier: 1,
        finalTotal: new anchor.BN(120_000_000),
        cohortHash: Array.from(
          createHash("sha256").update(dataEntryMeta.toBuffer()).digest()
        ),
        selectedParticipants: [
          payoutProviderOne.publicKey,
          payoutProviderTwo.publicKey,
        ],
      })
      .accountsStrict({
        orderConfig,
        job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();
    logStep("it: submitPreflightResult succeeded");

    await fundActor(
      researcher,
      researcherConfirmBudgetLamports,
      fundingStatsFor(researcher)
    );
    await orderHandler.methods
      .confirmJobAndPay(jobId, new anchor.BN(150_000_000))
      .accountsStrict({
        job,
        escrowVault,
        researcher: researcher.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([researcher])
      .rpc();
    logStep("it: confirmJobAndPay succeeded");

    await orderHandler.methods
      .submitResult(
        jobId,
        "bafybeidevnetresultcid333333333333333333333333333333",
        Array.from(createHash("sha256").update("devnet-output").digest())
      )
      .accountsStrict({
        orderConfig,
        job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();
    logStep("it: submitResult succeeded");

    await orderHandler.methods
      .finalizeJob(jobId)
      .accountsStrict({
        orderConfig,
        job,
        escrowVault,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();
    logStep("it: finalizeJob succeeded");

    const providerOneBefore = await provider.connection.getBalance(
      payoutProviderOne.publicKey
    );
    const providerTwoBefore = await provider.connection.getBalance(
      payoutProviderTwo.publicKey
    );

    await fundActor(
      payoutProviderOne,
      payoutClaimBudgetLamports,
      fundingStatsFor(payoutProviderOne)
    );
    await orderHandler.methods
      .claimPayout(jobId)
      .accountsStrict({
        job,
        escrowVault,
        provider: payoutProviderOne.publicKey,
      })
      .signers([payoutProviderOne])
      .rpc();
    logStep("it: claimPayout for provider one succeeded");

    await fundActor(
      payoutProviderTwo,
      payoutClaimBudgetLamports,
      fundingStatsFor(payoutProviderTwo)
    );
    await orderHandler.methods
      .claimPayout(jobId)
      .accountsStrict({
        job,
        escrowVault,
        provider: payoutProviderTwo.publicKey,
      })
      .signers([payoutProviderTwo])
      .rpc();
    logStep("it: claimPayout for provider two succeeded");

    const completedJob = await orderHandler.account.job.fetch(job);
    const providerOneAfter = await provider.connection.getBalance(
      payoutProviderOne.publicKey
    );
    const providerTwoAfter = await provider.connection.getBalance(
      payoutProviderTwo.publicKey
    );

    expect(completedJob.status).to.deep.equal({ completed: {} });
    expect(completedJob.claimedBitmap.toNumber()).to.equal(3);
    expect(completedJob.resultCid).to.equal(
      "bafybeidevnetresultcid333333333333333333333333333333"
    );
    expect(providerOneAfter).to.be.greaterThan(providerOneBefore);
    expect(providerTwoAfter).to.be.greaterThan(providerTwoBefore);
    logStep(
      `it: provider1: ${formatSol(providerOneBefore)} SOL -> ${formatSol(
        providerOneAfter
      )} SOL, provider2: ${formatSol(providerTwoBefore)} SOL -> ${formatSol(
        providerTwoAfter
      )} SOL`
    );
    logStep("it: final assertions passed, full flow complete");

    await tryReclaimActorLamports(dataProvider, fundingStatsFor(dataProvider));
    await tryReclaimActorLamports(researcher, fundingStatsFor(researcher));
    await tryReclaimActorLamports(
      payoutProviderOne,
      fundingStatsFor(payoutProviderOne)
    );
    await tryReclaimActorLamports(
      payoutProviderTwo,
      fundingStatsFor(payoutProviderTwo)
    );
    logFundingSummary([...fundingStatsByActor.values()]);
  });

  it("enforces payout and workflow guardrails on devnet", async () => {
    const guardrailResearcher = Keypair.generate();
    const selectedProvider = Keypair.generate();
    const outsiderProvider = Keypair.generate();
    const rogueRofl = Keypair.generate();

    const guardrailResearcherStats: FundingStats = {
      label: "guardrailResearcher",
      fundedLamports: 0,
      reclaimedLamports: 0,
    };
    const selectedProviderStats: FundingStats = {
      label: "selectedProvider",
      fundedLamports: 0,
      reclaimedLamports: 0,
    };
    const outsiderProviderStats: FundingStats = {
      label: "outsiderProvider",
      fundedLamports: 0,
      reclaimedLamports: 0,
    };
    const rogueRoflStats: FundingStats = {
      label: "rogueRofl",
      fundedLamports: 0,
      reclaimedLamports: 0,
    };

    await fundActor(
      guardrailResearcher,
      20_000_000,
      guardrailResearcherStats
    );
    await fundActor(selectedProvider, 2_000_000, selectedProviderStats);
    await fundActor(outsiderProvider, 2_000_000, outsiderProviderStats);
    await fundActor(rogueRofl, 2_000_000, rogueRoflStats);

    const configBefore = await orderHandler.account.orderConfig.fetch(orderConfig);
    const jobId = configBefore.nextJobId as anchor.BN;
    const job = deriveJobPda(jobId);
    const escrowVault = deriveEscrowVaultPda(jobId);

    logStep(`guardrails: derived jobId=${jobId.toString()} job=${job.toBase58()}`);

    await orderHandler.methods
      .requestJob({
        templateId: 1,
        numDays: 1,
        dataTypes: ["heart_rate"],
        maxParticipants: 1,
        startDayUtc: new anchor.BN(0),
        filterQuery: "guardrail_devnet = true",
      })
      .accountsStrict({
        orderConfig,
        job,
        researcher: guardrailResearcher.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([guardrailResearcher])
      .rpc();
    logStep("guardrails: requestJob succeeded");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .submitPreflightResult(jobId, {
            effectiveParticipantsScaled: new anchor.BN(1),
            qualityTier: 1,
            finalTotal: new anchor.BN(100_000_000),
            cohortHash: Array.from(
              createHash("sha256").update("rogue-preflight").digest()
            ),
            selectedParticipants: [selectedProvider.publicKey],
          })
          .accountsStrict({
            orderConfig,
            job,
            roflAuthority: rogueRofl.publicKey,
          })
          .signers([rogueRofl])
          .rpc(),
      "NotRoflAuthority"
    );
    logStep("guardrails: rejected rogue rofl preflight as expected");

    await orderHandler.methods
      .submitPreflightResult(jobId, {
        effectiveParticipantsScaled: new anchor.BN(1),
        qualityTier: 1,
        finalTotal: new anchor.BN(100_000_000),
        cohortHash: Array.from(
          createHash("sha256").update("guardrail-preflight").digest()
        ),
        selectedParticipants: [selectedProvider.publicKey],
      })
      .accountsStrict({
        orderConfig,
        job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();
    logStep("guardrails: valid preflight succeeded");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .submitResult(
            jobId,
            "bafybeidevnetguardrailtoosoon555555555555555555555555",
            Array.from(createHash("sha256").update("too-soon").digest())
          )
          .accountsStrict({
            orderConfig,
            job,
            roflAuthority: roflAuthority.publicKey,
          })
          .signers(signerFor(roflAuthority))
          .rpc(),
      "InvalidStatus"
    );
    logStep("guardrails: rejected submitResult before payment as expected");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .confirmJobAndPay(jobId, new anchor.BN(1_000_000))
          .accountsStrict({
            job,
            escrowVault,
            researcher: guardrailResearcher.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([guardrailResearcher])
          .rpc(),
      "InsufficientPayment"
    );
    logStep("guardrails: rejected insufficient payment as expected");

    await fundActor(
      guardrailResearcher,
      105_000_000,
      guardrailResearcherStats
    );
    await orderHandler.methods
      .confirmJobAndPay(jobId, new anchor.BN(100_000_000))
      .accountsStrict({
        job,
        escrowVault,
        researcher: guardrailResearcher.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([guardrailResearcher])
      .rpc();
    logStep("guardrails: valid confirmJobAndPay succeeded");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .claimPayout(jobId)
          .accountsStrict({
            job,
            escrowVault,
            provider: selectedProvider.publicKey,
          })
          .signers([selectedProvider])
          .rpc(),
      "InvalidStatus"
    );
    logStep("guardrails: rejected payout claim before completion as expected");

    await orderHandler.methods
      .submitResult(
        jobId,
        "bafybeidevnetguardrailresult444444444444444444444444",
        Array.from(createHash("sha256").update("guardrail-output").digest())
      )
      .accountsStrict({
        orderConfig,
        job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();

    await orderHandler.methods
      .finalizeJob(jobId)
      .accountsStrict({
        orderConfig,
        job,
        escrowVault,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers(signerFor(roflAuthority))
      .rpc();
    logStep("guardrails: finalized guardrail job");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .sweepVaultDust(jobId)
          .accountsStrict({
            job,
            escrowVault,
            recipient: outsiderProvider.publicKey,
          })
          .rpc(),
      "SweepNotAllowed"
    );
    logStep("guardrails: rejected outsider sweep before all claims as expected");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .claimPayout(jobId)
          .accountsStrict({
            job,
            escrowVault,
            provider: outsiderProvider.publicKey,
          })
          .signers([outsiderProvider])
          .rpc(),
      "NotAParticipant"
    );
    logStep("guardrails: rejected outsider claim as expected");

    await orderHandler.methods
      .claimPayout(jobId)
      .accountsStrict({
        job,
        escrowVault,
        provider: selectedProvider.publicKey,
      })
      .signers([selectedProvider])
      .rpc();
    logStep("guardrails: selected provider claim succeeded");

    await expectAnchorError(
      () =>
        orderHandler.methods
          .claimPayout(jobId)
          .accountsStrict({
            job,
            escrowVault,
            provider: selectedProvider.publicKey,
          })
          .signers([selectedProvider])
          .rpc(),
      "AlreadyClaimed"
    );
    logStep("guardrails: rejected double-claim as expected");

    await tryReclaimActorLamports(guardrailResearcher, guardrailResearcherStats);
    await tryReclaimActorLamports(selectedProvider, selectedProviderStats);
    await tryReclaimActorLamports(outsiderProvider, outsiderProviderStats);
    await tryReclaimActorLamports(rogueRofl, rogueRoflStats);
    logFundingSummary([
      guardrailResearcherStats,
      selectedProviderStats,
      outsiderProviderStats,
      rogueRoflStats,
    ]);
  });
});
