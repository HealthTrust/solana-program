// Generates a rich data-registry populate payload for the DEV marketplace.
// The frontend groups datasets by serviceProvider (one card per provider, with
// N participants + aggregated demographic breakdowns). So we emit several
// branded providers, each with multiple varied-demographic participants, each
// with multiple units. rawCids reuse the 5 known-good pinned blobs (real,
// TEE-decryptable); dataTypes per provider match each blob's real content.
const fs = require("fs");
const path = require("path");

const DAY = 86400;
const BASE = 1780000000; // recent-ish day windows

// Each brand themed to one real data profile (CIDs + their true dataTypes).
const BRANDS = [
  {
    serviceProvider: "Fitbit", deviceType: "Smartwatch", deviceModel: "Fitbit Charge 6",
    owner: "./.anchor/provider1.json", dataTypes: ["heart_rate", "sleep", "steps"],
    cids: [
      "bafkreid6wjuqbscmktylhwtc64fmcb4odnkfd4gky2nmx2xzmzifvspirq",
      "bafkreiayqacbdkbunq7qj67wk5jevcaktt4u42znfl4xfispmwsm6dbsky",
    ],
  },
  {
    serviceProvider: "Dexcom", deviceType: "CGM + Fitness Band", deviceModel: "Dexcom G7 / Fitbit",
    owner: "./.anchor/provider2.json", dataTypes: ["heart_rate", "glucose", "steps", "calories_burned"],
    cids: [
      "bafkreicsomjllan4zbz6d4e4pulfgeu2odpx3jj4tji5s6nkha6h7zwnzm",
      "bafkreiff3ndd7cxgpxiwdtjscmrf4ikidkj4dxdl55lo3rftnf7hbl6554",
      "bafkreige7gs3h23xtfbzgtapjw3aanuxtchsc3utjifubydsvcnpi3odhu",
    ],
  },
  {
    serviceProvider: "Omron", deviceType: "Home Monitor", deviceModel: "Omron Complete",
    owner: "./.anchor/provider3.json", dataTypes: ["heart_rate", "blood_pressure", "spo2"],
    cids: [
      "bafkreifyi5qetgzrtsohmsvnuf6kbh5t6f2totvkwl6eoo3fk3smynazuq",
      "bafkreiam7whyeqhhvz5ckw3nqzrwzlli7h5fghrkandt6ok6bzpezk33ve",
    ],
  },
  {
    serviceProvider: "BioPatch", deviceType: "Clinical Wearable", deviceModel: "BioPatch CX",
    owner: "./.anchor/provider4.json", dataTypes: ["heart_rate", "ecg", "respiration_rate", "temperature"],
    cids: [
      "bafkreiflgrg226zgy5eefoajdxlojiw7zoykx5yyeibos7n2k7fvi566xi",
      "bafkreidptdvdd4wqb7zv4eeuqjso4mxp4m37tvwypp6eigc4n3udxpzuni",
    ],
  },
  {
    serviceProvider: "Oura", deviceType: "Smart Ring", deviceModel: "Oura Ring Gen 3",
    owner: "./.anchor/provider5.json", dataTypes: ["heart_rate", "sleep", "hydration", "stress"],
    cids: [
      "bafkreiewxmjznoaw5iir2zm63t6fnfofiucon6wvexq5xy4b5aas75pp54",
      "bafkreigvpnw4z6frxuddd4kqlkibpgac5zs2b3oev5gopotmoz3mn4kbqy",
    ],
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
// partial run), and "--out <path>". Defaults to all brands x3 -> dev-populate.generated.json.
const argv = process.argv.slice(2);
let outName = "dev-populate.generated.json";
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
    const appended = brand.cids.slice(1).map((cid) => {
      const s = BASE + unitClock++ * DAY;
      return { rawCid: cid, dayStartTimestamp: s, dayEndTimestamp: s + DAY };
    });
    entries.push({
      ownerKeypair: brand.owner,
      rawCid: brand.cids[0],
      dataTypes: brand.dataTypes,
      deviceType: brand.deviceType,
      deviceModel: brand.deviceModel,
      serviceProvider: brand.serviceProvider,
      dayStartTimestamp: u0s,
      dayEndTimestamp: u0s + DAY,
      age: person.age, gender: person.gender, height: person.height, weight: person.weight,
      region: person.region, physicalActivityLevel: person.physicalActivityLevel,
      smoker: person.smoker, diet: person.diet, chronicConditions: person.chronicConditions,
      appendedUploads: appended,
    });
  }
});

const out = { teeAuthority: null, entries };
const outPath = path.resolve(__dirname, "..", "docs", outName);
fs.writeFileSync(outPath, JSON.stringify(out, null, 2));
console.log(`wrote ${entries.length} entries (${selected.length} providers) -> ${outPath}`);
console.log(`total units: ${entries.reduce((n, e) => n + 1 + e.appendedUploads.length, 0)}`);
