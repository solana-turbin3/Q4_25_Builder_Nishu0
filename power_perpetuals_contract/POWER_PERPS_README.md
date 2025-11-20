# Power Perpetuals Protocol

## Overview

This protocol implements **Power Perpetuals** - a novel type of derivative that provides non-linear exposure to underlying assets. Unlike traditional linear perpetuals where returns are proportional to price changes, power perpetuals amplify returns based on the **power of the price ratio**.

## What are Power Perpetuals?

Power Perpetuals are derivatives whose payoff is determined by raising the price ratio to a power (exponent). This creates convex payoff profiles that amplify both gains and losses.

### Linear vs Power Perpetuals

**Linear Perpetuals (power=1):**
```
Return = (Exit_Price / Entry_Price) - 1
```
- If price doubles (2x): +100% return
- If price halves (0.5x): -50% return

**Power Perpetuals (power=n):**
```
Return = (Exit_Price / Entry_Price)^n - 1
```

**Example with power=2 (Squared Perps):**
- If price doubles (2x): (2)² - 1 = +300% return
- If price halves (0.5x): (0.5)² - 1 = -75% return

**Example with power=3 (Cubed Perps):**
- If price doubles (2x): (2)³ - 1 = +700% return
- If price triples (3x): (3)³ - 1 = +2600% return

## Architecture

### Core Components

```
┌─────────────────────────────────────────────────────────────┐
│                    Power Perpetuals Protocol                 │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐         ┌─────────────┐                   │
│  │  Perpetuals  │────────▶│    Pool     │                   │
│  │   (Global)   │         │  (Markets)  │                   │
│  └──────────────┘         └─────────────┘                   │
│         │                        │                           │
│         │                        ├──────┐                    │
│         │                        │      │                    │
│         ▼                        ▼      ▼                    │
│  ┌──────────────┐         ┌───────┐ ┌───────┐              │
│  │   Multisig   │         │Custody│ │Custody│ ...          │
│  │   (Admin)    │         │(Token)│ │(Token)│              │
│  └──────────────┘         └───────┘ └───────┘              │
│                                 │                            │
│                                 ▼                            │
│                          ┌──────────────┐                   │
│                          │   Position   │                   │
│                          │  (User Trade)│                   │
│                          │  power: 1-5  │                   │
│                          └──────────────┘                   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Key Accounts

#### 1. **Perpetuals** (Global State)
- Single program-wide account
- Stores global settings and permissions
- Manages admin multisig configuration

#### 2. **Pool**
- Represents a trading market (e.g., "MainPool")
- Contains multiple token custodies
- Tracks Assets Under Management (AUM)
- Manages liquidity provider (LP) token mint

#### 3. **Custody**
- One per token per pool
- Stores token-specific configuration:
  - Oracle settings (Pyth price feeds)
  - Pricing parameters (spreads, fees)
  - Leverage limits
  - Borrow rate parameters
- Holds actual token balances in custody token account

#### 4. **Position**
- User's open trade
- **New field: `power` (1-5)** - determines payoff curve
- Tracks:
  - Position side (Long/Short)
  - Entry price
  - Position size
  - Collateral amount
  - Unrealized PnL
  - Accumulated interest

#### 5. **Multisig**
- Admin access control
- Requires M-of-N signatures for privileged operations
- Manages protocol parameters

## Power Perpetuals Math

### Payoff Calculation

The core innovation is in the PnL calculation (`calc_power_perps_pnl` in `math.rs`):

```rust
// Calculate price ratio: exit_price / entry_price
let ratio = exit_price / entry_price

// Raise to power
let ratio_powered = ratio^power

// Calculate return
if ratio_powered >= 1.0:
    profit_usd = size_usd * (ratio_powered - 1)
else:
    loss_usd = size_usd * (1 - ratio_powered)
```

### Implementation Details

**Step 1: Price Ratio Calculation**
```
ratio = (exit_price * 10^price_decimals) / entry_price
```
We scale by `10^price_decimals` to maintain precision during exponentiation.

**Step 2: Power Calculation**
```rust
if power == 1:
    ratio_powered = ratio
else:
    ratio_powered = ratio
    for i in 1..power:
        ratio_powered = (ratio_powered * ratio) / 10^price_decimals
```
We multiply iteratively and rescale to avoid overflow.

**Step 3: Return Calculation**
```
return_multiplier = |ratio_powered - 10^price_decimals|
pnl = (size_usd * return_multiplier) / 10^price_decimals
```

### Long vs Short Positions

**Long Position (betting price goes up):**
```
pnl = calc_power_perps_pnl(exit_price, entry_price, size, power)
```

**Short Position (betting price goes down):**
```
pnl = calc_power_perps_pnl(entry_price, exit_price, size, power)
```
We invert the prices for shorts, so they profit when price decreases.

## Power-Based Leverage Limits

Higher power = higher volatility = stricter leverage limits

| Power | Type      | Max Initial Leverage | Max Leverage | Risk Profile |
|-------|-----------|---------------------|--------------|--------------|
| 1     | Linear    | Custody default     | Custody default | Standard |
| 2     | Squared   | 20x                 | 40x          | High |
| 3     | Cubed     | 10x                 | 20x          | Very High |
| 4     | Power-4   | 5x                  | 10x          | Extreme |
| 5     | Power-5   | 3x                  | 6x           | Maximum |

Implementation in `pool.rs` `check_leverage()`:
```rust
let power_max_initial_leverage = match position.power {
    1 => custody.pricing.max_initial_leverage,
    2 => min(custody.pricing.max_initial_leverage, 20_0000), // 20x in BPS
    3 => min(custody.pricing.max_initial_leverage, 10_0000),
    4 => min(custody.pricing.max_initial_leverage, 5_0000),
    5 => min(custody.pricing.max_initial_leverage, 3_0000),
    _ => custody.pricing.max_initial_leverage,
};
```

## Code Structure

### Modified Files

1. **`programs/perpetuals/src/state/position.rs`**
   - Added `power: u8` field to Position struct
   - Power must be between 1-5

2. **`programs/perpetuals/src/math.rs`**
   - New function: `calc_power_perps_pnl()`
   - Implements power-based payoff calculation
   - Uses safe checked math to prevent overflow

3. **`programs/perpetuals/src/state/pool.rs`**
   - Modified `get_pnl_usd()` to use power perps formula
   - Updated `check_leverage()` with power-based limits
   - Replaces linear price difference with power calculation

4. **`programs/perpetuals/src/instructions/open_position.rs`**
   - Added `power: u8` to `OpenPositionParams`
   - Validates power is 1-5
   - Assigns power to position on creation

### Instruction Flow

**Opening a Position:**
```
1. User calls open_position with:
   - price (slippage protection)
   - collateral amount
   - position size
   - side (Long/Short)
   - power (1-5) ← NEW!

2. Validate:
   - Permissions enabled
   - Valid inputs
   - Power in range [1,5] ← NEW!
   - Collateral custody correct

3. Calculate:
   - Entry price (with spread)
   - Position size in USD
   - Collateral value in USD
   - Initial leverage

4. Verify:
   - Leverage within power-based limits ← MODIFIED!
   - Sufficient liquidity available

5. Execute:
   - Transfer collateral from user
   - Lock funds for potential profit
   - Create Position account with power ← MODIFIED!
   - Update custody statistics
```

**Calculating PnL:**
```
1. Get current price from oracle
2. Calculate exit price (with spread/fees)
3. Apply power perps formula: ← MODIFIED!
   - Long: pnl = size * ((exit/entry)^power - 1)
   - Short: pnl = size * ((entry/exit)^power - 1)
4. Add/subtract fees, interest, unrealized PnL
5. Cap profit at locked collateral
6. Return (profit, loss, fees)
```

## Example Scenarios

### Scenario 1: Linear Perps (power=1)

**Setup:**
- Entry Price: $100
- Position Size: $10,000
- Power: 1

**Outcome if price → $150 (+50%):**
```
Return = (150/100)^1 - 1 = 0.50 = +50%
Profit = $10,000 * 0.50 = $5,000
```

### Scenario 2: Squared Perps (power=2)

**Setup:**
- Entry Price: $100
- Position Size: $10,000
- Power: 2

**Outcome if price → $150 (+50%):**
```
Return = (150/100)^2 - 1 = 2.25 - 1 = 1.25 = +125%
Profit = $10,000 * 1.25 = $12,500
```

**Outcome if price → $75 (-25%):**
```
Return = (75/100)^2 - 1 = 0.5625 - 1 = -0.4375 = -43.75%
Loss = $10,000 * 0.4375 = $4,375
```

### Scenario 3: Cubed Perps (power=3)

**Setup:**
- Entry Price: $100
- Position Size: $10,000
- Power: 3

**Outcome if price → $150 (+50%):**
```
Return = (150/100)^3 - 1 = 3.375 - 1 = 2.375 = +237.5%
Profit = $10,000 * 2.375 = $23,750
```

**Outcome if price → $75 (-25%):**
```
Return = (75/100)^3 - 1 = 0.421875 - 1 = -0.578125 = -57.8%
Loss = $10,000 * 0.578125 = $5,781.25
```

### Scenario 4: Max Power (power=5)

**Setup:**
- Entry Price: $100
- Position Size: $10,000
- Power: 5

**Outcome if price → $120 (+20%):**
```
Return = (120/100)^5 - 1 = 2.48832 - 1 = 1.48832 = +148.8%
Profit = $10,000 * 1.48832 = $14,883.20
```

**Outcome if price → $90 (-10%):**
```
Return = (90/100)^5 - 1 = 0.59049 - 1 = -0.40951 = -40.95%
Loss = $10,000 * 0.40951 = $4,095.10
```

## Risk Considerations

### Amplified Volatility
- Power>1 amplifies both gains AND losses
- Small price movements create large PnL swings
- Example: power=5 with 10% price move = ~41-49% PnL swing

### Leverage Interaction
- Power perps are already leveraged by nature
- Additional leverage multiplies the effect
- Max leverage decreases as power increases

### Liquidation Risk
- Higher power = faster approach to liquidation
- Tighter stop-losses recommended
- Smaller position sizes advisable

### Optimal Use Cases

**Power=1 (Linear):**
- Standard perpetual trading
- Lower risk, moderate returns
- Suitable for all market conditions

**Power=2 (Squared):**
- Moderate volatility amplification
- Good for trending markets
- Balanced risk/reward

**Power=3-5 (High Power):**
- Extreme volatility amplification
- Only for strong convictions
- Requires tight risk management
- Best in highly volatile or breakout scenarios

## Deployment

### Requirements
- Solana devnet/mainnet access
- 5-6 SOL for deployment
- Admin keypair

### Commands

**Deploy Program:**
```bash
anchor build
anchor deploy --provider.cluster devnet \
  --program-name perpetuals \
  --program-keypair target/deploy/perpetuals-keypair.json
```

**Initialize IDL:**
```bash
anchor idl init --provider.cluster devnet \
  --filepath ./target/idl/perpetuals.json \
  <PROGRAM_ID>
```

**Initialize Protocol:**
```bash
cd app
npx ts-node src/cli.ts -k <ADMIN_WALLET> init \
  --min-signatures 1 <ADMIN_PUBKEY>
```

**Create Pool:**
```bash
npx ts-node src/cli.ts -k <ADMIN_WALLET> add-pool MainPool
```

**Add Token Custody:**
```bash
npx ts-node src/cli.ts -k <ADMIN_WALLET> add-custody \
  MainPool \
  <TOKEN_MINT> \
  <PYTH_ORACLE_ACCOUNT>
```

## Testing

### Test Opening Positions with Different Powers

```typescript
// Linear perps (power=1)
await client.openPosition(
  wallet,
  "MainPool",
  tokenMint,
  collateralMint,
  "long",
  price,
  collateral,
  size,
  1  // power
);

// Squared perps (power=2)
await client.openPosition(
  wallet,
  "MainPool",
  tokenMint,
  collateralMint,
  "long",
  price,
  collateral,
  size,
  2  // power
);

// Max power (power=5)
await client.openPosition(
  wallet,
  "MainPool",
  tokenMint,
  collateralMint,
  "long",
  price,
  collateral,
  size,
  5  // power
);
```

## Security Considerations

1. **Overflow Protection**
   - All power calculations use checked math
   - Rescaling after each multiplication prevents overflow

2. **Power Validation**
   - Strictly enforced 1-5 range
   - Invalid powers rejected at instruction level

3. **Leverage Caps**
   - Automatically reduced for higher powers
   - Prevents excessive risk exposure

4. **Price Oracle Integrity**
   - Relies on Pyth for price feeds
   - Oracle staleness checks in place

5. **Collateral Requirements**
   - Locked collateral caps maximum profit
   - Protects pool from unlimited losses

## Future Enhancements

1. **Dynamic Power Adjustment**
   - Allow changing power on existing positions
   - Recalculate collateral requirements

2. **Power-Specific Fee Structures**
   - Higher fees for higher power positions
   - Risk-adjusted pricing

3. **Cross-Power Hedging**
   - Match power=2 longs with power=0.5 positions
   - Natural hedging strategies

4. **Power Options**
   - Combine power perps with options
   - Create complex payoff structures

## Conclusion

Power Perpetuals provide a powerful new primitive for DeFi trading, offering convex payoff profiles that amplify returns based on configurable power settings (1-5). The protocol maintains safety through power-adjusted leverage limits while enabling sophisticated trading strategies unavailable in traditional linear perpetuals.

The implementation is production-ready, fully tested, and deployed on Solana devnet.

## Program ID

- **Devnet:** `GxegSBD3PQFQjzYui524RWraUM7SzQsBZpWnwBbpLQbk`

## Resources

- [Solana Devnet Explorer](https://explorer.solana.com/?cluster=devnet)
- [Pyth Price Feeds](https://pyth.network/developers/price-feed-ids)
- [Anchor Documentation](https://www.anchor-lang.com/)

## License

[Add your license here]

## Contact

[Add contact information]
