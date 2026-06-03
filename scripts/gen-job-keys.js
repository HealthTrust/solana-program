// Generate a researcher Solana wallet + an RSA keypair for result encryption.
// Writes: .anchor/researcher.json (solana wallet),
//         .anchor/researcher-rsa-public.b64 (base64 SPKI, for --result-encryption-key),
//         .anchor/researcher-rsa-private.pem (to decrypt the result later if desired).
const { Keypair } = require("@solana/web3.js");
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const dir = path.resolve(__dirname, "..", ".anchor");
if (!fs.existsSync(dir)) fs.mkdirSync(dir);

const walletPath = path.join(dir, "researcher.json");
let wallet;
if (fs.existsSync(walletPath)) {
  wallet = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8"))));
} else {
  wallet = Keypair.generate();
  fs.writeFileSync(walletPath, JSON.stringify(Array.from(wallet.secretKey)));
}

const { publicKey, privateKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
const spkiDer = publicKey.export({ type: "spki", format: "der" });
const pubB64 = Buffer.from(spkiDer).toString("base64");
fs.writeFileSync(path.join(dir, "researcher-rsa-public.b64"), pubB64);
fs.writeFileSync(path.join(dir, "researcher-rsa-private.pem"), privateKey.export({ type: "pkcs8", format: "pem" }));

console.log(JSON.stringify({
  researcherWallet: wallet.publicKey.toBase58(),
  rsaPublicKeyB64: pubB64,
}, null, 2));
