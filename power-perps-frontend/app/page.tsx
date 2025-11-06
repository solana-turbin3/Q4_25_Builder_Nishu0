import { Header } from "@/components/Header";
import { MarketHeader } from "@/components/MarketHeader";
import { PriceInfo } from "@/components/PriceInfo";
import { TradingChart } from "@/components/TradingChart";
import { TradingPanel } from "@/components/TradingPanel";
import { PositionsTable } from "@/components/PositionsTable";
import { Footer } from "@/components/Footer";

export default function Home() {
  return (
    <div className="flex h-screen flex-col bg-black">
      <Header />
      <MarketHeader />
      <PriceInfo />

      <div className="flex flex-1 overflow-hidden">
        <TradingChart />
        <TradingPanel />
      </div>

      <PositionsTable />
      <Footer />
    </div>
  );
}
