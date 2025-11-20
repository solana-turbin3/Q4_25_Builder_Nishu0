#!/bin/bash

# Deployment script for Perpetuals Program to Devnet

set -e

echo "================================================"
echo "Perpetuals Program Deployment Script"
echo "================================================"
echo ""

# Configuration
PROGRAM_KEYPAIR="target/deploy/perpetuals-keypair.json"
IDL_PATH="target/idl/perpetuals.json"
CLUSTER="devnet"

# Get program ID
PROGRAM_ID=$(solana address -k $PROGRAM_KEYPAIR)
echo "Program ID: $PROGRAM_ID"
echo ""

# Check balance
BALANCE=$(solana balance | awk '{print $1}')
echo "Current Balance: $BALANCE SOL"
echo ""

# Check if we have enough SOL (at least 5 SOL)
if (( $(echo "$BALANCE < 5" | bc -l) )); then
    echo "⚠️  WARNING: You may not have enough SOL for deployment"
    echo "   Recommended: At least 5-6 SOL"
    echo "   Get more SOL from: https://faucet.solana.com/"
    echo ""
    read -p "Continue anyway? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Deployment cancelled"
        exit 1
    fi
fi

echo "Step 1: Building program..."
anchor build
echo "✅ Build complete"
echo ""

echo "Step 2: Deploying program to $CLUSTER..."
anchor deploy --provider.cluster $CLUSTER --program-keypair $PROGRAM_KEYPAIR
echo "✅ Program deployed"
echo ""

echo "Step 3: Initializing IDL..."
anchor idl init --provider.cluster $CLUSTER --filepath $IDL_PATH $PROGRAM_ID
echo "✅ IDL initialized"
echo ""

echo "================================================"
echo "✅ Deployment Complete!"
echo "================================================"
echo ""
echo "Program ID: $PROGRAM_ID"
echo "Cluster: $CLUSTER"
echo ""
echo "Next Steps:"
echo "1. Initialize the program:"
echo "   cd app"
echo "   npx ts-node src/cli.ts -k ~/.config/solana/id.json init --min-signatures 1 $WALLET_PUBKEY"
echo ""
echo "2. Add a pool:"
echo "   npx ts-node src/cli.ts -k ~/.config/solana/id.json add-pool TestPool1"
echo ""
echo "3. Add custody (SOL example):"
echo "   npx ts-node src/cli.ts -k ~/.config/solana/id.json add-custody TestPool1 \\"
echo "     So11111111111111111111111111111111111111112 \\"
echo "     J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"
echo ""
