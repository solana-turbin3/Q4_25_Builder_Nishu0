import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Perpetuals } from "../target/types/perpetuals";
import { PublicKey } from "@solana/web3.js";

describe("perpetuals", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.perpetuals as Program<Perpetuals>;

  it("Initializes the pool", async () => {
    // Create a mock oracle address (in production, this would be a real oracle)
    const mockOracle = anchor.web3.Keypair.generate().publicKey;

    // Derive the pool PDA
    const [poolPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool")],
      program.programId
    );

    // Initialize the pool
    const tx = await program.methods
      .initialize(mockOracle)
      .accounts({
        pool: poolPda,
        authority: anchor.getProvider().wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Pool initialized! Transaction signature:", tx);
    console.log("Pool PDA:", poolPda.toString());

    // Fetch and verify the pool account
    const poolAccount = await program.account.pool.fetch(poolPda);
    console.log("Pool authority:", poolAccount.authority.toString());
    console.log("Pool oracle:", poolAccount.oracle.toString());
    console.log("Total collateral:", poolAccount.totalCollateral.toString());
  });
});
