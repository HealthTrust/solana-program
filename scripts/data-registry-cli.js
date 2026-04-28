const anchor = require("@coral-xyz/anchor");
const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const { Keypair, PublicKey, SystemProgram, Transaction } = anchor.web3;
const idl = require("../target/idl/data_registry.json");

const REGISTRY_SEED = Buffer.from("registry_state");
const META_SEED = Buffer.from("meta");
const UNIT_SEED = Buffer.from("unit");
const DEFAULT_PROGRAM_ID = "3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT";
const DEFAULT_RPC_URL = "http://127.0.0.1:8899";
const DEFAULT_PROVIDER_TARGET_LAMPORTS = 50_000_000;

function fail(message) {
  console.error(`[data-registry-cli] ERROR: ${message}`);
  process.exit(1);
}

function log(message) {
  console.log(`[data-registry-cli] ${message}`);
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
  if (!idl.address) {
    idl.address = DEFAULT_PROGRAM_ID;
  }

  return new anchor.Program(idl, provider);
}

function bn64(value) {
  return new anchor.BN(value);
}

function bnToString(value) {
  return value && typeof value.toString === "function" ? value.toString() : value;
}

function metaIdBuffer(metaId) {
  return new anchor.BN(metaId).toArrayLike(Buffer, "le", 8);
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
  const booleanOptions = new Set([
    "help",
    "initialize-if-needed",
    "skip-feat-cids",
  ]);

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
      "  node ./scripts/data-registry-cli.js <command> [options]",
      "",
      "Commands:",
      "  read                Read registry state and one or more metas",
      "  upload-meta         Create a new meta entry with unit 0",
      "  append-upload       Add a new raw upload unit to an existing meta",
      "  update-feat         Attach or replace featCid on an upload unit",
      "  close-meta          Close a meta account",
      "  close-unit          Close a specific upload unit",
      "  fund-provider       Transfer SOL from the main wallet to a provider wallet",
      "  generate-real-payload Generate real encrypted raw data + optional feat CIDs from a template",
      "  populate            Bulk populate from a JSON file",
      "  populate-real       Generate real CIDs from a template, then populate on-chain",
      "",
      "Global options:",
      "  --url <rpc-url>",
      "  --wallet <keypair>",
      "  --tee-keypair <keypair>",
      "",
      "Examples:",
      "  node ./scripts/data-registry-cli.js read --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --meta-from 12 --meta-to 16",
      "  node ./scripts/data-registry-cli.js upload-meta --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --provider-keypair ./.anchor/provider1.json --data-types heart_rate,sleep --device-type Smartwatch --device-model 'Apple Watch Series 9' --service-provider HealthTrust --day-start 1713916800 --day-end 1714003200 --age 29 --gender 1 --height 165 --weight 58 --region 1 --physical-activity-level 2 --smoker 0 --diet 1 --chronic-conditions 1 --raw-cid QmExample",
      "  node ./scripts/data-registry-cli.js update-feat --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --tee-keypair \\\\wsl.localhost\\Ubuntu\\home\\giorgos\\.config\\solana\\devnet-tee.json --meta-id 12 --unit-index 0 --feat-cid bafy...",
      "  node ./scripts/data-registry-cli.js generate-real-payload --template ./docs/data-registry-populate.example.json --output ./docs/data-registry-populate.generated.json --tee-backend-dir ../MVP-TEE-Backend --skip-feat-cids",
      "  node ./scripts/data-registry-cli.js populate --input ./docs/data-registry-populate.example.json --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --tee-keypair \\\\wsl.localhost\\Ubuntu\\home\\giorgos\\.config\\solana\\devnet-tee.json",
      "  node ./scripts/data-registry-cli.js populate-real --template ./docs/data-registry-populate.example.json --output ./docs/data-registry-populate.generated.json --tee-backend-dir ../MVP-TEE-Backend --skip-feat-cids --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json",
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

function parseCsvNumbers(value) {
  return parseCsvStrings(value).map((item) => Number(item));
}

function lamportsTarget(options) {
  if (Number.isFinite(options["provider-min-lamports"])) {
    return Math.floor(options["provider-min-lamports"]);
  }

  return DEFAULT_PROVIDER_TARGET_LAMPORTS;
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

async function ensureRegistryState(program, provider, options) {
  const registryState = deriveRegistryStatePda(program.programId);
  const existing = await program.account.registryState.fetchNullable(registryState);
  if (!existing) {
    if (!options["initialize-if-needed"]) {
      fail(`Registry state ${registryState.toBase58()} does not exist. Pass --initialize-if-needed to create it.`);
    }

    const teeAuthority = options["tee-keypair"]
      ? readKeypairFromFile(options["tee-keypair"]).publicKey
      : options["tee-authority"]
        ? new PublicKey(options["tee-authority"])
        : provider.publicKey;

    await program.methods
      .initializeRegistry(teeAuthority)
      .accountsStrict({
        registryState,
        owner: provider.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    return {
      address: registryState,
      account: await program.account.registryState.fetch(registryState),
    };
  }

  return { address: registryState, account: existing };
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

async function readMeta(program, metaId) {
  const metaAddress = deriveMetaPda(program.programId, metaId);
  const meta = await program.account.dataEntryMeta.fetchNullable(metaAddress);
  if (!meta) {
    return null;
  }

  const unitCount = Number(meta.unitCount);
  const units = [];
  for (let unitIndex = 0; unitIndex < unitCount; unitIndex += 1) {
    const unitAddress = deriveUnitPda(program.programId, metaId, unitIndex);
    const unit = await program.account.uploadUnit.fetchNullable(unitAddress);
    units.push({
      address: unitAddress.toBase58(),
      exists: Boolean(unit),
      unitIndex,
      rawCid: unit ? unit.rawCid : null,
      featCid: unit ? unit.featCid : null,
      dayStartTimestamp: unit ? bnToString(unit.dayStartTimestamp) : null,
      dayEndTimestamp: unit ? bnToString(unit.dayEndTimestamp) : null,
      dateOfCreation: unit ? bnToString(unit.dateOfCreation) : null,
    });
  }

  return {
    metaId: String(metaId),
    address: metaAddress.toBase58(),
    owner: new PublicKey(meta.owner).toBase58(),
    deviceType: meta.deviceType,
    deviceModel: meta.deviceModel,
    serviceProvider: meta.serviceProvider,
    dataTypes: meta.dataTypes,
    chronicConditions: Array.from(meta.chronicConditions),
    age: meta.age,
    gender: meta.gender,
    height: meta.height,
    weight: meta.weight,
    region: meta.region,
    physicalActivityLevel: meta.physicalActivityLevel,
    smoker: meta.smoker,
    diet: meta.diet,
    totalDuration: bnToString(meta.totalDuration),
    unitCount,
    dateOfCreation: bnToString(meta.dateOfCreation),
    dateOfModification: bnToString(meta.dateOfModification),
    units,
  };
}

function normalizeMetaIds(options) {
  if (options["meta-ids"]) {
    return parseCsvNumbers(options["meta-ids"]).filter(Number.isFinite);
  }

  if (Number.isFinite(options["meta-from"]) && Number.isFinite(options["meta-to"])) {
    const ids = [];
    for (let value = options["meta-from"]; value <= options["meta-to"]; value += 1) {
      ids.push(value);
    }
    return ids;
  }

  if (Number.isFinite(options["meta-id"])) {
    return [options["meta-id"]];
  }

  return [];
}

async function commandRead(program, provider, options) {
  const registry = await ensureRegistryState(program, provider, options);
  const metaIds = normalizeMetaIds(options);
  const output = {
    rpcUrl: provider.connection.rpcEndpoint,
    wallet: provider.publicKey.toBase58(),
    registryState: {
      address: registry.address.toBase58(),
      owner: new PublicKey(registry.account.owner).toBase58(),
      pricingProgram: new PublicKey(registry.account.pricingProgram).toBase58(),
      teeAuthority: new PublicKey(registry.account.teeAuthority).toBase58(),
      nextMetaId: bnToString(registry.account.nextMetaId),
      paused: registry.account.paused,
    },
    metas: [],
  };

  for (const metaId of metaIds) {
    const meta = await readMeta(program, metaId);
    output.metas.push(meta || { metaId: String(metaId), exists: false });
  }

  console.log(JSON.stringify(output, null, 2));
}

function buildUploadMetaParams(options) {
  return {
    rawCid: String(requireOption(options, "raw-cid")),
    dataTypes: parseCsvStrings(requireOption(options, "data-types")),
    deviceType: String(requireOption(options, "device-type")),
    deviceModel: String(requireOption(options, "device-model")),
    serviceProvider: String(requireOption(options, "service-provider")),
    dayStartTimestamp: bn64(requireOption(options, "day-start")),
    dayEndTimestamp: bn64(requireOption(options, "day-end")),
    age: Number(requireOption(options, "age")),
    gender: Number(requireOption(options, "gender")),
    height: Number(requireOption(options, "height")),
    weight: Number(requireOption(options, "weight")),
    region: Number(requireOption(options, "region")),
    physicalActivityLevel: Number(requireOption(options, "physical-activity-level")),
    smoker: Number(requireOption(options, "smoker")),
    diet: Number(requireOption(options, "diet")),
    chronicConditions: Buffer.from(parseCsvNumbers(options["chronic-conditions"])),
  };
}

async function loadProviderSigner(provider, options) {
  const providerKeypairPath = options["provider-keypair"];
  if (!providerKeypairPath) {
    return { publicKey: provider.publicKey, signers: [] };
  }

  const providerSigner = readKeypairFromFile(providerKeypairPath);
  await fundProviderIfNeeded(provider, providerSigner.publicKey, lamportsTarget(options));
  return { publicKey: providerSigner.publicKey, signers: [providerSigner] };
}

async function commandUploadMeta(program, provider, options) {
  const registry = await ensureRegistryState(program, provider, options);
  const registryState = registry.address;
  const providerSigner = await loadProviderSigner(provider, options);
  const registryBefore = await program.account.registryState.fetch(registryState);
  const metaId = registryBefore.nextMetaId;
  const metaAddress = deriveMetaPda(program.programId, metaId);
  const unitAddress = deriveUnitPda(program.programId, metaId, 0);

  await program.methods
    .uploadNewMeta(buildUploadMetaParams(options))
    .accountsStrict({
      registryState,
      dataEntryMeta: metaAddress,
      uploadUnit: unitAddress,
      provider: providerSigner.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers(providerSigner.signers)
    .rpc();

  const featCid = options["feat-cid"];
  if (featCid) {
    const teeSigner = await resolveTeeSigner(
      provider,
      new PublicKey(registry.account.teeAuthority),
      options["tee-keypair"]
    );
    if (!teeSigner) {
      fail("--tee-keypair is required for --feat-cid");
    }

    await program.methods
      .updateUploadUnit(metaId, 0, String(featCid))
      .accountsStrict({
        registryState,
        uploadUnit: unitAddress,
        teeAuthority: teeSigner.publicKey,
      })
      .signers(teeSigner.signers)
      .rpc();
  }

  console.log(
    JSON.stringify(
      {
        action: "upload-meta",
        metaId: metaId.toString(),
        dataEntryMeta: metaAddress.toBase58(),
        uploadUnit: unitAddress.toBase58(),
        owner: providerSigner.publicKey.toBase58(),
      },
      null,
      2
    )
  );
}

async function commandAppendUpload(program, provider, options) {
  const registry = await ensureRegistryState(program, provider, options);
  const registryState = registry.address;
  const providerSigner = await loadProviderSigner(provider, options);
  const metaId = Number(requireOption(options, "meta-id"));
  const metaAddress = deriveMetaPda(program.programId, metaId);
  const meta = await program.account.dataEntryMeta.fetch(metaAddress);
  const unitIndex = Number(meta.unitCount);
  const unitAddress = deriveUnitPda(program.programId, metaId, unitIndex);

  await program.methods
    .registerRawUpload(
      bn64(metaId),
      String(requireOption(options, "raw-cid")),
      bn64(requireOption(options, "day-start")),
      bn64(requireOption(options, "day-end"))
    )
    .accountsStrict({
      registryState,
      dataEntryMeta: metaAddress,
      uploadUnit: unitAddress,
      provider: providerSigner.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .signers(providerSigner.signers)
    .rpc();

  const featCid = options["feat-cid"];
  if (featCid) {
    const teeSigner = await resolveTeeSigner(
      provider,
      new PublicKey(registry.account.teeAuthority),
      options["tee-keypair"]
    );
    if (!teeSigner) {
      fail("--tee-keypair is required for --feat-cid");
    }

    await program.methods
      .updateUploadUnit(metaId, unitIndex, String(featCid))
      .accountsStrict({
        registryState,
        uploadUnit: unitAddress,
        teeAuthority: teeSigner.publicKey,
      })
      .signers(teeSigner.signers)
      .rpc();
  }

  console.log(
    JSON.stringify(
      {
        action: "append-upload",
        metaId: String(metaId),
        unitIndex,
        uploadUnit: unitAddress.toBase58(),
        owner: providerSigner.publicKey.toBase58(),
      },
      null,
      2
    )
  );
}

async function commandUpdateFeat(program, provider, options) {
  const registry = await ensureRegistryState(program, provider, options);
  const teeSigner = await resolveTeeSigner(
    provider,
    new PublicKey(registry.account.teeAuthority),
    options["tee-keypair"]
  );
  if (!teeSigner) {
    fail("--tee-keypair is required for update-feat");
  }

  const metaId = Number(requireOption(options, "meta-id"));
  const unitIndex = Number(requireOption(options, "unit-index"));
  const uploadUnit = deriveUnitPda(program.programId, metaId, unitIndex);

  await program.methods
    .updateUploadUnit(metaId, unitIndex, String(requireOption(options, "feat-cid")))
    .accountsStrict({
      registryState: registry.address,
      uploadUnit,
      teeAuthority: teeSigner.publicKey,
    })
    .signers(teeSigner.signers)
    .rpc();

  console.log(
    JSON.stringify(
      {
        action: "update-feat",
        metaId: String(metaId),
        unitIndex,
        uploadUnit: uploadUnit.toBase58(),
      },
      null,
      2
    )
  );
}

async function commandCloseMeta(program, provider, options) {
  const registry = await ensureRegistryState(program, provider, options);
  const providerSigner = await loadProviderSigner(provider, options);
  const metaId = Number(requireOption(options, "meta-id"));
  const metaAddress = deriveMetaPda(program.programId, metaId);

  await program.methods
    .closeDataEntryMeta(bn64(metaId))
    .accountsStrict({
      registryState: registry.address,
      dataEntryMeta: metaAddress,
      provider: providerSigner.publicKey,
    })
    .signers(providerSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "close-meta", metaId: String(metaId), dataEntryMeta: metaAddress.toBase58() }, null, 2));
}

async function commandCloseUnit(program, provider, options) {
  const providerSigner = await loadProviderSigner(provider, options);
  const metaId = Number(requireOption(options, "meta-id"));
  const unitIndex = Number(requireOption(options, "unit-index"));
  const metaAddress = deriveMetaPda(program.programId, metaId);
  const unitAddress = deriveUnitPda(program.programId, metaId, unitIndex);

  await program.methods
    .closeUploadUnit(bn64(metaId), unitIndex)
    .accountsStrict({
      dataEntryMeta: metaAddress,
      uploadUnit: unitAddress,
      provider: providerSigner.publicKey,
    })
    .signers(providerSigner.signers)
    .rpc();

  console.log(JSON.stringify({ action: "close-unit", metaId: String(metaId), unitIndex, uploadUnit: unitAddress.toBase58() }, null, 2));
}

async function commandFundProvider(provider, options) {
  const providerKeypair = readKeypairFromFile(requireOption(options, "provider-keypair"));
  await fundProviderIfNeeded(provider, providerKeypair.publicKey, lamportsTarget(options));
  const balance = await provider.connection.getBalance(providerKeypair.publicKey);
  console.log(
    JSON.stringify(
      {
        action: "fund-provider",
        provider: providerKeypair.publicKey.toBase58(),
        balanceLamports: balance,
      },
      null,
      2
    )
  );
}

function defaultGeneratedPayloadPath(templatePath) {
  const parsed = path.parse(templatePath);
  return path.join(parsed.dir, `${parsed.name}.generated${parsed.ext || ".json"}`);
}

function defaultArtifactDir(outputPath) {
  return path.join(path.dirname(outputPath), "generated-ipfs-artifacts");
}

function runTeeGenerator(teeBackendDir, generatorArgs) {
  const result = spawnSync("go", ["run", "./cmd/generate-registry-cids", ...generatorArgs], {
    cwd: teeBackendDir,
    env: process.env,
    encoding: "utf8",
  });

  if (result.error) {
    fail(`run TEE generator: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`TEE generator failed: ${result.stderr || result.stdout || "unknown error"}`);
  }

  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    fail(`parse TEE generator output: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function generateUnitName(entry, entryIndex, unitIndex) {
  const providerPath = typeof entry.ownerKeypair === "string"
    ? path.basename(entry.ownerKeypair, path.extname(entry.ownerKeypair))
    : `provider-${entryIndex + 1}`;
  return `${providerPath}-entry-${entryIndex + 1}-unit-${unitIndex}`;
}

function buildGeneratorArgs(entry, unit, entryIndex, unitIndex, artifactDir) {
  const args = [
    "--name", generateUnitName(entry, entryIndex, unitIndex),
    "--data-types", entry.dataTypes.join(","),
    "--day-start", String(unit.dayStartTimestamp),
    "--day-end", String(unit.dayEndTimestamp),
  ];

  if (artifactDir) {
    args.push("--output-dir", artifactDir);
  }

  return args;
}

function normalizePopulateEntry(entry) {
  if (!entry || typeof entry !== "object") {
    fail("Each populate entry must be an object");
  }
  return {
    ...entry,
    appendedUploads: Array.isArray(entry.appendedUploads) ? entry.appendedUploads : [],
  };
}

async function generateRealPayloadFromTemplate(options) {
  const templatePath = resolveInputPath(requireOption(options, "template"));
  const outputPath = resolveInputPath(options.output || defaultGeneratedPayloadPath(templatePath));
  const teeBackendDir = resolveInputPath(options["tee-backend-dir"] || "..\\MVP-TEE-Backend");
  const artifactDir = resolveInputPath(options["artifact-dir"] || defaultArtifactDir(outputPath));
  const template = readJsonFile(templatePath);

  if (!template || !Array.isArray(template.entries) || template.entries.length === 0) {
    fail("Template payload must contain a non-empty entries array");
  }

  const generated = {
    teeAuthority: template.teeAuthority ?? null,
    entries: [],
  };
  const skipFeatCids = Boolean(options["skip-feat-cids"]);

  template.entries.map(normalizePopulateEntry).forEach((entry, entryIndex) => {
    const generatedEntry = {
      ...entry,
      appendedUploads: [],
    };
    delete generatedEntry.initialFeatCid;

    const initialUnit = {
      dayStartTimestamp: entry.dayStartTimestamp,
      dayEndTimestamp: entry.dayEndTimestamp,
    };
    const initialResult = runTeeGenerator(
      teeBackendDir,
      [
        ...buildGeneratorArgs(entry, initialUnit, entryIndex, 0, artifactDir),
        ...(skipFeatCids ? ["--skip-feat-upload"] : []),
      ]
    );
    generatedEntry.rawCid = initialResult.rawCid;
    if (!skipFeatCids && initialResult.featCID) {
      generatedEntry.initialFeatCid = initialResult.featCID;
    }
    generatedEntry.generatedArtifacts = {
      initial: initialResult,
      appended: [],
    };

    entry.appendedUploads.forEach((upload, uploadIndex) => {
      const result = runTeeGenerator(
        teeBackendDir,
        [
          ...buildGeneratorArgs(entry, upload, entryIndex, uploadIndex + 1, artifactDir),
          ...(skipFeatCids ? ["--skip-feat-upload"] : []),
        ]
      );
      const generatedUpload = {
        ...upload,
        rawCid: result.rawCid,
      };
      delete generatedUpload.featCid;
      if (!skipFeatCids && result.featCID) {
        generatedUpload.featCid = result.featCID;
      }
      generatedEntry.appendedUploads.push(generatedUpload);
      generatedEntry.generatedArtifacts.appended.push(result);
    });

    generated.entries.push(generatedEntry);
  });

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, JSON.stringify(generated, null, 2));

  return {
    outputPath,
    artifactDir,
    payload: generated,
  };
}

async function commandGenerateRealPayload(options) {
  const generated = await generateRealPayloadFromTemplate(options);
  console.log(
    JSON.stringify(
      {
        action: "generate-real-payload",
        outputPath: generated.outputPath,
        artifactDir: generated.artifactDir,
        entries: generated.payload.entries.map((entry) => ({
          ownerKeypair: entry.ownerKeypair,
          rawCid: entry.rawCid,
          initialFeatCid: entry.initialFeatCid ?? null,
          appendedUploads: entry.appendedUploads.map((upload) => ({
            rawCid: upload.rawCid,
            featCid: upload.featCid ?? null,
          })),
        })),
      },
      null,
      2
    )
  );
}

async function commandPopulate(program, provider, options) {
  const inputPath = resolveInputPath(requireOption(options, "input"));
  const payload = readJsonFile(inputPath);
  if (!payload || !Array.isArray(payload.entries) || payload.entries.length === 0) {
    fail("Populate payload must contain a non-empty entries array");
  }

  const registry = await ensureRegistryState(program, provider, options);
  const teeSigner = await resolveTeeSigner(
    provider,
    new PublicKey(registry.account.teeAuthority),
    options["tee-keypair"]
  );
  const results = [];

  for (const entry of payload.entries.map(normalizePopulateEntry)) {
    const providerSigner = entry.ownerKeypair
      ? readKeypairFromFile(entry.ownerKeypair)
      : null;
    const providerPublicKey = providerSigner ? providerSigner.publicKey : provider.publicKey;
    const providerSigners = providerSigner ? [providerSigner] : [];

    await fundProviderIfNeeded(provider, providerPublicKey, lamportsTarget({
      ...options,
      "provider-min-lamports": Number.isFinite(entry.providerMinLamports)
        ? entry.providerMinLamports
        : options["provider-min-lamports"],
    }));

    const registryBefore = await program.account.registryState.fetch(registry.address);
    const metaId = registryBefore.nextMetaId;
    const dataEntryMeta = deriveMetaPda(program.programId, metaId);
    const uploadUnit0 = deriveUnitPda(program.programId, metaId, 0);

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
        chronicConditions: Buffer.from(entry.chronicConditions || []),
      })
      .accountsStrict({
        registryState: registry.address,
        dataEntryMeta,
        uploadUnit: uploadUnit0,
        provider: providerPublicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers(providerSigners)
      .rpc();

    if (entry.initialFeatCid) {
      if (!teeSigner) {
        fail("Populate payload includes featCid updates but no usable tee signer is available");
      }
      await program.methods
        .updateUploadUnit(metaId, 0, entry.initialFeatCid)
        .accountsStrict({
          registryState: registry.address,
          uploadUnit: uploadUnit0,
          teeAuthority: teeSigner.publicKey,
        })
        .signers(teeSigner.signers)
        .rpc();
    }

    for (let uploadIndex = 0; uploadIndex < entry.appendedUploads.length; uploadIndex += 1) {
      const appended = entry.appendedUploads[uploadIndex];
      const unitIndex = uploadIndex + 1;
      const uploadUnit = deriveUnitPda(program.programId, metaId, unitIndex);
      await program.methods
        .registerRawUpload(
          metaId,
          appended.rawCid,
          bn64(appended.dayStartTimestamp),
          bn64(appended.dayEndTimestamp)
        )
        .accountsStrict({
          registryState: registry.address,
          dataEntryMeta,
          uploadUnit,
          provider: providerPublicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers(providerSigners)
        .rpc();

      if (appended.featCid) {
        if (!teeSigner) {
          fail("Populate payload includes featCid updates but no usable tee signer is available");
        }
        await program.methods
          .updateUploadUnit(metaId, unitIndex, appended.featCid)
          .accountsStrict({
            registryState: registry.address,
            uploadUnit,
            teeAuthority: teeSigner.publicKey,
          })
          .signers(teeSigner.signers)
          .rpc();
      }
    }

    const meta = await program.account.dataEntryMeta.fetch(dataEntryMeta);
    results.push({
      metaId: metaId.toString(),
      dataEntryMeta: dataEntryMeta.toBase58(),
      owner: providerPublicKey.toBase58(),
      unitCount: meta.unitCount,
      totalDuration: meta.totalDuration.toString(),
    });
  }

  console.log(
    JSON.stringify(
      {
        action: "populate",
        registryState: registry.address.toBase58(),
        results,
      },
      null,
      2
    )
  );
}

async function commandPopulateReal(program, provider, options) {
  const generated = await generateRealPayloadFromTemplate(options);
  log(`Generated real payload at ${generated.outputPath}`);
  await commandPopulate(program, provider, {
    ...options,
    input: generated.outputPath,
  });
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
    case "upload-meta":
      await commandUploadMeta(program, provider, options);
      break;
    case "append-upload":
      await commandAppendUpload(program, provider, options);
      break;
    case "update-feat":
      await commandUpdateFeat(program, provider, options);
      break;
    case "close-meta":
      await commandCloseMeta(program, provider, options);
      break;
    case "close-unit":
      await commandCloseUnit(program, provider, options);
      break;
    case "fund-provider":
      await commandFundProvider(provider, options);
      break;
    case "generate-real-payload":
      await commandGenerateRealPayload(options);
      break;
    case "populate":
      await commandPopulate(program, provider, options);
      break;
    case "populate-real":
      await commandPopulateReal(program, provider, options);
      break;
    default:
      fail(`Unknown command: ${command}`);
  }
}

main().catch((error) => {
  const message = error instanceof Error ? error.stack || error.message : String(error);
  fail(message);
});
