const anchor = require("@coral-xyz/anchor");
const fs = require("fs");
const path = require("path");

const { Keypair, PublicKey, SystemProgram } = anchor.web3;
const idl = require("../target/idl/data_registry.json");

const REGISTRY_SEED = Buffer.from("registry_state");
const META_SEED = Buffer.from("meta");
const UNIT_SEED = Buffer.from("unit");
const DEFAULT_PROGRAM_ID = "3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT";
const DEFAULT_RPC_URL = "http://127.0.0.1:8899";
const DEFAULT_PROVIDER_TARGET_LAMPORTS = 50_000_000;

function log(message) {
  console.log(`[populate-data-registry] ${message}`);
}

function fail(message) {
  console.error(`[populate-data-registry] ERROR: ${message}`);
  process.exit(1);
}

function expandHome(inputPath) {
  if (!inputPath) {
    return inputPath;
  }

  if (inputPath.startsWith("~/")) {
    return path.join(process.env.HOME || process.env.USERPROFILE || "", inputPath.slice(2));
  }

  return inputPath;
}

function resolveInputPath(inputPath) {
  return path.resolve(process.cwd(), expandHome(inputPath));
}

function parseArgs(argv) {
  const options = {
    initializeIfNeeded: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];

    if (token === "--input") {
      options.input = argv[++index];
    } else if (token === "--url") {
      options.url = argv[++index];
    } else if (token === "--wallet") {
      options.wallet = argv[++index];
    } else if (token === "--tee-keypair") {
      options.teeKeypair = argv[++index];
    } else if (token === "--provider-min-lamports") {
      options.providerMinLamports = Number(argv[++index]);
    } else if (token === "--initialize-if-needed") {
      options.initializeIfNeeded = true;
    } else if (token === "--help" || token === "-h") {
      options.help = true;
    } else {
      fail(`Unknown argument: ${token}`);
    }
  }

  return options;
}

function printUsage() {
  console.log(
    [
      "Usage:",
      "  node ./scripts/populate-data-registry.js --input <json-file> [--url <rpc-url>] [--wallet <keypair>] [--tee-keypair <keypair>] [--initialize-if-needed]",
      "",
      "Notes:",
      "  --input points to a JSON payload shaped like docs/data-registry-populate.example.json.",
      "  --wallet defaults to ANCHOR_WALLET or ~/.config/solana/id.json.",
      "  --url defaults to ANCHOR_PROVIDER_URL or localnet.",
      "  Provider wallets are auto-funded by the main --wallet before they submit data.",
      "  --provider-min-lamports overrides the per-provider target balance (default 50000000 lamports).",
      "  --tee-keypair is required only when the payload wants feat CID updates and the registry tee authority is not the wallet.",
    ].join("\n")
  );
}

function readJsonFile(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function readKeypairFromFile(filePath) {
  const secret = readJsonFile(resolveInputPath(filePath));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

function bn64(value) {
  return new anchor.BN(value);
}

function metaIdBuffer(metaId) {
  return metaId.toArrayLike(Buffer, "le", 8);
}

function unitIndexBuffer(unitIndex) {
  return new anchor.BN(unitIndex).toArrayLike(Buffer, "le", 4);
}

function deriveRegistryStatePda(programId) {
  return PublicKey.findProgramAddressSync([REGISTRY_SEED], programId)[0];
}

function deriveMetaPda(programId, metaId) {
  return PublicKey.findProgramAddressSync(
    [META_SEED, metaIdBuffer(metaId)],
    programId
  )[0];
}

function deriveUnitPda(programId, metaId, unitIndex) {
  return PublicKey.findProgramAddressSync(
    [UNIT_SEED, metaIdBuffer(metaId), unitIndexBuffer(unitIndex)],
    programId
  )[0];
}

function createProgram(provider) {
  if (!idl.address) {
    idl.address = DEFAULT_PROGRAM_ID;
  }

  return new anchor.Program(idl, provider);
}

function normalizeEntry(entry, index) {
  if (!entry || typeof entry !== "object") {
    fail(`Entry ${index} is not an object.`);
  }

  if (!entry.rawCid) {
    fail(`Entry ${index} is missing rawCid.`);
  }

  if (!Array.isArray(entry.dataTypes) || entry.dataTypes.length === 0) {
    fail(`Entry ${index} must include at least one data type.`);
  }

  if (!Array.isArray(entry.chronicConditions)) {
    fail(`Entry ${index} chronicConditions must be an array.`);
  }

  if (typeof entry.dayStartTimestamp !== "number" || typeof entry.dayEndTimestamp !== "number") {
    fail(`Entry ${index} must include numeric dayStartTimestamp and dayEndTimestamp.`);
  }

  if (!Array.isArray(entry.appendedUploads)) {
    entry.appendedUploads = [];
  }

  return entry;
}

function entryNeedsFeatUpdates(entry) {
  if (entry.initialFeatCid) {
    return true;
  }

  return entry.appendedUploads.some((upload) => upload.featCid);
}

function lamportsTargetForEntry(entry, options) {
  if (
    options.providerMinLamports !== undefined &&
    Number.isFinite(options.providerMinLamports)
  ) {
    return Math.floor(options.providerMinLamports);
  }

  if (
    entry.providerMinLamports !== undefined &&
    Number.isFinite(entry.providerMinLamports)
  ) {
    return Math.floor(entry.providerMinLamports);
  }

  return DEFAULT_PROVIDER_TARGET_LAMPORTS;
}

async function transferLamports(provider, recipient, lamports) {
  const tx = new anchor.web3.Transaction().add(
    SystemProgram.transfer({
      fromPubkey: provider.publicKey,
      toPubkey: recipient,
      lamports,
    })
  );

  await provider.sendAndConfirm(tx, []);
}

async function fundProviderIfNeeded(provider, providerPublicKey, targetLamports) {
  if (providerPublicKey.equals(provider.publicKey)) {
    return;
  }

  const currentBalance = await provider.connection.getBalance(providerPublicKey);
  if (currentBalance >= targetLamports) {
    log(
      `Funding skipped for provider ${providerPublicKey.toBase58()}; balance ${currentBalance} already meets target ${targetLamports}`
    );
    return;
  }

  const shortfall = targetLamports - currentBalance;
  const payerBalance = await provider.connection.getBalance(provider.publicKey);
  if (payerBalance < shortfall) {
    fail(
      `Main wallet ${provider.publicKey.toBase58()} has ${payerBalance} lamports, but ${shortfall} lamports are needed to fund provider ${providerPublicKey.toBase58()}.`
    );
  }

  log(
    `Funding provider ${providerPublicKey.toBase58()} with ${shortfall} lamports from main wallet ${provider.publicKey.toBase58()}`
  );
  await transferLamports(provider, providerPublicKey, shortfall);
}

async function ensureRegistryReady(program, provider, options, payload, teeSigner) {
  const registryState = deriveRegistryStatePda(program.programId);
  const existing = await program.account.registryState.fetchNullable(registryState);

  if (!existing) {
    const requestedTeeAuthority = teeSigner
      ? teeSigner.publicKey
      : payload.teeAuthority
        ? new PublicKey(payload.teeAuthority)
        : provider.publicKey;

    if (!options.initializeIfNeeded) {
      fail(
        `Registry state ${registryState.toBase58()} does not exist on ${provider.connection.rpcEndpoint}. Re-run with --initialize-if-needed if you want this script to create it.`
      );
    }

    log(
      `Initializing registry ${registryState.toBase58()} with owner ${provider.publicKey.toBase58()} and tee authority ${requestedTeeAuthority.toBase58()}`
    );
    await program.methods
      .initializeRegistry(requestedTeeAuthority)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    return {
      registryState,
      teeAuthority: requestedTeeAuthority,
    };
  }

  const owner = new PublicKey(existing.owner);
  const teeAuthority = new PublicKey(existing.teeAuthority);

  if (existing.paused) {
    if (!owner.equals(provider.publicKey)) {
      fail(
        `Registry ${registryState.toBase58()} is paused and owned by ${owner.toBase58()}. Use the owner wallet to unpause it first.`
      );
    }

    log(`Unpausing registry ${registryState.toBase58()}`);
    await program.methods
      .setPaused(false)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
      })
      .rpc();
  }

  return {
    registryState,
    teeAuthority,
  };
}

async function resolveTeeSigner(provider, registryTeeAuthority, teeKeypairPath) {
  if (registryTeeAuthority.equals(provider.publicKey)) {
    return {
      publicKey: provider.publicKey,
      signers: [],
    };
  }

  if (!teeKeypairPath) {
    return null;
  }

  const teeKeypair = readKeypairFromFile(teeKeypairPath);
  if (!teeKeypair.publicKey.equals(registryTeeAuthority)) {
    fail(
      `Provided tee keypair ${teeKeypair.publicKey.toBase58()} does not match registry tee authority ${registryTeeAuthority.toBase58()}.`
    );
  }

  return {
    publicKey: teeKeypair.publicKey,
    signers: [teeKeypair],
  };
}

async function uploadEntry(program, provider, registryState, teeSigner, rawEntry, entryIndex, options) {
  const entry = normalizeEntry(rawEntry, entryIndex);
  const providerSigner = entry.ownerKeypair
    ? readKeypairFromFile(entry.ownerKeypair)
    : null;
  const providerPublicKey = providerSigner ? providerSigner.publicKey : provider.publicKey;
  const providerSigners = providerSigner ? [providerSigner] : [];
  const targetLamports = lamportsTargetForEntry(entry, options);

  await fundProviderIfNeeded(provider, providerPublicKey, targetLamports);

  const registryBefore = await program.account.registryState.fetch(registryState);
  const metaId = registryBefore.nextMetaId;
  const dataEntryMeta = deriveMetaPda(program.programId, metaId);
  const initialUploadUnit = deriveUnitPda(program.programId, metaId, 0);

  log(
    `Creating meta ${metaId.toString()} for provider ${providerPublicKey.toBase58()} with ${entry.dataTypes.join(", ")}`
  );

  await program.methods
    .uploadNewMeta({
      rawCid: entry.rawCid,
      dataTypes: entry.dataTypes,
      deviceType: entry.deviceType,
      deviceModel: entry.deviceModel,
      serviceProvider: entry.serviceProvider,
      dayStartTimestamp: bn64(entry.dayStartTimestamp),
      dayEndTimestamp: bn64(entry.dayEndTimestamp),
      age: entry.age,
      gender: entry.gender,
      height: entry.height,
      weight: entry.weight,
      region: entry.region,
      physicalActivityLevel: entry.physicalActivityLevel,
      smoker: entry.smoker,
      diet: entry.diet,
      chronicConditions: Buffer.from(entry.chronicConditions),
    })
    .accountsStrict({
      registryState,
      dataEntryMeta,
      uploadUnit: initialUploadUnit,
      provider: providerPublicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers(providerSigners)
    .rpc();

  if (entry.initialFeatCid) {
    if (!teeSigner) {
      fail(`Entry ${entryIndex} requests initialFeatCid, but no usable tee signer is available.`);
    }

    log(`Attaching feat CID to meta ${metaId.toString()} unit 0`);
    await program.methods
      .updateUploadUnit(metaId, 0, entry.initialFeatCid)
      .accountsStrict({
        registryState,
        uploadUnit: initialUploadUnit,
        teeAuthority: teeSigner.publicKey,
      })
      .signers(teeSigner.signers)
      .rpc();
  }

  for (let uploadIndex = 0; uploadIndex < entry.appendedUploads.length; uploadIndex += 1) {
    const upload = entry.appendedUploads[uploadIndex];
    const unitIndex = uploadIndex + 1;
    const uploadUnit = deriveUnitPda(program.programId, metaId, unitIndex);

    log(`Appending raw upload to meta ${metaId.toString()} as unit ${unitIndex}`);
    await program.methods
      .registerRawUpload(
        metaId,
        upload.rawCid,
        bn64(upload.dayStartTimestamp),
        bn64(upload.dayEndTimestamp)
      )
      .accountsStrict({
        registryState,
        dataEntryMeta,
        uploadUnit,
        provider: providerPublicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers(providerSigners)
      .rpc();

    if (upload.featCid) {
      if (!teeSigner) {
        fail(`Entry ${entryIndex} unit ${unitIndex} requests featCid, but no usable tee signer is available.`);
      }

      log(`Attaching feat CID to meta ${metaId.toString()} unit ${unitIndex}`);
      await program.methods
        .updateUploadUnit(metaId, unitIndex, upload.featCid)
        .accountsStrict({
          registryState,
          uploadUnit,
          teeAuthority: teeSigner.publicKey,
        })
        .signers(teeSigner.signers)
        .rpc();
    }
  }

  const meta = await program.account.dataEntryMeta.fetch(dataEntryMeta);
  log(
    `Created meta ${metaId.toString()} at ${dataEntryMeta.toBase58()} with ${meta.unitCount} unit(s) and total duration ${meta.totalDuration.toString()}`
  );

  return {
    metaId: metaId.toString(),
    dataEntryMeta: dataEntryMeta.toBase58(),
    owner: providerPublicKey.toBase58(),
    unitCount: meta.unitCount,
    totalDuration: meta.totalDuration.toString(),
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printUsage();
    return;
  }

  if (!options.input) {
    printUsage();
    fail("--input is required.");
  }

  const inputPath = resolveInputPath(options.input);
  const payload = readJsonFile(inputPath);
  if (!payload || !Array.isArray(payload.entries) || payload.entries.length === 0) {
    fail("Input JSON must contain a non-empty entries array.");
  }

  process.env.ANCHOR_PROVIDER_URL = options.url || process.env.ANCHOR_PROVIDER_URL || DEFAULT_RPC_URL;
  process.env.ANCHOR_WALLET =
    resolveInputPath(options.wallet || process.env.ANCHOR_WALLET || "~/.config/solana/id.json");

  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  log(`RPC URL: ${provider.connection.rpcEndpoint}`);
  log(`Wallet: ${provider.publicKey.toBase58()}`);

  const program = createProgram(provider);
  log(`Program ID: ${program.programId.toBase58()}`);
  const teeKeypair = options.teeKeypair ? readKeypairFromFile(options.teeKeypair) : null;
  const registry = await ensureRegistryReady(program, provider, options, payload, teeKeypair);
  const payloadNeedsFeatUpdates = payload.entries.some(entryNeedsFeatUpdates);
  const teeSigner = await resolveTeeSigner(
    provider,
    registry.teeAuthority,
    options.teeKeypair
  );

  if (payloadNeedsFeatUpdates && !teeSigner) {
    fail(
      `Registry tee authority is ${registry.teeAuthority.toBase58()}, so --tee-keypair is required for feat CID updates in this payload.`
    );
  }

  const results = [];
  for (let index = 0; index < payload.entries.length; index += 1) {
    const result = await uploadEntry(
      program,
      provider,
      registry.registryState,
      teeSigner,
      payload.entries[index],
      index,
      options
    );
    results.push(result);
  }

  console.log(JSON.stringify({
    registryState: registry.registryState.toBase58(),
    teeAuthority: registry.teeAuthority.toBase58(),
    results,
  }, null, 2));
}

main().catch((error) => {
  const message = error instanceof Error ? error.stack || error.message : String(error);
  fail(message);
});
