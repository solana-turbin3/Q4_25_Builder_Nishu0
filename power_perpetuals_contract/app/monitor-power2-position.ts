/**
 * Monitor Power=2 Position
 * Opens a power=2 position and monitors PnL and liquidation status live
 */

import { PerpetualsClient } from "./src/client";
import { PublicKey } from "@solana/web3.js";
import { BN } from "@coral-xyz/anchor";

// Configuration
const CLUSTER_URL = "http://localhost:8899";
const ADMIN_KEY_PATH = process.env.HOME + "/.config/solana/id.json";
const POOL_NAME = "PowerPerpsPool";
const TOKEN_MINT = new PublicKey("So11111111111111111111111111111111111111112"); // SOL
const COLLATERAL_MINT = TOKEN_MINT; // Using SOL as collateral
const SIDE = "long" as const;
const POWER = 2; // Power=2 (Squared Perps)

// Position parameters
const COLLATERAL_SOL = 1.0; // 1 SOL collateral
const SIZE_SOL = 0.5; // 0.5 SOL position size
const SLIPPAGE_BPS = 100; // 1% slippage tolerance

// Monitor interval (milliseconds)
const MONITOR_INTERVAL = 3000; // 3 seconds

async function main() {
  console.log("╔════════════════════════════════════════════════════════════╗");
  console.log("║         Power=2 Position Monitor (Squared Perps)          ║");
  console.log("╚════════════════════════════════════════════════════════════╝");
  console.log();

  // Initialize client
  process.env["ANCHOR_WALLET"] = ADMIN_KEY_PATH;
  const client = new PerpetualsClient(CLUSTER_URL, ADMIN_KEY_PATH);
  const wallet = client.provider.publicKey;

  console.log(`📍 Wallet: ${wallet.toBase58()}`);
  console.log(`🏦 Pool: ${POOL_NAME}`);
  console.log(`💰 Token: SOL`);
  console.log();

  try {
    // Get current oracle price
    console.log("📈 Fetching current SOL price...");
    const oraclePrice = await client.getOraclePrice(POOL_NAME, TOKEN_MINT, false);
    const currentPrice = oraclePrice.toNumber() / 1e6;
    console.log(`   Current Price: $${currentPrice.toFixed(2)}`);
    console.log();

    // Calculate position parameters
    const maxPrice = new BN(Math.floor(oraclePrice.toNumber() * (1 + SLIPPAGE_BPS / 10000)));
    const collateral = new BN(Math.floor(COLLATERAL_SOL * 1e9)); // Convert to lamports
    const size = new BN(Math.floor(SIZE_SOL * 1e9)); // Convert to lamports

    const leverage = SIZE_SOL / COLLATERAL_SOL;
    const sizeUsd = SIZE_SOL * currentPrice;
    const collateralUsd = COLLATERAL_SOL * currentPrice;

    console.log("📝 Position Parameters:");
    console.log(`   Side: ${SIDE.toUpperCase()}`);
    console.log(`   Power: ${POWER} (Squared Perps)`);
    console.log(`   Collateral: ${COLLATERAL_SOL} SOL ($${collateralUsd.toFixed(2)})`);
    console.log(`   Size: ${SIZE_SOL} SOL ($${sizeUsd.toFixed(2)})`);
    console.log(`   Initial Leverage: ${leverage.toFixed(2)}x`);
    console.log(`   Max Entry Price: $${(maxPrice.toNumber() / 1e6).toFixed(2)}`);
    console.log();

    console.log("🔓 Opening Power=2 position...");
    console.log();

    // Open the position with power=2
    await client.openPosition(
      POOL_NAME,
      TOKEN_MINT,
      COLLATERAL_MINT,
      SIDE,
      maxPrice,
      collateral,
      size,
      POWER // Power parameter!
    );

    console.log("✅ Position opened successfully!");
    console.log();
    console.log("═".repeat(62));
    console.log("📊 LIVE MONITORING (Updates every 3 seconds)");
    console.log("═".repeat(62));
    console.log();

    // Monitor the position
    let iteration = 0;
    const startTime = Date.now();

    const monitorInterval = setInterval(async () => {
      try {
        iteration++;
        const elapsed = Math.floor((Date.now() - startTime) / 1000);

        // Fetch position data
        const position = await client.getUserPosition(
          wallet,
          POOL_NAME,
          TOKEN_MINT,
          SIDE
        );

        // Get current price
        const currentOraclePrice = await client.getOraclePrice(
          POOL_NAME,
          TOKEN_MINT,
          false
        );
        const currentMarketPrice = currentOraclePrice.toNumber() / 1e6;

        // Get PnL
        const pnl = await client.getPnl(
          wallet,
          POOL_NAME,
          TOKEN_MINT,
          COLLATERAL_MINT,
          SIDE
        );

        // Get liquidation info
        const liquidationPrice = await client.getLiquidationPrice(
          wallet,
          POOL_NAME,
          TOKEN_MINT,
          COLLATERAL_MINT,
          SIDE,
          new BN(0),
          new BN(0)
        );

        const liquidationState = await client.getLiquidationState(
          wallet,
          POOL_NAME,
          TOKEN_MINT,
          COLLATERAL_MINT,
          SIDE
        );

        // Calculate metrics
        const entryPrice = position.price.toNumber() / 1e6;
        const liqPrice = liquidationPrice.toNumber() / 1e6;
        const priceChange = ((currentMarketPrice - entryPrice) / entryPrice) * 100;

        const profit = pnl.profit.toNumber() / 1e6;
        const loss = pnl.loss.toNumber() / 1e6;
        const netPnl = profit - loss;
        const pnlPercent = (netPnl / collateralUsd) * 100;

        // Calculate distance to liquidation
        const distanceToLiq = ((currentMarketPrice - liqPrice) / currentMarketPrice) * 100;

        // Liquidation state labels
        const liqStateLabels = ["None", "CanBeLiquidated", "MustBeLiquidated"];
        const liqStateLabel = liqStateLabels[liquidationState] || "Unknown";

        // Display status
        console.clear();
        console.log("╔════════════════════════════════════════════════════════════╗");
        console.log("║         Power=2 Position Monitor (Squared Perps)          ║");
        console.log("╚════════════════════════════════════════════════════════════╝");
        console.log();
        console.log(`⏱️  Monitoring Time: ${elapsed}s | Update #${iteration}`);
        console.log();
        console.log("━".repeat(62));
        console.log("📍 POSITION DETAILS");
        console.log("━".repeat(62));
        console.log(`   Power Level: ${position.power} (Squared Perps)`);
        console.log(`   Side: ${SIDE.toUpperCase()}`);
        console.log(`   Entry Price: $${entryPrice.toFixed(2)}`);
        console.log(`   Position Size: ${SIZE_SOL} SOL ($${sizeUsd.toFixed(2)})`);
        console.log(`   Collateral: ${COLLATERAL_SOL} SOL ($${collateralUsd.toFixed(2)})`);
        console.log();

        console.log("━".repeat(62));
        console.log("💹 CURRENT MARKET");
        console.log("━".repeat(62));
        console.log(`   Current Price: $${currentMarketPrice.toFixed(2)}`);
        console.log(`   Price Change: ${priceChange >= 0 ? "+" : ""}${priceChange.toFixed(2)}%`);
        console.log();

        console.log("━".repeat(62));
        console.log("💰 PROFIT & LOSS");
        console.log("━".repeat(62));
        const pnlSign = netPnl >= 0 ? "+" : "";
        const pnlColor = netPnl >= 0 ? "✅" : "❌";
        console.log(`   Net PnL: ${pnlColor} ${pnlSign}$${netPnl.toFixed(2)} (${pnlSign}${pnlPercent.toFixed(2)}%)`);
        console.log(`   Profit: +$${profit.toFixed(2)}`);
        console.log(`   Loss: -$${loss.toFixed(2)}`);
        console.log(`   Fees: $${(pnl.fee.toNumber() / 1e6).toFixed(2)}`);
        console.log();

        console.log("━".repeat(62));
        console.log("⚠️  LIQUIDATION STATUS");
        console.log("━".repeat(62));
        console.log(`   Liquidation Price: $${liqPrice.toFixed(2)}`);
        console.log(`   Distance to Liq: ${distanceToLiq >= 0 ? "+" : ""}${distanceToLiq.toFixed(2)}%`);
        console.log(`   Liquidation State: ${liqStateLabel}`);

        if (liquidationState === 0) {
          console.log(`   Status: ✅ SAFE`);
        } else if (liquidationState === 1) {
          console.log(`   Status: ⚠️  WARNING - Can be liquidated`);
        } else {
          console.log(`   Status: 🚨 DANGER - Must be liquidated!`);
        }
        console.log();

        console.log("━".repeat(62));
        console.log("📊 POWER=2 PAYOFF EXPLANATION");
        console.log("━".repeat(62));
        console.log(`   Formula: PnL = Size × ((Exit/Entry)² - 1)`);
        console.log(`   Price Ratio: ${(currentMarketPrice / entryPrice).toFixed(4)}`);
        console.log(`   Ratio Squared: ${Math.pow(currentMarketPrice / entryPrice, 2).toFixed(4)}`);
        console.log(`   Expected Return: ${((Math.pow(currentMarketPrice / entryPrice, 2) - 1) * 100).toFixed(2)}%`);
        console.log();

        // Show power perps comparison
        console.log("━".repeat(62));
        console.log("🔢 POWER COMPARISON (Current Price Change)");
        console.log("━".repeat(62));
        const ratio = currentMarketPrice / entryPrice;
        const power1Return = ((ratio - 1) * 100).toFixed(2);
        const power2Return = ((Math.pow(ratio, 2) - 1) * 100).toFixed(2);
        const power3Return = ((Math.pow(ratio, 3) - 1) * 100).toFixed(2);

        console.log(`   Power=1 (Linear):  ${power1Return}%`);
        console.log(`   Power=2 (Squared): ${power2Return}% ← YOUR POSITION`);
        console.log(`   Power=3 (Cubed):   ${power3Return}%`);
        console.log();

        console.log("Press Ctrl+C to stop monitoring");

        // Auto-stop if liquidated
        if (liquidationState >= 2) {
          console.log();
          console.log("🚨 POSITION HAS BEEN LIQUIDATED!");
          console.log("Stopping monitor...");
          clearInterval(monitorInterval);
          process.exit(0);
        }

      } catch (error: any) {
        console.error("❌ Error monitoring position:", error.message);
        if (error.message.includes("Account does not exist")) {
          console.log("Position may have been closed or liquidated.");
          clearInterval(monitorInterval);
          process.exit(1);
        }
      }
    }, MONITOR_INTERVAL);

    // Handle Ctrl+C gracefully
    process.on("SIGINT", () => {
      console.log();
      console.log();
      console.log("🛑 Monitoring stopped by user");
      clearInterval(monitorInterval);
      process.exit(0);
    });

  } catch (error: any) {
    console.error("❌ Error:", error.message);

    if (error.logs) {
      console.log("\n📋 Transaction Logs:");
      error.logs.forEach((log: string) => console.log(`   ${log}`));
    }

    if (error.message.includes("custom program error: 0x1775")) {
      console.log("\n💡 Tip: This error suggests leverage is too high for power=2.");
      console.log("   Try reducing the position size or increasing collateral.");
      console.log(`   Max initial leverage for power=2: 20x`);
      console.log(`   Your leverage: ${(SIZE_SOL / COLLATERAL_SOL).toFixed(2)}x`);
    }

    if (error.message.includes("0x1")) {
      console.log("\n💡 Tip: Insufficient funds. Make sure you have:");
      console.log(`   - At least ${COLLATERAL_SOL} SOL in your wallet`);
      console.log(`   - SOL wrapped as token account if needed`);
    }

    process.exit(1);
  }
}

main();
