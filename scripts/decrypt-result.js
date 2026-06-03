// Fetch an encrypted job result from IPFS and decrypt it with the researcher's
// RSA private key — proving the envelope actually reaches the researcher.
// Usage: node scripts/decrypt-result.js <resultCid>
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");
const https = require("https");

const cid = process.argv[2];
if (!cid) {
  console.error("usage: node scripts/decrypt-result.js <resultCid>");
  process.exit(1);
}

const pem = fs.readFileSync(path.resolve(__dirname, "..", ".anchor", "researcher-rsa-private.pem"), "utf8");

function fetch(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      let d = "";
      res.on("data", (c) => (d += c));
      res.on("end", () => resolve(d));
    }).on("error", reject);
  });
}

(async () => {
  const ciphertextB64 = (await fetch(`https://gateway.pinata.cloud/ipfs/${cid}`)).trim();
  const plaintext = crypto.privateDecrypt(
    { key: pem, padding: crypto.constants.RSA_PKCS1_OAEP_PADDING, oaepHash: "sha256" },
    Buffer.from(ciphertextB64, "base64")
  );
  console.log("decrypted plaintext:");
  console.log(plaintext.toString("utf8"));
})().catch((e) => {
  console.error("decrypt failed:", e.message);
  process.exit(1);
});
