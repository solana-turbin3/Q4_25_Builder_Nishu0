import type { Metadata } from "next";
import { Inter } from "next/font/google";
import "./globals.css";
import { PrivyProvider } from "@/providers/PrivyProvider";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-inter",
});

export const metadata: Metadata = {
  title: "Power Solana - Perpetual Trading Platform",
  description:
    "Trade perpetual futures on Solana with up to 100x leverage. Powered by Privy.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="dark">
      <head>
        <script
          type="text/javascript"
          src="https://s3.tradingview.com/tv.js"
          async
        />
      </head>
      <body className={`${inter.variable} antialiased`}>
        <PrivyProvider>{children}</PrivyProvider>
      </body>
    </html>
  );
}
