# Perpetuals Program Deployment Guide

## Current Status ✅

- **Network:** Devnet
- **Wallet Address:** `4WgniDcyQtaf9JxsAmT5a4Ct9CzQ4da2pLJZCdEoSAqc`
- **Current Balance:** 1 SOL
- **Program ID:** `GxegSBD3PQFQjzYui524RWraUM7SzQsBZpWnwBbpLQbk`
- **Program Size:** 1.1MB

## What You Need

### SOL Required:
- **Deployment:** ~4-5 SOL (for program rent)
- **IDL Initialization:** ~0.1-0.5 SOL
- **Transaction Fees:** ~0.1-0.5 SOL
- **Total Recommended:** 5-6 SOL

### Get More Devnet SOL:

1. **Web Faucets (Recommended - No Rate Limits):**
   - https://faucet.solana.com/
   - https://faucet.quicknode.com/solana/devnet
   - Enter your wallet: `4WgniDcyQtaf9JxsAmT5a4Ct9CzQ4da2pLJZCdEoSAqc`

2. **CLI Faucet (Rate Limited):**
   ```bash
   solana airdrop 1
   ```

## Deployment Steps

### Option 1: Use the Automated Script

```bash
cd /Users/nisargthakkar/Turbine/power_perpetuals_contract
./deploy.sh
```

### Option 2: Manual Deployment

1. **Check your balance:**
   ```bash
   solana balance
   ```

2. **Build the program:**
   ```bash
   anchor build
   ```

3. **Deploy to devnet:**
   ```bash
   anchor deploy --provider.cluster devnet \
     --program-keypair target/deploy/perpetuals-keypair.json
   ```

4. **Initialize IDL:**
   ```bash
   anchor idl init --provider.cluster devnet \
     --filepath ./target/idl/perpetuals.json \
     GxegSBD3PQFQjzYui524RWraUM7SzQsBZpWnwBbpLQbk
   ```

## Post-Deployment: Initialize the Program

### 1. Install Dependencies (if not already done):
```bash
cd app
pnpm install
```

### 2. Initialize the Program:
```bash
npx ts-node src/cli.ts \
  -k ~/.config/solana/id.json \
  init \
  --min-signatures 1 \
  4WgniDcyQtaf9JxsAmT5a4Ct9CzQ4da2pLJZCdEoSAqc
```

### 3. Verify Initialization:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-multisig
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-perpetuals
```

### 4. Create a Pool:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json add-pool TestPool1
```

### 5. Add Token Custody (SOL Example):

**For SOL:**
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json add-custody \
  TestPool1 \
  So11111111111111111111111111111111111111112 \
  J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix
```

**For USDC (Devnet):**
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json add-custody -s \
  TestPool1 \
  4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU \
  5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7
```

**Pyth Oracle Accounts (Devnet):**
- SOL/USD: `J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix`
- USDC/USD: `5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7`
- BTC/USD: `HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J`
- ETH/USD: `EdVCmQ9FSPcVe5YySXDPCRmc8aDQLKJ9xvYBMZPie1Vw`

Find more at: https://pyth.network/developers/price-feed-ids#solana-devnet

### 6. Verify Pool and Custody:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-pool TestPool1
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-custodies TestPool1
```

## Adding Liquidity

### 1. Get LP Token Mint Address:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-lp-token-mint TestPool1
```

### 2. Create LP Token Account:
```bash
spl-token create-account <LP_TOKEN_MINT> \
  --owner 4WgniDcyQtaf9JxsAmT5a4Ct9CzQ4da2pLJZCdEoSAqc \
  --fee-payer ~/.config/solana/id.json
```

### 3. Add Liquidity:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json add-liquidity \
  TestPool1 \
  So11111111111111111111111111111111111111112 \
  --amount-in 1000000000 \
  --min-amount-out 0
```

## Useful Commands

### Check Balance:
```bash
solana balance
```

### Get All Pools:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-pools
```

### Get Oracle Price:
```bash
npx ts-node src/cli.ts -k ~/.config/solana/id.json get-oracle-price \
  TestPool1 \
  So11111111111111111111111111111111111111112
```

### View All CLI Commands:
```bash
npx ts-node src/cli.ts --help
```

## Important Notes

1. **Devnet vs Mainnet:**
   - Currently configured for devnet
   - To use mainnet, add `-u https://api.mainnet-beta.solana.com` to all commands

2. **Wallet Security:**
   - Seed phrase saved at wallet creation
   - Keypair location: `~/.config/solana/id.json`
   - **DO NOT use this wallet for mainnet with real funds**

3. **Program Authority:**
   - Current upgrade authority: Your wallet (`4WgniDcyQtaf9JxsAmT5a4Ct9CzQ4da2pLJZCdEoSAqc`)
   - Change with: `solana program set-upgrade-authority`

4. **Min Signatures:**
   - Set to 1 for single-admin setup
   - For multi-sig, increase and add more admin pubkeys

## Troubleshooting

### "Insufficient Funds" Error:
- Request more SOL from faucets
- Check balance: `solana balance`

### "Program already initialized" Error:
- IDL already exists, use `anchor idl upgrade` instead of `init`

### "Rate limit" Error on Faucet:
- Use web faucets instead
- Wait 1-2 minutes between CLI airdrop requests

### TypeScript Errors:
- Ensure dependencies are installed: `cd app && pnpm install`
- Rebuild program: `anchor build`

## Next Steps After Deployment

1. ✅ Deploy program
2. ✅ Initialize IDL
3. ✅ Initialize program with admin
4. ✅ Create pool
5. ✅ Add custodies for tokens
6. Add liquidity to pools
7. Test opening positions
8. Deploy UI/frontend (if applicable)

## Resources

- Solana Devnet Explorer: https://explorer.solana.com/?cluster=devnet
- Pyth Price Feeds: https://pyth.network/developers/price-feed-ids
- Anchor Docs: https://www.anchor-lang.com/
