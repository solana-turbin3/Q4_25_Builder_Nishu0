import { createKeyPairSignerFromBytes, createSolanaRpc, createSolanaRpcSubscriptions, devnet, airdropFactory, lamports } from "@solana/kit";
import wallet from "./dev-wallet.json";

const LAMPORTS_PER_SOL = BigInt(1_000_000_000);

async function performAirdrop() {
  // We're going to import our keypair from the wallet file
  const keypair = await createKeyPairSignerFromBytes(new Uint8Array(wallet));
  console.log(keypair.keyPair.privateKey);
  console.log(`Your Solana wallet address: ${keypair.address}`);
  
  // Create an rpc connection
  const rpc = createSolanaRpc(devnet("https://api.devnet.solana.com")); 
  const rpcSubscriptions = createSolanaRpcSubscriptions(devnet('ws://api.devnet.solana.com'));
  
  // Using the convenient airdropFactory from solana kit
  const airdrop = airdropFactory({ rpc, rpcSubscriptions });
  
  try {
    const sig = await airdrop({
      commitment: 'confirmed',
      recipientAddress: keypair.address,
      lamports: lamports(2n * LAMPORTS_PER_SOL),
    });
    console.log(`Success! Check out your TX here:
https://explorer.solana.com/tx/${sig}?cluster=devnet`);
  } catch (error) {
    console.error(`Oops, something went wrong: ${error}`);
  }
}

// Execute the async function
performAirdrop().catch(console.error);