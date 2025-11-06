"use client";

import { usePrivy } from "@privy-io/react-auth";

export function Header() {
  const { ready, authenticated, user, login, logout } = usePrivy();

  return (
    <header className="relative flex items-center justify-between bg-[#0a0a0a] px-6 py-3 border-b border-white/5">
      {/* Logo */}
      <div className="flex items-center gap-2.5 w-[114px]">
        <div className="flex h-8 w-8 items-center justify-center rounded-md bg-linear-to-br from-blue-500 to-blue-600 shadow-lg shadow-blue-500/20">
          <span className="text-base font-bold text-white">P</span>
        </div>
        <span className="text-base font-bold text-white">Power</span>
      </div>

      {/* Center Navigation */}
      <nav className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 flex items-center gap-[15px]">
        <button className="flex items-center gap-2 px-3 py-2 text-sm font-semibold text-zinc-500 transition-colors hover:text-white">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 16 16">
            <path fill="currentColor" d="M4.757 8.822v-2.82a.75.75 0 0 1 .75-.751h2.656l-.209 1.081a.42.42 0 0 0 .717.299l.754-.753 1.452-1.453a.43.43 0 0 0 .125-.299.42.42 0 0 0-.125-.297L9.425 2.377l-.754-.753a.42.42 0 0 0-.717.298l.209 1.082h-3.55A2.103 2.103 0 0 0 2.51 5.107v3.762c0 .631.522 1.141 1.158 1.122.615-.018 1.09-.554 1.09-1.17M11.243 7.179v2.82a.75.75 0 0 1-.75.75H7.838l.208-1.081a.42.42 0 0 0-.717-.299l-.754.754-1.452 1.452a.43.43 0 0 0-.125.299c0 .107.043.213.125.297l1.452 1.453.754.753a.42.42 0 0 0 .717-.299l-.208-1.081h3.55a2.103 2.103 0 0 0 2.102-2.104V7.133c0-.632-.522-1.142-1.158-1.123-.615.018-1.089.554-1.089 1.17"/>
          </svg>
          <span>Swap</span>
        </button>
        <button className="flex items-center gap-2 rounded-lg bg-blue-500/10 px-3 py-2 text-sm font-semibold text-blue-400 shadow-sm">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 16 16">
            <path fill="currentColor" d="M1.894 11.083v2.29c0 .14.113.256.255.256h1.77c.14 0 .256-.113.256-.255v-4.03L2.89 10.63a1.56 1.56 0 0 1-.995.453M5.057 9.315v4.059c0 .139.114.255.256.255h1.77c.14 0 .256-.113.256-.255v-2.39a1.54 1.54 0 0 1-1.065-.452zM8.221 10.732v2.642c0 .139.114.255.256.255h1.77c.14 0 .256-.113.256-.255V8.5l-2.029 2.029a1.5 1.5 0 0 1-.253.203M13.496 5.504l-2.11 2.11v5.76c0 .139.113.255.255.255h1.77c.14 0 .256-.113.256-.255v-7.71q-.086-.077-.133-.122z"/>
            <path fill="currentColor" d="M14.944 1.765c-.084-.09-.212-.136-.374-.136h-.047l-2.47.116c-.11.006-.261.012-.38.13a.4.4 0 0 0-.09.134c-.122.264.05.436.13.517l.207.209q.214.218.432.432L7.371 8.151 5.133 5.913a.71.71 0 0 0-1.007 0L1.145 8.892a.71.71 0 0 0 0 1.007l.133.133a.71.71 0 0 0 1.007 0L4.628 7.69l2.238 2.238a.71.71 0 0 0 1.01 0l5.62-5.62.636.633c.075.075.18.18.34.18a.4.4 0 0 0 .202-.055.6.6 0 0 0 .122-.09c.125-.125.148-.285.154-.41q.036-.8.075-1.605l.038-.804q.015-.252-.119-.392"/>
          </svg>
          <span>Perps</span>
        </button>
        <button className="flex items-center gap-2 px-3 py-2 text-sm font-semibold text-zinc-500 transition-colors hover:text-white">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 16 16">
            <path fill="currentColor" fillRule="evenodd" d="M8.002 1.25a6.75 6.75 0 0 1 5.25 2.505V3.2a.75.75 0 0 1 1.5 0v2.04a1.11 1.11 0 0 1-1.11 1.11h-2.04a.75.75 0 0 1 0-1.5h.602a5.25 5.25 0 0 0-4.202-2.1 5.25 5.25 0 0 0-5.224 4.724.75.75 0 1 1-1.493-.148A6.75 6.75 0 0 1 8.002 1.25m6.074 6.604a.75.75 0 0 1 .672.82 6.75 6.75 0 0 1-12.116 3.377v.749a.6.6 0 1 1-1.2 0v-2.04a.96.96 0 0 1 .543-.865A.747.747 0 0 1 2.98 9.8h1.452a.6.6 0 1 1 0 1.2h-.71a5.25 5.25 0 0 0 9.534-2.474.75.75 0 0 1 .82-.672m-5.83-.616a8 8 0 0 1 .867.396c1.001.554 1.27 1.811.563 2.67-.254.31-.585.519-.973.625-.169.046-.245.135-.236.31q.005.166 0 .333l-.002.185c0 .153-.078.236-.23.239q-.278.007-.556.002c-.164-.004-.24-.096-.242-.255l-.001-.189-.002-.19c-.003-.277-.012-.288-.28-.331-.341-.055-.677-.132-.99-.284-.247-.121-.273-.182-.202-.44q.078-.288.165-.573c.07-.22.129-.248.332-.142.347.18.716.28 1.1.328.25.032.493.007.723-.094.43-.188.495-.685.133-.984a1.7 1.7 0 0 0-.41-.242 11 11 0 0 0-.362-.15c-.26-.105-.52-.21-.763-.355-.576-.346-.943-.82-.9-1.522.048-.793.497-1.289 1.224-1.553l.016-.006c.285-.103.287-.104.288-.412v-.138q0-.09.002-.181c.007-.236.046-.278.28-.284h.219c.251 0 .377 0 .44.063.063.062.063.187.064.437.002.356.002.357.355.412q.405.066.78.233c.137.06.19.157.147.302l-.068.235q-.059.209-.124.414c-.066.204-.13.233-.325.14a2.5 2.5 0 0 0-1.236-.245 1 1 0 0 0-.329.067c-.373.164-.434.577-.115.832.161.128.345.221.537.3z" clipRule="evenodd"/>
          </svg>
          <span>Earn</span>
        </button>
        <button className="flex items-center gap-2 px-3 py-2 text-sm font-semibold text-zinc-500 transition-colors hover:text-white">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 16 16">
            <path fill="currentColor" d="M12.494 6.39a.29.29 0 0 0-.255-.148H8.4l.644-4.917a.295.295 0 0 0-.54-.192L3.836 9.312a.293.293 0 0 0 .25.446h3.78l-.51 4.924a.295.295 0 0 0 .543.18l4.591-8.177a.29.29 0 0 0 .003-.295"/>
          </svg>
          <span>Token</span>
        </button>
      </nav>

      {/* Right side controls */}
      <div className="flex items-center gap-2">
        {/* Wallet button */}
        {!ready ? (
          <button
            disabled
            className="rounded-lg bg-linear-to-r from-yellow-400 via-yellow-500 to-green-500 px-5 py-2 text-xs font-semibold leading-5 text-black shadow-lg"
          >
            Loading...
          </button>
        ) : authenticated ? (
          <div className="flex items-center gap-2">
            <div className="rounded-lg bg-[#111111] px-3 py-1.5 shadow-sm">
              <span className="text-xs font-medium text-zinc-400">
                {user?.wallet?.address?.slice(0, 6)}...
                {user?.wallet?.address?.slice(-4)}
              </span>
            </div>
            <button
              onClick={logout}
              className="rounded-lg bg-[#111111] px-3 py-1.5 text-xs font-medium text-zinc-400 shadow-sm transition-colors hover:bg-[#1a1a1a] hover:text-white"
            >
              Disconnect
            </button>
          </div>
        ) : (
          <button
            onClick={login}
            className="rounded-lg bg-linear-to-r from-yellow-400 via-yellow-500 to-green-500 px-5 py-2 text-xs font-semibold leading-5 text-black shadow-lg transition-all hover:shadow-xl"
          >
            Get Started!
          </button>
        )}

        {/* Settings button (3 dots) */}
        <button className="flex h-8 w-8 items-center justify-center rounded-full bg-[#111111] shadow-sm transition-colors hover:bg-white/10 duration-300">
          <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" fill="none" viewBox="0 0 24 24">
            <circle cx="12" cy="5" r="2" fill="rgba(255,255,255,0.4)"/>
            <circle cx="12" cy="12" r="2" fill="rgba(255,255,255,0.4)"/>
            <circle cx="12" cy="19" r="2" fill="rgba(255,255,255,0.4)"/>
          </svg>
        </button>
      </div>
    </header>
  );
}
