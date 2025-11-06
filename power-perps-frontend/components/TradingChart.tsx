"use client";

import { useState } from "react";
import { TradingViewChart } from "./TradingViewChart";

export function TradingChart() {
  const [selectedSymbol, setSelectedSymbol] = useState("BINANCE:SOLUSDT");

  const markets = [
    { symbol: "BINANCE:SOLUSDT", label: "SOL" },
    { symbol: "BINANCE:BTCUSDT", label: "BTC" },
    { symbol: "BINANCE:ETHUSDT", label: "ETH" },
  ];

  return (
    <div className="flex flex-1 flex-col bg-[#0a0a0a]">
      {/* Market selector tabs */}
      <div className="flex items-center gap-2 bg-[#0a0a0a] px-4 py-2">
        {markets.map((market) => (
          <button
            key={market.symbol}
            onClick={() => setSelectedSymbol(market.symbol)}
            className={`rounded-md px-3 py-1 text-xs font-medium transition-all ${
              selectedSymbol === market.symbol
                ? "bg-blue-500/10 text-blue-400 shadow-sm"
                : "text-zinc-600 hover:text-zinc-400"
            }`}
          >
            {market.label}
          </button>
        ))}
      </div>

      {/* TradingView Chart */}
      <div className="flex-1">
        <TradingViewChart symbol={selectedSymbol} />
      </div>
    </div>
  );
}

