import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { expect } from "chai";
import { Healthtrust } from "../target/types/healthtrust";

describe("healthtrust", () => {
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.healthtrust as Program<Healthtrust>;
  const provider = anchor.getProvider() as anchor.AnchorProvider;

  it("initialize succeeds and lands on-chain", async () => {
    const sig = await program.methods.initialize().rpc();

    const fetched = await provider.connection.getTransaction(sig, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });

    expect(fetched).to.not.equal(null);
    expect(fetched!.meta?.err).to.equal(null);
  });
});
