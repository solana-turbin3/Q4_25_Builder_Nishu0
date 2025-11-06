"use client";

import { Star } from "lucide-react";
import { formatNumber } from "@/lib/utils";

export function MarketHeader() {
  const markets = [
    { symbol: "ZEC", name: "Zcash", price: 0, change: 12.26 },
    { symbol: "SOL", name: "Solana", price: 160.31, change: -0.74, active: true },
    { symbol: "ETH", name: "Ethereum", price: 0, change: -0.8 },
    { symbol: "BTC", name: "Bitcoin", price: 0, change: -0.45 },
    { symbol: "WIF", name: "WIF", price: 0, change: -1.43 },
    { symbol: "XAG", name: "Silver", price: 0, change: 1.01 },
    { symbol: "EUR", name: "EUR", price: 0, change: 0.35 },
    { symbol: "ZZ", name: "ZZ", price: 0, change: 0 },
  ];

  return (
    <div className="flex items-center gap-4 bg-[#0a0a0a] px-6 py-2 border-b border-white/5">
      {/* Star icon */}
      <span className="mr-2">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 16 16">
          <path
            fill="url(#star-fill_svg__a)"
            d="m8 11.847 4.12 2.486-1.093-4.686 3.64-3.154-4.794-.406L8 1.667l-1.873 4.42-4.794.406 3.64 3.154-1.093 4.686z"
          />
          <defs>
            <linearGradient id="star-fill_svg__a" x1="1.333" x2="19.163" y1="1.667" y2="1.998" gradientUnits="userSpaceOnUse">
              <stop offset="0.302" stopColor="#FFBC42" />
              <stop offset="1" stopColor="#FFC864" />
            </linearGradient>
          </defs>
        </svg>
      </span>

      {/* Market ticker strip */}
      <div className="flex items-center gap-[10px] overflow-x-auto flex-1 no-scrollbar">
        {markets.map((market) => (
          <button
            key={market.symbol}
            className={`flex items-center gap-[5px] whitespace-nowrap px-[10px] py-1 rounded-lg transition-all ${
              market.active
                ? "bg-blue-500/10 text-white border border-blue-500/20"
                : "text-zinc-600 hover:bg-white/5 hover:text-zinc-400 border border-transparent"
            }`}
          >
            {/* Coin icon placeholder - in real app would use actual coin images */}
            <div className="w-4 h-4 rounded-full bg-gradient-to-br from-blue-400 to-purple-600"></div>
            
            <div className="flex items-center gap-2">
              <span className={`text-xs font-medium uppercase ${market.active ? "text-white" : ""}`}>
                {market.symbol}
              </span>
              {market.change !== 0 && (
                <span className={`flex items-center gap-1 text-xs font-medium ${market.change > 0 ? "text-[#4ADE80]" : "text-[#EF4444]"}`}>
                  {market.change > 0 ? (
                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" fill="none" viewBox="0 0 10 10">
                      <path
                        stroke="currentColor"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth="1.667"
                        d="M5 9V1m0 0L1 4.778M5 1l4 3.778"
                      />
                    </svg>
                  ) : (
                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" fill="none" viewBox="0 0 10 10">
                      <path
                        stroke="currentColor"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth="1.667"
                        d="M5 1v8m0 0L1 5.222M5 9l4-3.778"
                      />
                    </svg>
                  )}
                  {Math.abs(market.change).toFixed(2)}%
                </span>
              )}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
