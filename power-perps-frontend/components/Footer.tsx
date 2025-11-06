"use client";

import { Settings, HelpCircle } from "lucide-react";

export function Footer() {
  return (
    <footer className="fixed bottom-0 left-0 right-0 z-50 hidden bg-[#0a0a0a] sm:block">
      <div className="flex items-center justify-between px-6 py-2.5">
        {/* PYTH Logo */}
        <div className="flex items-center gap-4">
          <div className="flex h-4 w-16 items-center text-zinc-700">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="50"
              height="16"
              fill="none"
              viewBox="0 0 371 128"
            >
              <path
                fill="currentColor"
                d="M51.17 0c-9.32 0-18.06 2.49-25.59 6.85a51 51 0 0 0-12.79 10.48C4.83 26.35 0 38.2 0 51.17v38.38l12.79 12.79V51.17c0-11.36 4.94-21.58 12.79-28.61 3.69-3.3 8.03-5.9 12.79-7.58 4-1.42 8.31-2.19 12.79-2.19 21.19 0 38.38 17.18 38.38 38.38S72.36 89.55 51.16 89.55v12.79c28.26 0 51.17-22.91 51.17-51.17S79.44 0 51.17 0"
              />
              <path
                fill="currentColor"
                d="M63.96 51.17c0 7.06-5.73 12.79-12.79 12.79v12.79c14.13 0 25.59-11.46 25.59-25.59S65.3 25.57 51.17 25.57c-4.66 0-9.03 1.24-12.79 3.43-7.65 4.42-12.79 12.69-12.79 22.16v63.97l11.5 11.5 1.29 1.29V51.17c0-7.06 5.73-12.79 12.79-12.79s12.79 5.73 12.79 12.79"
              />
            </svg>
          </div>

          <div className="flex items-center gap-4 text-[11px] font-normal text-zinc-600">
            <div className="flex items-center gap-1.5">
              <span>Priority Fees:</span>
              <span className="text-zinc-500">DYNAMIC</span>
            </div>

            <div className="flex items-center gap-1.5">
              <span>Pyth Status:</span>
              <span className="text-zinc-500">Up</span>
            </div>

            <div className="flex items-center gap-1.5">
              <div className="h-1 w-1 rounded-full bg-cyan-400 shadow-sm shadow-cyan-400/50" />
              <span>Triton RPC Pool</span>
              <span className="text-zinc-700">(184 ms)</span>
            </div>

            <span className="text-zinc-600">mainnet-beta</span>

            <span className="text-zinc-600">3081.78 TPS</span>
          </div>
        </div>

        {/* Right side buttons */}
        <div className="flex items-center gap-2">
          <button className="flex items-center gap-1.5 rounded-lg bg-[#111111] px-2.5 py-1.5 text-[11px] font-medium text-zinc-500 shadow-sm transition-colors hover:bg-[#1a1a1a] hover:text-zinc-400">
            <Settings className="h-3 w-3" />
            <span>Settings</span>
          </button>

          <button className="flex items-center gap-1.5 rounded-lg bg-[#111111] px-2.5 py-1.5 text-[11px] font-medium text-cyan-500 shadow-sm transition-colors hover:bg-[#1a1a1a]">
            <HelpCircle className="h-3 w-3" />
            <span>Help</span>
          </button>
        </div>
      </div>
    </footer>
  );
}

