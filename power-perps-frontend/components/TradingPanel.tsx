"use client";

import { useState } from "react";
import { ChevronDown, Info, TrendingUp, TrendingDown } from "lucide-react";
import { cn, formatNumber } from "@/lib/utils";

type OrderType = "Market" | "Limit";
type TradeType = "Long" | "Short";

export function TradingPanel() {
  const [tradeType, setTradeType] = useState<TradeType>("Long");
  const [orderType, setOrderType] = useState<OrderType>("Market");
  const [price, setPrice] = useState("161.00");
  const [size, setSize] = useState("0.00");
  const [leverage, setLeverage] = useState(1);

  const leverageOptions = [1, 25, 50, 75, 100];

  return (
    <div className="flex w-96 flex-col gap-3 bg-[#0a0a0a] p-5">
      {/* Long/Short Tabs */}
      <div className="flex gap-2 rounded-lg bg-[#111111] p-1 shadow-sm">
        <button
          onClick={() => setTradeType("Long")}
          className={cn(
            "flex flex-1 items-center justify-center gap-1.5 rounded-md py-2 text-xs font-semibold transition-all",
            tradeType === "Long"
              ? "bg-cyan-500 text-white shadow-md shadow-cyan-500/20"
              : "text-zinc-500 hover:text-white"
          )}
        >
          <TrendingUp className="h-3.5 w-3.5" />
          Long
        </button>
        <button
          onClick={() => setTradeType("Short")}
          className={cn(
            "flex flex-1 items-center justify-center gap-1.5 rounded-md py-2 text-xs font-semibold transition-all",
            tradeType === "Short"
              ? "bg-zinc-700 text-white shadow-md"
              : "text-zinc-500 hover:text-white"
          )}
        >
          <TrendingDown className="h-3.5 w-3.5" />
          Short
        </button>
      </div>

      {/* Market/Limit Tabs */}
      <div className="flex gap-2">
        <button
          onClick={() => setOrderType("Market")}
          className={cn(
            "flex-1 rounded-lg py-1.5 text-xs font-medium transition-all",
            orderType === "Market"
              ? "bg-[#111111] text-white shadow-sm"
              : "text-zinc-600 hover:text-zinc-400"
          )}
        >
          Market
        </button>
        <button
          onClick={() => setOrderType("Limit")}
          className={cn(
            "flex-1 rounded-lg py-1.5 text-xs font-medium transition-all",
            orderType === "Limit"
              ? "bg-[#111111] text-white shadow-sm"
              : "text-zinc-600 hover:text-zinc-400"
          )}
        >
          Limit
        </button>
      </div>

      {/* Price Input */}
      <div>
        <label className="mb-1.5 block text-xs font-normal text-zinc-600">
          Price
        </label>
        <div className="flex items-center gap-2 rounded-lg bg-[#111111] px-3 py-2.5 shadow-sm">
          <input
            type="text"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            className="flex-1 bg-transparent text-base font-medium text-white outline-none"
            placeholder="0.00"
          />
          <span className="text-xs font-medium text-zinc-500">USD</span>
        </div>
      </div>

      {/* Size Inputs */}
      <div>
        <label className="mb-1.5 block text-xs font-normal text-zinc-600">
          Size
        </label>
        <div className="flex items-center gap-2 rounded-lg bg-[#111111] px-3 py-2.5 shadow-sm">
          <input
            type="text"
            value={size}
            onChange={(e) => setSize(e.target.value)}
            className="flex-1 bg-transparent text-base font-medium text-white outline-none"
            placeholder="0.00"
          />
          <button className="flex items-center gap-1 text-xs font-medium text-zinc-400">
            <span className="flex h-4 w-4 items-center justify-center rounded bg-blue-500 text-[10px] text-white">
              ⟡
            </span>
            USDC
            <ChevronDown className="h-3 w-3" />
          </button>
        </div>
      </div>

      <div>
        <div className="flex items-center gap-2 rounded-lg bg-[#111111] px-3 py-2.5 shadow-sm">
          <input
            type="text"
            value={size}
            onChange={(e) => setSize(e.target.value)}
            className="flex-1 bg-transparent text-base font-medium text-white outline-none"
            placeholder="0.00"
          />
          <button className="flex items-center gap-1 text-xs font-medium text-zinc-400">
            <span className="text-base">◎</span>
            SOL
            <ChevronDown className="h-3 w-3" />
          </button>
        </div>
      </div>

      {/* Leverage */}
      <div>
        <div className="mb-2 flex items-center justify-between">
          <label className="text-xs font-normal text-zinc-600">Leverage</label>
          <span className="text-xs text-zinc-500">{leverage}x</span>
        </div>
        <div className="mb-3">
          <input
            type="range"
            min="1"
            max="100"
            value={leverage}
            onChange={(e) => setLeverage(Number(e.target.value))}
            className="w-full cursor-pointer"
            style={{
              background: `linear-gradient(to right, #3b82f6 0%, #3b82f6 ${leverage}%, #1a1a1a ${leverage}%, #1a1a1a 100%)`,
            }}
          />
        </div>
        <div className="flex items-center justify-between gap-2">
          <div className="flex gap-1.5">
            {leverageOptions.map((lev) => (
              <button
                key={lev}
                onClick={() => setLeverage(lev)}
                className={cn(
                  "rounded-full px-3 py-1 text-xs font-medium transition-all",
                  leverage === lev
                    ? "bg-blue-500 text-white shadow-md shadow-blue-500/20"
                    : "bg-[#111111] text-zinc-500 hover:bg-[#1a1a1a] hover:text-zinc-400"
                )}
              >
                {lev}x
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Slippage Tolerance */}
      <div className="flex items-center justify-between rounded-lg bg-[#111111] px-3 py-2 shadow-sm">
        <span className="text-xs text-zinc-500">Slippage Tolerance</span>
        <div className="flex items-center gap-1">
          <span className="text-xs font-medium text-zinc-400">0.8%</span>
          <ChevronDown className="h-3 w-3 text-zinc-600" />
        </div>
      </div>

      {/* Take Profit / Stop Loss */}
      <div className="flex items-center justify-between rounded-lg bg-[#111111] px-3 py-2 shadow-sm">
        <span className="text-xs text-zinc-500">Take Profit / Stop Loss</span>
        <div className="flex items-center gap-1">
          <Info className="h-3 w-3 text-zinc-600" />
          <ChevronDown className="h-3 w-3 text-zinc-600" />
        </div>
      </div>

      {/* Connect Wallet Button */}
      <button className="w-full rounded-lg bg-gradient-to-r from-yellow-400 via-yellow-500 to-green-500 py-3 text-sm font-semibold text-black shadow-lg transition-all hover:shadow-xl">
        Connect Wallet
      </button>

      {/* Collateral Info */}
      <div className="space-y-1.5 pt-3 text-xs">
        <div className="flex justify-between">
          <span className="text-zinc-600">Collateral In</span>
          <span className="font-medium text-zinc-400">JitoSOL</span>
        </div>
        <div className="flex justify-between">
          <span className="text-zinc-600">Leverage</span>
          <span className="font-medium text-zinc-400">-</span>
        </div>
        <div className="flex justify-between">
          <span className="text-zinc-600">Entry Price</span>
          <span className="font-medium text-zinc-400">-</span>
        </div>
        <div className="flex justify-between">
          <span className="text-zinc-600">Liq. Price</span>
          <span className="font-medium text-zinc-400">-</span>
        </div>
        <div className="flex justify-between">
          <span className="text-zinc-600">Fees (0.051%)</span>
          <span className="font-medium text-zinc-400">-</span>
        </div>
        <div className="flex justify-between">
          <span className="text-zinc-600">Margin Fees</span>
          <span className="font-medium text-zinc-400">0.00105% / 1hr</span>
        </div>
        <div className="flex justify-between pt-2">
          <span className="text-zinc-600">Available liquidity</span>
          <span className="flex items-center gap-1">
            <Info className="h-3 w-3 text-zinc-700" />
            <span className="font-semibold text-white">$2,180,845</span>
          </span>
        </div>
      </div>
    </div>
  );
}

