// Read the dev order_handler order_config + jobs straight from devnet, using the
// upgraded program id (CUj7KeoY) and the algorithm_id-patched IDL.
const anchor = require("@coral-xyz/anchor");
const fs = require("fs");
const path = require("path");
const { PublicKey, Keypair, Connection } = anchor.web3;

const DEVNET = "https://api.devnet.solana.com";
const PROGRAM_ID = new PublicKey("CUj7KeoY8cX8N5FiCsX3iVaYHKvFaJCdB8rhifpYmDzG");
const ORDER_CONFIG_SEED = Buffer.from("order_config");
const JOB_SEED = Buffer.from("job");

const idl = require("../target/idl/order_handler.json");
idl.address = PROGRAM_ID.toBase58();

function jobPda(jobId) {
  return PublicKey.findProgramAddressSync(
    [JOB_SEED, new anchor.BN(jobId).toArrayLike(Buffer, "le", 8)],
    PROGRAM_ID
  )[0];
}
function statusName(s) { return s && typeof s === "object" ? Object.keys(s)[0] : String(s); }
function bnToStr(v) { return v && v.toString ? v.toString() : v; }

(async () => {
  const conn = new Connection(DEVNET, "confirmed");
  const wallet = new anchor.Wallet(Keypair.generate()); // read-only
  const provider = new anchor.AnchorProvider(conn, wallet, { commitment: "confirmed" });
  const program = new anchor.Program(idl, provider);

  const [orderConfig] = PublicKey.findProgramAddressSync([ORDER_CONFIG_SEED], PROGRAM_ID);
  const cfg = await program.account.orderConfig.fetchNullable(orderConfig);
  if (!cfg) { console.log("order_config NOT initialized on devnet for", PROGRAM_ID.toBase58()); return; }
  const nextJobId = Number(cfg.nextJobId.toString());
  console.log(JSON.stringify({
    programId: PROGRAM_ID.toBase58(),
    orderConfig: orderConfig.toBase58(),
    roflAuthority: new PublicKey(cfg.roflAuthority).toBase58(),
    nextJobId,
  }, null, 2));

  for (let id = Math.max(1, nextJobId - 5); id < nextJobId; id++) {
    const job = await program.account.job.fetchNullable(jobPda(id));
    if (!job) { console.log(`job ${id}: (none)`); continue; }
    console.log(JSON.stringify({
      jobId: id,
      status: statusName(job.status),
      algorithmId: job.algorithmId,
      dataTypes: job.dataTypes,
      maxParticipants: job.maxParticipants,
      selectedParticipants: job.selectedParticipants.length,
      finalTotal: bnToStr(job.finalTotal),
      resultCid: job.resultCid,
      researcher: new PublicKey(job.researcher).toBase58(),
    }, null, 2));
  }
})().catch((e) => { console.error("read failed:", e.message); process.exit(1); });
