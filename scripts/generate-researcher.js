const { Keypair } = require('@solana/web3.js');
const fs = require('fs');
const path = require('path');

const keypair = Keypair.generate();
const secretKey = Array.from(keypair.secretKey);

const dir = path.join(__dirname, '..', '.anchor');
if (!fs.existsSync(dir)) {
  fs.mkdirSync(dir);
}

fs.writeFileSync(path.join(dir, 'researcher.json'), JSON.stringify(secretKey));

console.log('Researcher wallet created at .anchor/researcher.json');
console.log('Public key:', keypair.publicKey.toString());