"use client";

import { AlertCircle, TrendingUp, History, BarChart3 } from "lucide-react";

export function PositionsTable() {
  const tabs = [
    { label: "Positions", icon: TrendingUp },
    { label: "Orders", icon: BarChart3 },
    { label: "History", icon: History },
    { label: "PnL", icon: AlertCircle },
  ];

  return (
    <div className="mb-12 bg-[#0a0a0a]">
      {/* Tabs */}
      <div className="flex items-center justify-center gap-6 px-6 py-2">
        {tabs.map((tab, index) => (
          <button
            key={tab.label}
            className="flex flex-col items-center gap-1 py-2 transition-all data-[active=true]:text-cyan-400"
            data-active={index === 0}
          >
            <tab.icon
              className={`h-4 w-4 ${index === 0 ? "text-cyan-400" : "text-zinc-600"}`}
            />
          </button>
        ))}
      </div>

      {/* Table Header */}
      <div className="grid grid-cols-9 gap-4 px-6 py-2 text-xs font-normal text-zinc-600">
        <div>Market</div>
        <div>Side</div>
        <div>Size</div>
        <div>Collateral</div>
        <div>Entry Price</div>
        <div>Mark Price</div>
        <div>Liq Price</div>
        <div>SL/TP</div>
        <div className="text-right">Actions</div>
      </div>

      {/* Empty State */}
      <div className="flex min-h-[140px] flex-col items-center justify-center py-6">
        <div className="mb-2 flex h-8 w-8 items-center justify-center rounded-full bg-[#111111] shadow-sm">
          <AlertCircle className="h-4 w-4 text-zinc-700" />
        </div>
        <h3 className="mb-1 text-xs font-medium text-zinc-500">
          No Positions
        </h3>
        <p className="text-[11px] text-zinc-700">
          Please connect your wallet
        </p>
      </div>
    </div>
  );
}

