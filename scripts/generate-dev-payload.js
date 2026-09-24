// Generates a rich TEMPLATE for `data-registry-cli.js populate-real` for the
// DEV marketplace. The CLI will turn this template into fresh encrypted IPFS
// blobs by calling the TEE-side generator, so this file contains participant
// owners, demographics, data types, and day windows — not stale raw CIDs.
//
// The frontend groups datasets by serviceProvider (one card per provider, with
// N participants + aggregated demographic breakdowns). So we emit several
// branded providers, each with multiple varied-demographic participants, each
// with multiple units.
const fs = require("fs");
const path = require("path");
const anchor = require("@coral-xyz/anchor");
const { Keypair } = anchor.web3;

const DAY = 86400;
const BASE = 1780000000; // recent-ish day windows

// One distinct wallet PER PARTICIPANT. A provider card shows N participants;
// each must be a separate on-chain owner so cohort algorithms that group by
// owner (e.g. cohort_cosinor) count them as N distinct people. Using one wallet
// per provider (the old behaviour) collapsed every provider to 1 participant.
// Keypair files are created on first use and reused thereafter, so re-running
// the generator is stable and re-seeding maps to the same owners.
function participantKeypairPath(serviceProvider, participantIndex) {
  const slug = serviceProvider.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  return `./.anchor/participants/${slug}-p${participantIndex}.json`;
}

function ensureKeypairFile(relPath) {
  const abs = path.resolve(__dirname, "..", relPath.replace(/^\.\//, ""));
  if (!fs.existsSync(abs)) {
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, JSON.stringify(Array.from(Keypair.generate().secretKey)));
  }
  return relPath;
}

// Each brand themed to one provider profile. `unitsPerParticipant` controls how
// many daily upload units each participant will get when the template is turned
// into real raw CIDs by `populate-real`.
const BRANDS = [
  {
    serviceProvider: "Fitbit", deviceType: "Smartwatch", deviceModel: "Fitbit Charge 6",
    dataTypes: ["heart_rate", "sleep", "steps"],
    unitsPerParticipant: 2,
  },
  {
    serviceProvider: "Dexcom", deviceType: "CGM + Fitness Band", deviceModel: "Dexcom G7 / Fitbit",
    dataTypes: ["heart_rate", "glucose", "steps", "calories_burned"],
    unitsPerParticipant: 3,
  },
  {
    serviceProvider: "Omron", deviceType: "Home Monitor", deviceModel: "Omron Complete",
    dataTypes: ["heart_rate", "blood_pressure", "spo2"],
    unitsPerParticipant: 2,
  },
  {
    serviceProvider: "BioPatch", deviceType: "Clinical Wearable", deviceModel: "BioPatch CX",
    dataTypes: ["heart_rate", "ecg", "respiration_rate", "temperature"],
    unitsPerParticipant: 2,
  },
  {
    serviceProvider: "Oura", deviceType: "Smart Ring", deviceModel: "Oura Ring Gen 3",
    dataTypes: ["heart_rate", "sleep", "hydration", "stress"],
    unitsPerParticipant: 2,
  },
];

// gender 1=male 2=female; region 1-6; activity 1-5; smoker 0=no 1=yes 2=former; diet 1-7.
const PEOPLE = [
  { age: 27, gender: 2, height: 168, weight: 60, region: 1, physicalActivityLevel: 3, smoker: 0, diet: 2, chronicConditions: [] },
  { age: 34, gender: 1, height: 178, weight: 82, region: 2, physicalActivityLevel: 2, smoker: 2, diet: 1, chronicConditions: [1] },
  { age: 45, gender: 2, height: 165, weight: 71, region: 3, physicalActivityLevel: 1, smoker: 0, diet: 4, chronicConditions: [1, 4] },
  { age: 52, gender: 1, height: 182, weight: 90, region: 1, physicalActivityLevel: 2, smoker: 1, diet: 1, chronicConditions: [2, 5] },
  { age: 23, gender: 2, height: 170, weight: 58, region: 4, physicalActivityLevel: 4, smoker: 0, diet: 3, chronicConditions: [] },
  { age: 61, gender: 1, height: 175, weight: 85, region: 2, physicalActivityLevel: 1, smoker: 2, diet: 5, chronicConditions: [1, 2] },
  { age: 39, gender: 2, height: 172, weight: 66, region: 5, physicalActivityLevel: 3, smoker: 0, diet: 2, chronicConditions: [4] },
  { age: 30, gender: 1, height: 180, weight: 78, region: 6, physicalActivityLevel: 4, smoker: 0, diet: 6, chronicConditions: [] },
  { age: 48, gender: 2, height: 160, weight: 74, region: 1, physicalActivityLevel: 2, smoker: 1, diet: 1, chronicConditions: [1, 5] },
];

// Optional argv: "Brand:count ..." to emit a subset (e.g. a remainder after a
// partial run), and "--out <path>". Defaults to all brands x3 ->
// dev-populate.template.json.
const argv = process.argv.slice(2);
let outName = "dev-populate.template.json";
const pick = {};
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === "--out") { outName = argv[++i]; continue; }
  const [b, c] = argv[i].split(":");
  pick[b] = Number(c || 3);
}
const DEFAULT_PER_BRAND = 3;
const selected = Object.keys(pick).length
  ? BRANDS.filter((b) => pick[b.serviceProvider]).map((b) => ({ brand: b, count: pick[b.serviceProvider] }))
  : BRANDS.map((b) => ({ brand: b, count: DEFAULT_PER_BRAND }));

const entries = [];
let unitClock = 100; // offset so remainder windows don't collide with the first run

selected.forEach(({ brand, count }, bi) => {
  for (let e = 0; e < count; e++) {
    const person = PEOPLE[(bi * 3 + e) % PEOPLE.length];
    const u0s = BASE + unitClock++ * DAY;
    const appended = Array.from({ length: Math.max(0, (brand.unitsPerParticipant || 1) - 1) }, () => {
      const s = BASE + unitClock++ * DAY;
      return { dayStartTimestamp: s, dayEndTimestamp: s + DAY };
    });
    entries.push({
      ownerKeypair: ensureKeypairFile(participantKeypairPath(brand.serviceProvider, e)),
      dataTypes: brand.dataTypes,
      deviceType: brand.deviceType,
      deviceModel: brand.deviceModel,
      serviceProvider: brand.serviceProvider,
      dayStartTimestamp: u0s,
      dayEndTimestamp: u0s + DAY,
      // Personal attributes are OFF-chain: `populate --backend-url` registers
      // this profile with the backend (signed by ownerKeypair) and uploads
      // only the returned commitment. Age is derived by the backend from the
      // date of birth, so the persona's age is turned into a DOB here.
      profile: {
        dateOfBirth: `${2026 - person.age}-06-15`,
        gender: person.gender, height: person.height, weight: person.weight,
        region: person.region, physicalActivityLevel: person.physicalActivityLevel,
        smoker: person.smoker, diet: person.diet, chronicConditions: person.chronicConditions,
      },
      appendedUploads: appended,
    });
  }
});

const out = { teeAuthority: null, entries };
const outPath = path.resolve(__dirname, "..", "docs", outName);
fs.writeFileSync(outPath, JSON.stringify(out, null, 2));
console.log(`wrote ${entries.length} entries (${selected.length} providers) -> ${outPath}`);
console.log(`total units: ${entries.reduce((n, e) => n + 1 + e.appendedUploads.length, 0)}`);
