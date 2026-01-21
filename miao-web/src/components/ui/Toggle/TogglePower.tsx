"use client";

import { cn } from "@/lib/utils";
import { Power } from "lucide-react";
import { useEffect, useState } from "react";

interface TogglePowerProps {
  running: boolean;
  loading?: boolean;
  onToggle: () => void;
  size?: "md" | "lg";
  className?: string;
}

const sizeStyles = {
  md: "w-12 h-12 text-xl",
  lg: "w-16 h-16 text-2xl",
};

export function TogglePower({ running, loading, onToggle, size = "md", className }: TogglePowerProps) {
  const [pulsing, setPulsing] = useState(false);

  useEffect(() => {
    if (running) {
      const interval = setInterval(() => {
        setPulsing(true);
        setTimeout(() => setPulsing(false), 200);
      }, 2000);
      return () => clearInterval(interval);
    }
  }, [running]);

  return (
    <button
      onClick={onToggle}
      disabled={loading}
      className={cn(
        "rounded-full flex items-center justify-center",
        "border-2 border-transparent",
        "transition-all duration-300 ease-bezier(0.4, 0, 0.2, 1)",
        "relative overflow-hidden",
        running
          ? "bg-gradient-to-br from-red-500 to-red-600 shadow-lg shadow-red-500/50"
          : "bg-gradient-to-br from-emerald-500 to-emerald-600 shadow-lg shadow-emerald-500/50",
        sizeStyles[size],
        (loading) && "opacity-50 cursor-not-allowed",
        !loading && "hover:scale-108 cursor-pointer",
        className
      )}
      title={running ? "点击停止服务" : "点击启动服务"}
    >
      <Power
        className={cn(
          "transition-transform duration-200",
          pulsing && "scale-110"
        )}
      />
      {running && (
        <span className="absolute inset-0 rounded-full animate-ping bg-red-500/30" />
      )}
      {loading && (
        <span className="absolute inset-0 flex items-center justify-center">
          <span className="w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
        </span>
      )}
    </button>
  );
}
