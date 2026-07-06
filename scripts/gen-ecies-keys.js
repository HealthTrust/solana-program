// Generate (or reuse) a researcher secp256k1 keypair for ECIES result
// encryption — the CLI-test stand-in for the frontend's Privy-wallet-derived
// key. The public key goes on-chain as result_encryption_key in the 0x-hex
// uncompressed form the TEE's encryptJobResult detects as ECIES.
// Writes: .anchor/researcher-ecies-private.hex, .anchor/researcher-ecies-public.hex
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const dir = path.resolve(__dirname, "..", ".anchor");
const privPath = path.join(dir, "researcher-ecies-private.hex");
const pubPath = path.join(dir, "researcher-ecies-public.hex");

let privHex;
if (fs.existsSync(privPath)) {
  privHex = fs.readFileSync(privPath, "utf8").trim();
} else {
  const ecdh = crypto.createECDH("secp256k1");
  ecdh.generateKeys();
  privHex = ecdh.getPrivateKey("hex").padStart(64, "0");
  fs.writeFileSync(privPath, privHex);
}

const ecdh = crypto.createECDH("secp256k1");
ecdh.setPrivateKey(Buffer.from(privHex, "hex"));
const pubHex = "0x" + ecdh.getPublicKey("hex", "uncompressed");
fs.writeFileSync(pubPath, pubHex);

console.log(JSON.stringify({ eciesPublicKey: pubHex, length: pubHex.length }, null, 2));
