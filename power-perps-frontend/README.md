# Power Solana - Perpetual Trading Platform

A modern, high-performance perpetual futures trading platform built on Solana with up to 100x leverage. Features seamless wallet integration powered by Privy.

![Power Solana Platform](https://img.shields.io/badge/Solana-Devnet-14F195?logo=solana)
![Next.js](https://img.shields.io/badge/Next.js-16.0-black?logo=next.js)
![TypeScript](https://img.shields.io/badge/TypeScript-5.0-blue?logo=typescript)

## ✨ Features

- 🚀 **High Leverage Trading**: Trade with up to 100x leverage on Solana
- 💼 **Multiple Wallet Options**: Connect via Privy with email, social logins, or Web3 wallets
- 📊 **Real-time Market Data**: Live price feeds powered by Pyth Network
- 🎯 **Advanced Order Types**: Market and limit orders with stop-loss/take-profit
- 🌙 **Dark Mode UI**: Beautiful, modern interface optimized for traders
- ⚡ **Lightning Fast**: Built on Solana for instant transactions and low fees

## 🛠️ Tech Stack

- **Framework**: Next.js 16 (App Router)
- **Language**: TypeScript
- **Styling**: Tailwind CSS
- **Authentication**: Privy (Wallet & Social Auth)
- **Blockchain**: Solana (Devnet)
- **Icons**: Lucide React

## 📋 Prerequisites

Before you begin, ensure you have:

- Node.js 18+ installed
- npm, yarn, or pnpm package manager
- A Privy account and App ID (see setup below)

## 🚀 Quick Start

### 1. Clone the Repository

```bash
git clone <your-repo-url>
cd power-perps-frontend
```

### 2. Install Dependencies

```bash
npm install
# or
yarn install
# or
pnpm install
```

### 3. Set Up Privy (Required)

#### Create a Privy Account

1. Go to [https://dashboard.privy.io](https://dashboard.privy.io)
2. Sign up for a free account
3. Create a new app

#### Configure Your Privy App

In your Privy Dashboard:

1. **App Settings** → **Basics**:
   - Set your app name: "Power Solana"
   - Add your app logo (optional)

2. **App Settings** → **Login Methods**:
   - Enable the following login methods:
     - ✅ Email
     - ✅ Wallet (Solana)
     - ✅ Google
     - ✅ Twitter
     - ✅ Discord
     - ✅ Apple (optional)

3. **App Settings** → **Embedded Wallets**:
   - Enable "Create embedded wallets for users without wallets"
   - This allows users without a Web3 wallet to trade using Privy's embedded wallet

4. **App Settings** → **Chains**:
   - Add Solana Devnet to supported chains
   - Chain ID: `900`
   - RPC URL: `https://api.devnet.solana.com`

5. **App Settings** → **Domains**:
   - Add `localhost:3000` for development
   - Add your production domain when deploying

6. **Copy Your App ID**:
   - Go to **App Settings** → **Basics**
   - Copy your "App ID" (looks like: `clxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`)

### 4. Configure Environment Variables

Create a `.env.local` file in the root directory:

```bash
cp .env.local.example .env.local
```

Edit `.env.local` and add your Privy App ID:

```env
# Privy Configuration
NEXT_PUBLIC_PRIVY_APP_ID=your_privy_app_id_here

# Solana Network Configuration
NEXT_PUBLIC_SOLANA_NETWORK=devnet
NEXT_PUBLIC_SOLANA_RPC_URL=https://api.devnet.solana.com
```

**Important**: Replace `your_privy_app_id_here` with your actual Privy App ID from step 3.

### 5. Run the Development Server

```bash
npm run dev
# or
yarn dev
# or
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

## 🎯 Using the Platform

### Connecting Your Wallet

1. Click the **"Get Started!"** button in the top-right corner
2. Choose your preferred login method:
   - **Email**: Enter your email for a passwordless login
   - **Wallet**: Connect your existing Solana wallet (Phantom, Solflare, etc.)
   - **Social**: Sign in with Google, Twitter, or Discord

### Trading

1. **Select a Market**: Choose SOL/USDC or other available pairs
2. **Choose Order Type**: Market (instant) or Limit (specific price)
3. **Set Leverage**: Adjust leverage slider (1x - 100x)
4. **Enter Position Size**: Specify the amount you want to trade
5. **Place Order**: Click "Long" (buy) or "Short" (sell)

### Managing Positions

- View your open positions in the **Positions** tab at the bottom
- Monitor your PnL (Profit & Loss) in real-time
- Set stop-loss and take-profit levels to manage risk
- Close positions partially or completely

## 📁 Project Structure

```
power-perps-frontend/
├── app/
│   ├── layout.tsx          # Root layout with Privy provider
│   ├── page.tsx            # Main trading interface
│   └── globals.css         # Global styles
├── components/
│   ├── Header.tsx          # Top navigation with wallet connection
│   ├── MarketHeader.tsx    # Market ticker strip
│   ├── PriceInfo.tsx       # Current price and 24h stats
│   ├── TradingChart.tsx    # Price chart (placeholder)
│   ├── TradingPanel.tsx    # Order entry and position controls
│   └── PositionsTable.tsx  # Open positions and history
├── providers/
│   └── PrivyProvider.tsx   # Privy configuration wrapper
├── lib/
│   └── utils.ts            # Utility functions
└── .env.local              # Environment variables (not in git)
```

## 🔧 Configuration

### Changing Network (Devnet → Mainnet)

To switch to Solana Mainnet:

1. Update `.env.local`:
```env
NEXT_PUBLIC_SOLANA_NETWORK=mainnet-beta
NEXT_PUBLIC_SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

2. Update Privy dashboard to include Solana Mainnet chain
3. **Warning**: Use at your own risk. Test thoroughly on Devnet first!

### Customizing Appearance

Edit `providers/PrivyProvider.tsx` to customize the Privy modal:

```typescript
appearance: {
  theme: "dark",           // or "light"
  accentColor: "#22c55e",  // your brand color
  logo: "your-logo-url",   // your logo URL
}
```

## 🔐 Security Best Practices

- ✅ Never commit `.env.local` or expose your Privy App ID publicly
- ✅ Always test on Devnet before using real funds
- ✅ Implement proper rate limiting for production
- ✅ Use Privy's server-side authentication for sensitive operations
- ✅ Enable 2FA on your Privy dashboard account

## 📚 Environment Variables Reference

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `NEXT_PUBLIC_PRIVY_APP_ID` | ✅ Yes | Your Privy App ID from dashboard | `clxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` |
| `NEXT_PUBLIC_SOLANA_NETWORK` | ✅ Yes | Solana network to connect to | `devnet` or `mainnet-beta` |
| `NEXT_PUBLIC_SOLANA_RPC_URL` | ✅ Yes | Solana RPC endpoint | `https://api.devnet.solana.com` |

## 🐛 Troubleshooting

### "Privy App ID is not set" Error

- Make sure you created a `.env.local` file
- Verify your `NEXT_PUBLIC_PRIVY_APP_ID` is set correctly
- Restart the development server after changing environment variables

### Wallet Connection Issues

- Check that your domain is added in Privy dashboard → **Domains**
- Ensure the correct Solana network is configured in Privy
- Try clearing your browser cache and reconnecting

### Build Errors

```bash
# Clear Next.js cache
rm -rf .next
npm run dev
```

## 📖 Learn More

### Privy Documentation
- [Privy Documentation](https://docs.privy.io)
- [Privy React SDK](https://docs.privy.io/guide/react)
- [Privy + Solana Integration](https://docs.privy.io/guide/react/wallets/solana)

### Solana Resources
- [Solana Documentation](https://docs.solana.com)
- [Web3.js Guide](https://solana-labs.github.io/solana-web3.js/)
- [Pyth Network (Price Feeds)](https://pyth.network)

### Next.js Resources
- [Next.js Documentation](https://nextjs.org/docs)
- [Next.js App Router](https://nextjs.org/docs/app)
- [Tailwind CSS](https://tailwindcss.com/docs)

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

This project is open source and available under the [MIT License](LICENSE).

## 🆘 Support

If you encounter any issues or have questions:

1. Check the troubleshooting section above
2. Review [Privy Documentation](https://docs.privy.io)
3. Open an issue on GitHub

---

**Happy Trading! 🚀**

Built with ❤️ on Solana
