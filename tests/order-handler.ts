import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import { expect } from "chai";

import { OrderHandler } from "../target/types/order_handler";

const workspaceWalletPath = path.resolve(__dirname, "../.anchor/wsl-id.json");

process.env.ANCHOR_PROVIDER_URL ??= "http://127.0.0.1:8899";
process.env.ANCHOR_WALLET ??=
  fs.existsSync(workspaceWalletPath)
    ? workspaceWalletPath
    : `${process.env.HOME ?? ""}/.config/solana/id.json`;

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.OrderHandler as Program<OrderHandler>;

const orderConfigSeed = Buffer.from("order_config");
const jobSeed = Buffer.from("job");
const escrowSeed = Buffer.from("escrow");

function hashDataType(value: string): number[] {
  return [...createHash("sha256").update(value).digest()];
}

function jobIdBuffer(jobId: anchor.BN): Buffer {
  return jobId.toArrayLike(Buffer, "le", 8);
}

function deriveOrderConfigPda(): PublicKey {
  return PublicKey.findProgramAddressSync([orderConfigSeed], program.programId)[0];
}

function deriveJobPda(jobId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [jobSeed, jobIdBuffer(jobId)],
    program.programId
  )[0];
}

function deriveEscrowVaultPda(jobId: anchor.BN): PublicKey {
  return PublicKey.findProgramAddressSync(
    [escrowSeed, jobIdBuffer(jobId)],
    program.programId
  )[0];
}

async function fundKeypair(keypair: anchor.web3.Keypair) {
  const signature = await provider.connection.requestAirdrop(
    keypair.publicKey,
    3 * anchor.web3.LAMPORTS_PER_SOL
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

async function ensureOrderConfigInitialized(
  roflAuthority: PublicKey
): Promise<PublicKey> {
  const orderConfig = deriveOrderConfigPda();
  const existing = await program.account.orderConfig.fetchNullable(orderConfig);

  if (!existing) {
    await program.methods
      .initializeOrderConfig(roflAuthority)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return orderConfig;
  }

  const currentRofl = new PublicKey(existing.roflAuthority);
  if (!currentRofl.equals(roflAuthority)) {
    await program.methods
      .setRoflAuthority(roflAuthority)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
      })
      .rpc();
  }

  return orderConfig;
}

async function createRequestedJob(
  researcher: anchor.web3.Keypair,
  orderConfig: PublicKey
) {
  const configBefore = await program.account.orderConfig.fetch(orderConfig);
  const jobId = configBefore.nextJobId as anchor.BN;
  const job = deriveJobPda(jobId);

  const params = {
    templateId: 1,
    numDays: 1,
    dataTypeHashes: [hashDataType("heart_rate"), hashDataType("sleep")],
    maxParticipants: 10,
    startDayUtc: new anchor.BN(0),
    filterQuery: "age BETWEEN 20 AND 40",
  };

  await program.methods
    .requestJob(params)
    .accountsStrict({
      orderConfig,
      job,
      researcher: researcher.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers([researcher])
    .rpc();

  return {
    jobId,
    job,
    params,
  };
}

describe("order_handler migration parity", () => {
  const roflAuthority = anchor.web3.Keypair.generate();
  const researcher = anchor.web3.Keypair.generate();
  const providerOne = anchor.web3.Keypair.generate();
  const providerTwo = anchor.web3.Keypair.generate();
  const outsider = anchor.web3.Keypair.generate();

  let orderConfig: PublicKey;

  before(async () => {
    await Promise.all([
      fundKeypair(roflAuthority),
      fundKeypair(researcher),
      fundKeypair(providerOne),
      fundKeypair(providerTwo),
      fundKeypair(outsider),
    ]);
    orderConfig = await ensureOrderConfigInitialized(roflAuthority.publicKey);
  });

  beforeEach(async () => {
    await ensureOrderConfigInitialized(roflAuthority.publicKey);
  });

  it("lets a researcher create, confirm, execute, finalize, and pay out a job", async () => {
    const created = await createRequestedJob(researcher, orderConfig);
    const escrowVault = deriveEscrowVaultPda(created.jobId);

    const requestedJob = await program.account.job.fetch(created.job);
    expect(new PublicKey(requestedJob.researcher).equals(researcher.publicKey)).to
      .equal(true);
    expect(requestedJob.status).to.deep.equal({ pendingPreflight: {} });

    const preflight = {
      effectiveParticipantsScaled: new anchor.BN(10),
      qualityTier: 1,
      finalTotal: new anchor.BN(100_000_000),
      cohortHash: Array.from(createHash("sha256").update("cohort").digest()),
      selectedParticipants: [providerOne.publicKey, providerTwo.publicKey],
    };

    await program.methods
      .submitPreflightResult(created.jobId, preflight)
      .accountsStrict({
        orderConfig,
        job: created.job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers([roflAuthority])
      .rpc();

    const paymentAmount = new anchor.BN(200_000_000);
    await program.methods
      .confirmJobAndPay(created.jobId, paymentAmount)
      .accountsStrict({
        job: created.job,
        escrowVault,
        researcher: researcher.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([researcher])
      .rpc();

    await program.methods
      .submitResult(
        created.jobId,
        "bafybeigdyrzt4finalresultcid0000000000000000000000000",
        Array.from(createHash("sha256").update("attestation").digest())
      )
      .accountsStrict({
        orderConfig,
        job: created.job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers([roflAuthority])
      .rpc();

    await program.methods
      .finalizeJob(created.jobId)
      .accountsStrict({
        orderConfig,
        job: created.job,
        escrowVault,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers([roflAuthority])
      .rpc();

    const completedJob = await program.account.job.fetch(created.job);
    expect(completedJob.status).to.deep.equal({ completed: {} });
    expect(completedJob.amountPerProvider.toNumber()).to.be.greaterThan(0);

    const providerOneBefore = await provider.connection.getBalance(
      providerOne.publicKey
    );
    const providerTwoBefore = await provider.connection.getBalance(
      providerTwo.publicKey
    );

    await program.methods
      .claimPayout(created.jobId)
      .accountsStrict({
        job: created.job,
        escrowVault,
        provider: providerOne.publicKey,
      })
      .signers([providerOne])
      .rpc();

    await program.methods
      .claimPayout(created.jobId)
      .accountsStrict({
        job: created.job,
        escrowVault,
        provider: providerTwo.publicKey,
      })
      .signers([providerTwo])
      .rpc();

    const providerOneAfter = await provider.connection.getBalance(
      providerOne.publicKey
    );
    const providerTwoAfter = await provider.connection.getBalance(
      providerTwo.publicKey
    );

    expect(providerOneAfter).to.be.greaterThan(providerOneBefore);
    expect(providerTwoAfter).to.be.greaterThan(providerTwoBefore);

    const afterClaims = await program.account.job.fetch(created.job);
    expect(afterClaims.claimedBitmap.toNumber()).to.equal(3);

    const vaultBeforeSweep = await provider.connection.getBalance(escrowVault);
    await program.methods
      .sweepVaultDust(created.jobId)
      .accountsStrict({
        job: created.job,
        escrowVault,
        recipient: researcher.publicKey,
      })
      .rpc();
    const vaultAfterSweep = await provider.connection.getBalance(escrowVault);

    expect(vaultAfterSweep).to.be.lessThanOrEqual(vaultBeforeSweep);
    expect(
      (await program.account.job.fetch(created.job)).resultCid
    ).to.equal("bafybeigdyrzt4finalresultcid0000000000000000000000000");
  });

  it("enforces workflow access control and payment checks", async () => {
    const created = await createRequestedJob(researcher, orderConfig);
    const escrowVault = deriveEscrowVaultPda(created.jobId);

    await expectAnchorError(
      () =>
        program.methods
          .submitPreflightResult(created.jobId, {
            effectiveParticipantsScaled: new anchor.BN(5),
            qualityTier: 1,
            finalTotal: new anchor.BN(10_000_000),
            cohortHash: Array.from(createHash("sha256").update("bad").digest()),
            selectedParticipants: [providerOne.publicKey],
          })
          .accountsStrict({
            orderConfig,
            job: created.job,
            roflAuthority: outsider.publicKey,
          })
          .signers([outsider])
          .rpc(),
      "NotRoflAuthority"
    );

    await program.methods
      .submitPreflightResult(created.jobId, {
        effectiveParticipantsScaled: new anchor.BN(5),
        qualityTier: 1,
        finalTotal: new anchor.BN(10_000_000),
        cohortHash: Array.from(createHash("sha256").update("good").digest()),
        selectedParticipants: [providerOne.publicKey],
      })
      .accountsStrict({
        orderConfig,
        job: created.job,
        roflAuthority: roflAuthority.publicKey,
      })
      .signers([roflAuthority])
      .rpc();

    await expectAnchorError(
      () =>
        program.methods
          .confirmJobAndPay(created.jobId, new anchor.BN(1_000_000))
          .accountsStrict({
            job: created.job,
            escrowVault,
            researcher: researcher.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .signers([researcher])
          .rpc(),
      "InsufficientPayment"
    );

    await expectAnchorError(
      () =>
        program.methods
          .claimPayout(created.jobId)
          .accountsStrict({
            job: created.job,
            escrowVault,
            provider: outsider.publicKey,
          })
          .signers([outsider])
          .rpc(),
      "AccountNotInitialized"
    );
  });

  it("allows the owner to rotate the rofl authority", async () => {
    const replacementRofl = anchor.web3.Keypair.generate();
    await fundKeypair(replacementRofl);

    await program.methods
      .setRoflAuthority(replacementRofl.publicKey)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
      })
      .rpc();

    const configAfter = await program.account.orderConfig.fetch(orderConfig);
    expect(new PublicKey(configAfter.roflAuthority).equals(replacementRofl.publicKey))
      .to.equal(true);

    await program.methods
      .setRoflAuthority(roflAuthority.publicKey)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
      })
      .rpc();
  });
});
