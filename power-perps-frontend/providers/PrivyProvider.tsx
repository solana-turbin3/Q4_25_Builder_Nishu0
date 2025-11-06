"use client";

import { PrivyProvider as BasePrivyProvider } from "@privy-io/react-auth";
import { ReactNode } from "react";

export function PrivyProvider({ children }: { children: ReactNode }) {
  const appId = process.env.NEXT_PUBLIC_PRIVY_APP_ID;

  if (!appId) {
    console.error("NEXT_PUBLIC_PRIVY_APP_ID is not set");
    return <>{children}</>;
  }

  return (
    <BasePrivyProvider
      appId={appId}
      config={{
        appearance: {
          theme: "dark",
          accentColor: "#22c55e",
          logo: undefined,
        },
        loginMethods: ["email", "wallet", "google", "discord", "twitter"],
        embeddedWallets: {
          createOnLogin: "users-without-wallets",
        },
        defaultChain: {
          id: 900,
          name: "Solana Devnet",
          network: "solana-devnet",
          nativeCurrency: {
            name: "SOL",
            symbol: "SOL",
            decimals: 9,
          },
          rpcUrls: {
            default: {
              http: [
                process.env.NEXT_PUBLIC_SOLANA_RPC_URL ||
                  "https://api.devnet.solana.com",
              ],
            },
          },
        },
        supportedChains: [
          {
            id: 900,
            name: "Solana Devnet",
            network: "solana-devnet",
            nativeCurrency: {
              name: "SOL",
              symbol: "SOL",
              decimals: 9,
            },
            rpcUrls: {
              default: {
                http: [
                  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ||
                    "https://api.devnet.solana.com",
                ],
              },
            },
          },
        ],
      }}
    >
      {children}
    </BasePrivyProvider>
  );
}

