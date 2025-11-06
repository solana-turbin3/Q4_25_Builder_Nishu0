"use client";

import { formatNumber } from "@/lib/utils";

export function PriceInfo() {
  return (
    <div className="flex items-center gap-6 bg-[#0a0a0a] px-6 py-3">
      <div>
        <div className="mb-1 text-xs font-normal text-zinc-600">
          SOL/USDC · 100x
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-1.5 rounded-full bg-cyan-400 shadow-sm shadow-cyan-400/50" />
            <span className="text-2xl font-semibold text-white">
              ${formatNumber(160.31, 2)}
            </span>
          </div>
          <div className="flex items-center gap-3 text-xs text-zinc-500">
            <span>24h High</span>
            <span className="font-medium text-zinc-400">
              ${formatNumber(162.89, 2)}
            </span>
          </div>
          <div className="flex items-center gap-3 text-xs text-zinc-500">
            <span>24h Low</span>
            <span className="font-medium text-zinc-400">
              ${formatNumber(157.09, 2)}
            </span>
          </div>
          <div className="rounded-md bg-cyan-500/10 px-2 py-1 text-xs font-medium text-cyan-400">
            +0.35 (+0.22%)
          </div>
        </div>
      </div>

      <div className="ml-auto">
        <div className="text-xs font-normal text-zinc-600">24h Volume</div>
        <div className="text-base font-semibold text-white">$6.19M</div>
      </div>
    </div>
  );
}

