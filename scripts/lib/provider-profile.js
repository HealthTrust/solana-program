// Off-chain provider profile helpers for the seeding scripts.
//
// Personal attributes no longer go on-chain (MVP/OFFCHAIN_ATTRIBUTES_DESIGN.md).
// A provider writes them ONCE to the backend through the wallet-signed
// `PUT /profile`, which returns the 32-byte salted commitment that every
// subsequent `upload_new_meta` carries as `profile_commit`.
//
// This module mirrors MVP-Server-Backend/src/profile/{commitment,auth}.ts
// byte-for-byte (canonical layout schema 1, message prefix v1) so the seeding
// scripts can register realistic profiles and obtain real commitments.
// Signing uses Node's built-in Ed25519 (no extra dependency): a Solana
// keypair's 64-byte secretKey is seed(32) || pubkey(32).

const crypto = require("crypto");
// bs58 ≥5 ships ESM with a default export; older versions are plain CJS.
const bs58 = ((m) => (m && m.default) || m)(require("bs58"));

const CANONICAL_SCHEMA_VERSION = 1;
const PROFILE_AUTH_PREFIX = "healthtrust/provider-profile/v1";
const U8_FIELDS = ["gender", "height", "weight", "region", "physicalActivityLevel", "smoker", "diet"];
const ZERO_COMMIT = Buffer.alloc(32);

function dobToU32(dateOfBirth) {
  if (!dateOfBirth) return 0;
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateOfBirth);
  if (!m) throw new Error(`dateOfBirth must be YYYY-MM-DD, got ${dateOfBirth}`);
  return Number(m[1]) * 10_000 + Number(m[2]) * 100 + Number(m[3]);
}

function canonicalProfileBytes(profile) {
  const conditions = [...new Set(profile.chronicConditions || [])].sort((a, b) => a - b);
  const buf = Buffer.alloc(1 + 4 + U8_FIELDS.length + 1 + conditions.length);
  let o = 0;
  buf.writeUInt8(CANONICAL_SCHEMA_VERSION, o); o += 1;
  buf.writeUInt32LE(dobToU32(profile.dateOfBirth), o); o += 4;
  for (const f of U8_FIELDS) { buf.writeUInt8(profile[f] || 0, o); o += 1; }
  buf.writeUInt8(conditions.length, o); o += 1;
  for (const c of conditions) { buf.writeUInt8(c, o); o += 1; }
  return buf;
}

function profileDigestHex(profile) {
  return crypto.createHash("sha256").update(canonicalProfileBytes(profile)).digest("hex");
}

function buildProfileAuthMessage(action, owner, unixSeconds, profileDigest) {
  const parts = [PROFILE_AUTH_PREFIX, action, owner, String(unixSeconds)];
  if (action === "put") parts.push(profileDigest);
  return parts.join("|");
}

/** base58 ed25519 signature over the UTF-8 message, from a Solana Keypair. */
function signMessage(keypair, message) {
  const secret = Buffer.from(keypair.secretKey);
  const seed = secret.subarray(0, 32);
  const pub = secret.subarray(32, 64);
  const key = crypto.createPrivateKey({
    key: { kty: "OKP", crv: "Ed25519", d: seed.toString("base64url"), x: pub.toString("base64url") },
    format: "jwk",
  });
  return bs58.encode(crypto.sign(null, Buffer.from(message, "utf8"), key));
}

/**
 * Register (or refresh) a provider's profile with the backend and return the
 * commitment as a 32-byte Buffer. Idempotent: an unchanged profile returns the
 * existing version's commitment.
 */
async function registerProfile(backendUrl, keypair, profile) {
  const owner = keypair.publicKey.toBase58();
  const unixSeconds = Math.floor(Date.now() / 1000);
  const message = buildProfileAuthMessage("put", owner, unixSeconds, profileDigestHex(profile));
  const res = await fetch(`${backendUrl.replace(/\/$/, "")}/profile`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ owner, profile, auth: { message, signature: signMessage(keypair, message) } }),
  });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(`PUT /profile for ${owner} failed: HTTP ${res.status} ${body.error || ""}`);
  }
  if (typeof body.commitment !== "string" || body.commitment.length !== 64) {
    throw new Error(`PUT /profile for ${owner} returned no commitment`);
  }
  return Buffer.from(body.commitment, "hex");
}

/** Parse a hex commitment from a payload/CLI value; missing → all-zero. */
function parseCommitment(value) {
  if (value === undefined || value === null || value === "") return ZERO_COMMIT;
  if (Buffer.isBuffer(value)) return value;
  if (Array.isArray(value)) return Buffer.from(value);
  const hex = String(value).replace(/^0x/, "");
  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new Error(`profileCommit must be 32 bytes hex, got ${value}`);
  }
  return Buffer.from(hex, "hex");
}

/** Anchor expects `[u8; 32]` as a plain number array. */
function commitmentArg(buffer) {
  return Array.from(parseCommitment(buffer));
}

/**
 * Resolve the `profile_commit` for a populate entry: if a backend URL is given
 * and the entry has a `profile`, register it (signed by the entry's own
 * keypair) and use the returned commitment; otherwise use `entry.profileCommit`
 * (hex) or zeros. `cache` dedupes per owner+digest across entries.
 */
async function resolveEntryCommitment(entry, keypair, backendUrl, cache = new Map()) {
  if (backendUrl && entry.profile && keypair) {
    const cacheKey = `${keypair.publicKey.toBase58()}:${profileDigestHex(entry.profile)}`;
    if (!cache.has(cacheKey)) {
      cache.set(cacheKey, await registerProfile(backendUrl, keypair, entry.profile));
    }
    return cache.get(cacheKey);
  }
  return parseCommitment(entry.profileCommit);
}

module.exports = {
  CANONICAL_SCHEMA_VERSION,
  PROFILE_AUTH_PREFIX,
  canonicalProfileBytes,
  profileDigestHex,
  buildProfileAuthMessage,
  signMessage,
  registerProfile,
  parseCommitment,
  commitmentArg,
  resolveEntryCommitment,
};
