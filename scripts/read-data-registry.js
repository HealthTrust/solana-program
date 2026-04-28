const anchor = require("@coral-xyz/anchor");
const path = require("path");

const { PublicKey } = anchor.web3;
const idl = require("../target/idl/data_registry.json");

const REGISTRY_SEED = Buffer.from("registry_state");
const META_SEED = Buffer.from("meta");
const UNIT_SEED = Buffer.from("unit");
const DEFAULT_RPC_URL = "http://127.0.0.1:8899";
const DEFAULT_PROGRAM_ID = "3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT";

function fail(message) {
  console.error(`[read-data-registry] ERROR: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = {};

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];

    if (token === "--url") {
      options.url = argv[++index];
    } else if (token === "--wallet") {
      options.wallet = argv[++index];
    } else if (token === "--meta-ids") {
      options.metaIds = argv[++index];
    } else if (token === "--meta-from") {
      options.metaFrom = Number(argv[++index]);
    } else if (token === "--meta-to") {
      options.metaTo = Number(argv[++index]);
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
      "  node ./scripts/read-data-registry.js [--url <rpc-url>] [--wallet <keypair>] [--meta-ids 12,13,14] [--meta-from 12 --meta-to 16]",
      "",
      "Examples:",
      "  node ./scripts/read-data-registry.js --url https://api.devnet.solana.com --wallet ./.anchor/wsl-id.json --meta-from 12 --meta-to 16",
      "  node ./scripts/read-data-registry.js --meta-ids 12,16",
    ].join("\n")
  );
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

function normalizeWalletPath(inputPath) {
  return path.resolve(process.cwd(), expandHome(inputPath));
}

function createProgram(provider) {
  if (!idl.address) {
    idl.address = DEFAULT_PROGRAM_ID;
  }

  return new anchor.Program(idl, provider);
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

function normalizeMetaIds(options) {
  if (options.metaIds) {
    return options.metaIds
      .split(",")
      .map((value) => Number(value.trim()))
      .filter((value) => Number.isFinite(value));
  }

  if (Number.isFinite(options.metaFrom) && Number.isFinite(options.metaTo)) {
    const result = [];
    for (let value = options.metaFrom; value <= options.metaTo; value += 1) {
      result.push(value);
    }
    return result;
  }

  return [];
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

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printUsage();
    return;
  }

  process.env.ANCHOR_PROVIDER_URL = options.url || process.env.ANCHOR_PROVIDER_URL || DEFAULT_RPC_URL;
  process.env.ANCHOR_WALLET = normalizeWalletPath(
    options.wallet || process.env.ANCHOR_WALLET || "~/.config/solana/id.json"
  );

  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = createProgram(provider);
  const registryStateAddress = deriveRegistryStatePda(program.programId);
  const registryState = await program.account.registryState.fetch(registryStateAddress);
  const metaIds = normalizeMetaIds(options);

  const output = {
    rpcUrl: provider.connection.rpcEndpoint,
    wallet: provider.publicKey.toBase58(),
    registryState: {
      address: registryStateAddress.toBase58(),
      owner: new PublicKey(registryState.owner).toBase58(),
      pricingProgram: new PublicKey(registryState.pricingProgram).toBase58(),
      teeAuthority: new PublicKey(registryState.teeAuthority).toBase58(),
      nextMetaId: bnToString(registryState.nextMetaId),
      paused: registryState.paused,
    },
    metas: [],
  };

  for (const metaId of metaIds) {
    const meta = await readMeta(program, metaId);
    output.metas.push(meta || { metaId: String(metaId), exists: false });
  }

  console.log(JSON.stringify(output, null, 2));
}

main().catch((error) => {
  const message = error instanceof Error ? error.stack || error.message : String(error);
  fail(message);
});
