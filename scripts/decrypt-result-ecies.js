// Fetch an encrypted job result from IPFS and decrypt the v2 ECIES envelope
// with the researcher's secp256k1 private key (.anchor/researcher-ecies-private.hex).
// Falls back to explaining v1 envelopes so mixed-era results are diagnosable.
// Usage: node scripts/decrypt-result-ecies.js <resultCid>
// Requires eciesjs; resolved from MVP-Frontend/node_modules if not local.
const fs = require("fs");
const path = require("path");

let eciesjs;
try {
  eciesjs = require("eciesjs");
} catch {
  eciesjs = require(path.resolve(__dirname, "..", "..", "MVP-Frontend", "node_modules", "eciesjs"));
}

const cid = process.argv[2];
if (!cid) {
  console.error("usage: node scripts/decrypt-result-ecies.js <resultCid>");
  process.exit(1);
}

const privHex = fs
  .readFileSync(path.resolve(__dirname, "..", ".anchor", "researcher-ecies-private.hex"), "utf8")
  .trim();

(async () => {
  const res = await fetch(`https://gateway.pinata.cloud/ipfs/${cid}`);
  if (!res.ok) throw new Error(`IPFS fetch failed: ${res.status}`);
  const body = (await res.text()).trim();

  let envelope;
  try {
    envelope = JSON.parse(body);
  } catch {
    throw new Error("payload is not JSON — looks like a legacy v1 raw-RSA result; use decrypt-result.js");
  }
  if (envelope.v !== 2) throw new Error(`unexpected envelope version: ${envelope.v}`);
  console.log("envelope: v=" + envelope.v + " alg=" + envelope.alg);

  const plaintext = eciesjs.decrypt(privHex, Buffer.from(envelope.ct, "base64"));
  console.log("decrypted result:");
  console.log(JSON.stringify(JSON.parse(plaintext.toString("utf8")), null, 2));
})().catch((e) => {
  console.error("decrypt failed:", e.message);
  process.exit(1);
});
