const anchor = require("@coral-xyz/anchor");
const { createHash } = require("crypto");
const fs = require("fs");
const path = require("path");

const { Keypair, PublicKey, SystemProgram, Transaction } = anchor.web3;
const idl = require("../target/idl/order_handler.json");

const ORDER_CONFIG_SEED = Buffer.from("order_config");
const JOB_SEED = Buffer.from("job");
const ESCROW_SEED = Buffer.from("escrow");
const DEFAULT_RPC_URL = "http://127.0.0.1:8899";
const DEFAULT_ACTOR_TARGET_LAMPORTS = 50_000_000;

function fail(message) {
  console.error(`[order-handler-cli] ERROR: ${message}`);
  process.exit(1);
}

function log(message) {
  console.log(`[order-handler-cli] ${message}`);
}

function expandHome(inputPath) {
  if (!inputPath) {
    return inputPath;
  }

  if (typeof inputPath !== "string") {
    fail(`expected a path string but received ${typeof inputPath}`);
  }

  if (inputPath.startsWith("~/")) {
    return path.join(process.env.HOME || process.env.USERPROFILE || "", inputPath.slice(2));
  }

  return inputPath;
}

function resolveInputPath(inputPath) {
  return path.resolve(process.cwd(), expandHome(inputPath));
}

function readJsonFile(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function readKeypairFromFile(filePath) {
  const secret = readJsonFile(resolveInputPath(filePath));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

function createProgram(provider) {
  return new anchor.Program(idl, provider);
}

function bn64(value) {
  return new anchor.BN(value);
}

function bnToString(value) {
  return value && typeof value.toString === "function" ? value.toString() : value;
}

function jobIdBuffer(jobId) {
  return new anchor.BN(jobId).toArrayLike(Buffer, "le", 8);
}

function deriveOrderConfigPda(programId) {
  return PublicKey.findProgramAddressSync([ORDER_CONFIG_SEED], programId)[0];
}

function deriveJobPda(programId, jobId) {
  return PublicKey.findProgramAddressSync([JOB_SEED, jobIdBuffer(jobId)], programId)[0];
}

function deriveEscrowVaultPda(programId, jobId) {
  return PublicKey.findProgramAddressSync([ESCROW_SEED, jobIdBuffer(jobId)], programId)[0];
}

function parseValue(rawValue) {
  if (rawValue === undefined) {
    return true;
  }

  if (rawValue === "true") {
    return true;
  }

  if (rawValue === "false") {
    return false;
  }

  if (/^-?\d+$/.test(rawValue)) {
    return Number(rawValue);
  }

  return rawValue;
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const options = {};
  const booleanOptions = new Set(["help", "initialize-if-needed"]);

  for (let index = 0; index < rest.length; index += 1) {
    const token = rest[index];
    if (!token.startsWith("--")) {
      fail(`Unexpected argument: ${token}`);
    }

    const key = token.slice(2);
    const next = rest[index + 1];
    if (!next || next.startsWith("--")) {
      if (booleanOptions.has(key)) {
        options[key] = true;
        continue;
      }

      fail(`--${key} requires a value`);
    }

    options[key] = parseValue(next);
    index += 1;
  }

  return { command, options };
}

function printUsage() {
  console.log(
    [
      "Usage:",
      "  node ./scripts/order-handler-cli.js <command> [options]",
      "",
      "Commands:",
      "  read                  Read order config and one or more jobs",
      "  request-job           Create a new job request",
      "  submit-preflight      Submit ROFL preflight result",
      "  confirm-job           Fund escrow and confirm a job",
      "  submit-result         Submit result CID + output hash",
      "  finalize-job          Finalize an executed job",
      "  claim-payout          Claim a provider payout",
      "  sweep-dust            Sweep remaining lamports from escrow",
      "  cancel-job            Cancel a job as the researcher",
      "  fund-actor            Transfer SOL from main wallet to an actor wallet",
      "  set-rofl-authority    Rotate the ROFL authority",
      "",
      "Global options:",
      "  --url <rpc-url>",
      "  --wallet <keypair>",
      "  --rofl-keypair <keypair>",
      "",
      "Examples:",
      "  node ./scripts/order-handler-cli.js read --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --job-from 1 --job-to 3",
      "  node ./scripts/order-handler-cli.js request-job --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --researcher-keypair ./.anchor/researcher.json --template-id 1 --num-days 2 --data-types heart_rate,sleep --max-participants 3 --start-day-utc 0 --filter-query \"region = 1 AND smoker = 0\"",
      "  node ./scripts/order-handler-cli.js submit-preflight --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --rofl-keypair \\\\wsl.localhost\\Ubuntu\\home\\giorgos\\.config\\solana\\devnet-rofl.json --job-id 12 --effective-participants-scaled 3 --quality-tier 1 --final-total 120000000 --cohort-hash-text cohort-12 --selected-providers E5z72q...,AUVm7e...",
      "  node ./scripts/order-handler-cli.js confirm-job --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --researcher-keypair ./.anchor/researcher.json --job-id 12 --payment-amount 150000000",
    ].join("\n")
  );
}

function requireOption(options, key) {
  if (options[key] === undefined || options[key] === null || options[key] === "") {
    fail(`--${key} is required`);
  }

  return options[key];
}

function parseCsvStrings(value) {
  if (value === undefined || value === null || value === "") {
    return [];
  }

  return String(value)
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseCsvPublicKeys(value) {
  return parseCsvStrings(value).map((item) => new PublicKey(item));
}

function lamportsTarget(options) {
  if (Number.isFinite(options["actor-min-lamports"])) {
    return Math.floor(options["actor-min-lamports"]);
  }

  return DEFAULT_ACTOR_TARGET_LAMPORTS;
}

async function transferLamports(provider, recipient, lamports) {
  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: provider.publicKey,
      toPubkey: recipient,
      lamports,
    })
  );

  await provider.sendAndConfirm(tx, []);
}

async function fundActorIfNeeded(provider, actorPublicKey, targetLamports) {
  if (actorPublicKey.equals(provider.publicKey)) {
    return;
  }

  const currentBalance = await provider.connection.getBalance(actorPublicKey);
  if (currentBalance >= targetLamports) {
    log(
      `Funding skipped for actor ${actorPublicKey.toBase58()}; balance ${currentBalance} already meets target ${targetLamports}`
    );
    return;
  }

  const shortfall = targetLamports - currentBalance;
  const payerBalance = await provider.connection.getBalance(provider.publicKey);
  if (payerBalance < shortfall) {
    fail(
      `Main wallet ${provider.publicKey.toBase58()} has ${payerBalance} lamports, but ${shortfall} lamports are needed to fund actor ${actorPublicKey.toBase58()}.`
    );
  }

  log(
    `Funding actor ${actorPublicKey.toBase58()} with ${shortfall} lamports from main wallet ${provider.publicKey.toBase58()}`
  );
  await transferLamports(provider, actorPublicKey, shortfall);
}

async function ensureOrderConfig(program, provider, options) {
  const orderConfig = deriveOrderConfigPda(program.programId);
  const existing = await program.account.orderConfig.fetchNullable(orderConfig);
  if (!existing) {
    if (!options["initialize-if-needed"]) {
      fail(`Order config ${orderConfig.toBase58()} does not exist. Pass --initialize-if-needed to create it.`);
    }

    const roflAuthority = options["rofl-keypair"]
      ? readKeypairFromFile(options["rofl-keypair"]).publicKey
      : options["rofl-authority"]
        ? new PublicKey(options["rofl-authority"])
        : provider.publicKey;

    await program.methods
      .initializeOrderConfig(roflAuthority)
      .accountsStrict({
        orderConfig,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    return {
      address: orderConfig,
      account: await program.account.orderConfig.fetch(orderConfig),
    };
  }

  return { address: orderConfig, account: existing };
}

async function resolveRoflSigner(provider, roflAuthority, roflKeypairPath) {
  if (roflAuthority.equals(provider.publicKey)) {
    return {
      publicKey: provider.publicKey,
      signers: [],
    };
  }

  if (!roflKeypairPath) {
    return null;
  }

  const roflKeypair = readKeypairFromFile(roflKeypairPath);
  if (!roflKeypair.publicKey.equals(roflAuthority)) {
    fail(
      `Provided ROFL keypair ${roflKeypair.publicKey.toBase58()} does not match order config ROFL authority ${roflAuthority.toBase58()}.`
    );
  }

  return {
    publicKey: roflKeypair.publicKey,
    signers: [roflKeypair],
  };
}

async function loadActorSigner(provider, options, keyName) {
  const keypairPath = options[keyName];
  if (!keypairPath) {
    return { publicKey: provider.publicKey, signers: [] };
  }

  const signer = readKeypairFromFile(keypairPath);
  await fundActorIfNeeded(provider, signer.publicKey, lamportsTarget(options));
  return { publicKey: signer.publicKey, signers: [signer] };
}

function statusName(status) {
  if (!status || typeof status !== "object") {
    return String(status);
  }

  const [key] = Object.keys(status);
  return key || "unknown";
}

async function readJob(program, provider, jobId) {
  const jobAddress = deriveJobPda(program.programId, jobId);
  const escrowVault = deriveEscrowVaultPda(program.programId, jobId);
  const job = await program.account.job.fetchNullable(jobAddress);
  if (!job) {
    return null;
  }

  const escrowBalance = await provider.connection.getBalance(escrowVault);
  return {
    jobId: String(jobId),
    address: jobAddress.toBase58(),
    escrowVault: escrowVault.toBase58(),
    escrowVaultLamports: escrowBalance,
    researcher: new PublicKey(job.researcher).toBase58(),
    status: statusName(job.status),
    templateId: job.templateId,
    numDays: job.numDays,
    dataTypes: job.dataTypes,
    maxParticipants: job.maxParticipants,
    startDayUtc: bnToString(job.startDayUtc),
    filterQuery: job.filterQuery,
    escrowed: bnToString(job.escrowed),
    effectiveParticipantsScaled: bnToString(job.effectiveParticipantsScaled),
    qualityTier: job.qualityTier,
    finalTotal: bnToString(job.finalTotal),
    preflightTimestamp: bnToString(job.preflightTimestamp),
    cohortHashHex: Buffer.from(job.cohortHash).toString("hex"),
    selectedParticipants: job.selectedParticipants.map((item) => new PublicKey(item).toBase58()),
    resultCid: job.resultCid,
    executionTimestamp: bnToString(job.executionTimestamp),
    outputHashHex: Buffer.from(job.outputHash).toString("hex"),
    amountPerProvider: bnToString(job.amountPerProvider),
    claimedBitmap: bnToString(job.claimedBitmap),
    createdAt: bnToString(job.createdAt),
    updatedAt: bnToString(job.updatedAt),
  };
}

function normalizeJobIds(options) {
  if (options["job-ids"]) {
    return parseCsvStrings(options["job-ids"]).map((item) => Number(item)).filter(Number.isFinite);
  }

  if (Number.isFinite(options["job-from"]) && Number.isFinite(options["job-to"])) {
    const ids = [];
    for (let value = options["job-from"]; value <= options["job-to"]; value += 1) {
      ids.push(value);
    }
    return ids;
  }

  if (Number.isFinite(options["job-id"])) {
    return [options["job-id"]];
  }

  return [];
}

async function commandRead(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const jobIds = normalizeJobIds(options);
  const output = {
    rpcUrl: provider.connection.rpcEndpoint,
    wallet: provider.publicKey.toBase58(),
    orderConfig: {
      address: config.address.toBase58(),
      owner: new PublicKey(config.account.owner).toBase58(),
      roflAuthority: new PublicKey(config.account.roflAuthority).toBase58(),
      nextJobId: bnToString(config.account.nextJobId),
    },
    jobs: [],
  };

  for (const jobId of jobIds) {
    const job = await readJob(program, provider, jobId);
    output.jobs.push(job || { jobId: String(jobId), exists: false });
  }

  console.log(JSON.stringify(output, null, 2));
}

function buildJobParams(options) {
  return {
    templateId: Number(requireOption(options, "template-id")),
    numDays: Number(requireOption(options, "num-days")),
    dataTypes: parseCsvStrings(requireOption(options, "data-types")),
    maxParticipants: Number(requireOption(options, "max-participants")),
    startDayUtc: bn64(requireOption(options, "start-day-utc")),
    filterQuery: String(requireOption(options, "filter-query")),
  };
}

function sha256ArrayFromOptions(options, hexKey, textKey) {
  if (options[hexKey]) {
    const hex = String(options[hexKey]).replace(/^0x/, "");
    const bytes = Buffer.from(hex, "hex");
    if (bytes.length !== 32) {
      fail(`--${hexKey} must be 32 bytes (64 hex chars)`);
    }
    return Array.from(bytes);
  }

  const text = options[textKey] !== undefined ? String(options[textKey]) : "";
  return Array.from(createHash("sha256").update(text).digest());
}

async function commandRequestJob(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const researcherSigner = await loadActorSigner(provider, options, "researcher-keypair");
  const before = await program.account.orderConfig.fetch(config.address);
  const jobId = before.nextJobId;
  const job = deriveJobPda(program.programId, jobId);

  await program.methods
    .requestJob(buildJobParams(options))
    .accountsStrict({
      orderConfig: config.address,
      job,
      researcher: researcherSigner.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers(researcherSigner.signers)
    .rpc();

  console.log(
    JSON.stringify(
      {
        action: "request-job",
        jobId: jobId.toString(),
        job: job.toBase58(),
        researcher: researcherSigner.publicKey.toBase58(),
      },
      null,
      2
    )
  );
}

async function commandSubmitPreflight(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const roflSigner = await resolveRoflSigner(
    provider,
    new PublicKey(config.account.roflAuthority),
    options["rofl-keypair"]
  );
  if (!roflSigner) {
    fail("--rofl-keypair is required for submit-preflight");
  }

  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .submitPreflightResult(bn64(jobId), {
      effectiveParticipantsScaled: bn64(requireOption(options, "effective-participants-scaled")),
      qualityTier: Number(requireOption(options, "quality-tier")),
      finalTotal: bn64(requireOption(options, "final-total")),
      cohortHash: sha256ArrayFromOptions(options, "cohort-hash-hex", "cohort-hash-text"),
      selectedParticipants: parseCsvPublicKeys(requireOption(options, "selected-providers")),
    })
    .accountsStrict({
      orderConfig: config.address,
      job: deriveJobPda(program.programId, jobId),
      roflAuthority: roflSigner.publicKey,
    })
    .signers(roflSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "submit-preflight", jobId: String(jobId) }, null, 2));
}

async function commandConfirmJob(program, provider, options) {
  const researcherSigner = await loadActorSigner(provider, options, "researcher-keypair");
  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .confirmJobAndPay(bn64(jobId), bn64(requireOption(options, "payment-amount")))
    .accountsStrict({
      job: deriveJobPda(program.programId, jobId),
      escrowVault: deriveEscrowVaultPda(program.programId, jobId),
      researcher: researcherSigner.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers(researcherSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "confirm-job", jobId: String(jobId) }, null, 2));
}

async function commandSubmitResult(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const roflSigner = await resolveRoflSigner(
    provider,
    new PublicKey(config.account.roflAuthority),
    options["rofl-keypair"]
  );
  if (!roflSigner) {
    fail("--rofl-keypair is required for submit-result");
  }

  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .submitResult(
      bn64(jobId),
      String(requireOption(options, "result-cid")),
      sha256ArrayFromOptions(options, "output-hash-hex", "output-hash-text")
    )
    .accountsStrict({
      orderConfig: config.address,
      job: deriveJobPda(program.programId, jobId),
      roflAuthority: roflSigner.publicKey,
    })
    .signers(roflSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "submit-result", jobId: String(jobId) }, null, 2));
}

async function commandFinalizeJob(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const roflSigner = await resolveRoflSigner(
    provider,
    new PublicKey(config.account.roflAuthority),
    options["rofl-keypair"]
  );
  if (!roflSigner) {
    fail("--rofl-keypair is required for finalize-job");
  }

  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .finalizeJob(bn64(jobId))
    .accountsStrict({
      orderConfig: config.address,
      job: deriveJobPda(program.programId, jobId),
      escrowVault: deriveEscrowVaultPda(program.programId, jobId),
      roflAuthority: roflSigner.publicKey,
    })
    .signers(roflSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "finalize-job", jobId: String(jobId) }, null, 2));
}

async function commandClaimPayout(program, provider, options) {
  const providerSigner = await loadActorSigner(provider, options, "provider-keypair");
  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .claimPayout(bn64(jobId))
    .accountsStrict({
      job: deriveJobPda(program.programId, jobId),
      escrowVault: deriveEscrowVaultPda(program.programId, jobId),
      provider: providerSigner.publicKey,
    })
    .signers(providerSigner.signers)
    .rpc();

  console.log(
    JSON.stringify(
      { action: "claim-payout", jobId: String(jobId), provider: providerSigner.publicKey.toBase58() },
      null,
      2
    )
  );
}

async function commandSweepDust(program, provider, options) {
  const jobId = Number(requireOption(options, "job-id"));
  const recipient = options["recipient"]
    ? new PublicKey(options["recipient"])
    : provider.publicKey;
  await program.methods
    .sweepVaultDust(bn64(jobId))
    .accountsStrict({
      job: deriveJobPda(program.programId, jobId),
      escrowVault: deriveEscrowVaultPda(program.programId, jobId),
      recipient,
    })
    .rpc();

  console.log(JSON.stringify({ action: "sweep-dust", jobId: String(jobId), recipient: recipient.toBase58() }, null, 2));
}

async function commandCancelJob(program, provider, options) {
  const researcherSigner = await loadActorSigner(provider, options, "researcher-keypair");
  const jobId = Number(requireOption(options, "job-id"));
  await program.methods
    .cancelJob(bn64(jobId))
    .accountsStrict({
      job: deriveJobPda(program.programId, jobId),
      researcher: researcherSigner.publicKey,
    })
    .signers(researcherSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "cancel-job", jobId: String(jobId) }, null, 2));
}

async function commandFundActor(provider, options) {
  const actorKeypair = readKeypairFromFile(requireOption(options, "actor-keypair"));
  await fundActorIfNeeded(provider, actorKeypair.publicKey, lamportsTarget(options));
  const balance = await provider.connection.getBalance(actorKeypair.publicKey);
  console.log(
    JSON.stringify(
      {
        action: "fund-actor",
        actor: actorKeypair.publicKey.toBase58(),
        balanceLamports: balance,
      },
      null,
      2
    )
  );
}

async function commandSetRoflAuthority(program, provider, options) {
  const config = await ensureOrderConfig(program, provider, options);
  const newAuthority = options["rofl-keypair"]
    ? readKeypairFromFile(options["rofl-keypair"]).publicKey
    : new PublicKey(requireOption(options, "new-rofl-authority"));

  await program.methods
    .setRoflAuthority(newAuthority)
    .accountsStrict({
      orderConfig: config.address,
      owner: provider.publicKey,
    })
    .rpc();

  console.log(
    JSON.stringify(
      {
        action: "set-rofl-authority",
        orderConfig: config.address.toBase58(),
        roflAuthority: newAuthority.toBase58(),
      },
      null,
      2
    )
  );
}

async function main() {
  const { command, options } = parseArgs(process.argv.slice(2));
  if (!command || options.help || command === "help") {
    printUsage();
    return;
  }

  process.env.ANCHOR_PROVIDER_URL = options.url || process.env.ANCHOR_PROVIDER_URL || DEFAULT_RPC_URL;
  process.env.ANCHOR_WALLET = resolveInputPath(
    options.wallet || process.env.ANCHOR_WALLET || "~/.config/solana/id.json"
  );

  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = createProgram(provider);

  switch (command) {
    case "read":
      await commandRead(program, provider, options);
      break;
    case "request-job":
      await commandRequestJob(program, provider, options);
      break;
    case "submit-preflight":
      await commandSubmitPreflight(program, provider, options);
      break;
    case "confirm-job":
      await commandConfirmJob(program, provider, options);
      break;
    case "submit-result":
      await commandSubmitResult(program, provider, options);
      break;
    case "finalize-job":
      await commandFinalizeJob(program, provider, options);
      break;
    case "claim-payout":
      await commandClaimPayout(program, provider, options);
      break;
    case "sweep-dust":
      await commandSweepDust(program, provider, options);
      break;
    case "cancel-job":
      await commandCancelJob(program, provider, options);
      break;
    case "fund-actor":
      await commandFundActor(provider, options);
      break;
    case "set-rofl-authority":
      await commandSetRoflAuthority(program, provider, options);
      break;
    default:
      fail(`Unknown command: ${command}`);
  }
}

main().catch((error) => {
  const message = error instanceof Error ? error.stack || error.message : String(error);
  fail(message);
});
