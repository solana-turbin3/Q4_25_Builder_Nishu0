"use client";

import { useEffect, useRef } from "react";

interface TradingViewChartProps {
  symbol: string;
}

export function TradingViewChart({ symbol }: TradingViewChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const script = document.createElement("script");
    script.src = "https://s3.tradingview.com/tv.js";
    script.async = true;
    script.onload = () => {
      if (typeof (window as any).TradingView !== "undefined") {
        new (window as any).TradingView.widget({
          container_id: containerRef.current?.id || "tradingview_chart",
          autosize: true,
          symbol: symbol,
          interval: "15",
          timezone: "Etc/UTC",
          theme: "dark",
          style: "1",
          locale: "en",
          toolbar_bg: "#0a0a0a",
          enable_publishing: false,
          hide_top_toolbar: false,
          hide_legend: false,
          save_image: false,
          backgroundColor: "#0a0a0a",
          gridColor: "#111111",
          allow_symbol_change: false,
          studies: [],
          show_popup_button: false,
          popup_width: "1000",
          popup_height: "650",
          support_host: "https://www.tradingview.com",
          overrides: {
            "paneProperties.background": "#0a0a0a",
            "paneProperties.backgroundType": "solid",
            "paneProperties.vertGridProperties.color": "#111111",
            "paneProperties.horzGridProperties.color": "#111111",
            "scalesProperties.textColor": "#71717a",
            "scalesProperties.lineColor": "#18181b",
            "mainSeriesProperties.candleStyle.upColor": "#06b6d4",
            "mainSeriesProperties.candleStyle.downColor": "#52525b",
            "mainSeriesProperties.candleStyle.borderUpColor": "#06b6d4",
            "mainSeriesProperties.candleStyle.borderDownColor": "#52525b",
            "mainSeriesProperties.candleStyle.wickUpColor": "#06b6d4",
            "mainSeriesProperties.candleStyle.wickDownColor": "#52525b",
          },
        });
      }
    };
    document.head.appendChild(script);

    return () => {
      if (script.parentNode) {
        script.parentNode.removeChild(script);
      }
    };
  }, [symbol]);

  return (
    <div
      ref={containerRef}
      id={`tradingview_chart_${symbol}`}
      className="h-full w-full"
    />
  );
}

