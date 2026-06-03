const anchor = require("@coral-xyz/anchor");
const { PublicKey, Keypair, Connection } = anchor.web3;
const idl = require("../target/idl/data_registry.json");
const PROGRAM_ID = new PublicKey("Dp6VU1JPpAMhzrQfXtjxQFs2CMaMRX4f53sNm8M5TVU4");
idl.address = PROGRAM_ID.toBase58();

(async () => {
  const conn = new Connection("https://api.devnet.solana.com", "confirmed");
  const provider = new anchor.AnchorProvider(conn, new anchor.Wallet(Keypair.generate()), { commitment: "confirmed" });
  const program = new anchor.Program(idl, provider);
  const [registryState] = PublicKey.findProgramAddressSync([Buffer.from("registry_state")], PROGRAM_ID);
  const rs = await program.account.registryState.fetchNullable(registryState);
  if (!rs) { console.log("registry NOT initialized:", registryState.toBase58()); return; }
  console.log(JSON.stringify({
    registryState: registryState.toBase58(),
    owner: new PublicKey(rs.owner).toBase58(),
    teeAuthority: new PublicKey(rs.teeAuthority).toBase58(),
    paused: rs.paused,
    nextMetaId: rs.nextMetaId.toString(),
  }, null, 2));
})().catch((e) => { console.error("check failed:", e.message); process.exit(1); });
