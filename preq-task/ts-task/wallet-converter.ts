// src/wallet-converter.ts
import bs58 from 'bs58';
import { createKeyPairSignerFromBytes } from "@solana/kit";

export function base58ToWalletBytes(base58PrivateKey: string): Uint8Array {
  try {
    const walletBytes = bs58.decode(base58PrivateKey);
    return new Uint8Array(walletBytes);
  } catch (error) {
    throw new Error(`Invalid base58 private key: ${error}`);
  }
}

export function walletBytesToBase58(walletBytes: Uint8Array): string {
  return bs58.encode(walletBytes);
}

export async function importFromPhantom(base58PrivateKey: string) {
  const walletBytes = base58ToWalletBytes(base58PrivateKey);
  const signer = await createKeyPairSignerFromBytes(walletBytes);
  
  console.log(`Imported wallet address: ${signer.address}`);
  console.log(`Wallet bytes: [${Array.from(walletBytes).join(',')}]`);
  
  return signer;
}

export function exportToPhantom(walletBytes: Uint8Array): string {
  const base58Key = walletBytesToBase58(walletBytes);
  console.log(`Base58 private key for Phantom: ${base58Key}`);
  return base58Key;
}

export async function demonstrateConversion() {
  console.log("=== Converting Wallet Bytes to Base58 (for Phantom) ===");

  console.log("\n=== Converting Base58 back to Wallet Bytes ===");
  const recoveredBytes = base58ToWalletBytes("5jHYHxoHK649JGGRasVYnAo5pPqZfLVKgq6keVmGcsAF316bzLmEE1KWbr91B5pNTsv3ah26heggnUDEeRwWocDr");
  console.log(`Recovered bytes: [${Array.from(recoveredBytes).join(',')}]`);
}

// Run demonstration if this file is executed directly
if (require.main === module) {
  demonstrateConversion().catch(console.error);
}