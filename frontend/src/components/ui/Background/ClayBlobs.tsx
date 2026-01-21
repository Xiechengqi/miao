"use client";

import { cn } from "@/lib/utils";

/**
 * Atmospheric gradient background blobs for Corporate Trust design
 * Large blurred gradient orbs create dimensional depth
 */
export function ClayBlobs() {
  return (
    <div className="fixed inset-0 overflow-hidden pointer-events-none -z-10">
      {/* Top-left indigo blob */}
      <div
        className={cn(
          "absolute w-[600px] h-[600px]",
          "-top-48 -left-48",
          "rounded-full",
          "bg-gradient-to-br from-indigo-500/30 to-violet-500/20",
          "blur-3xl",
          "animate-[pulse_8s_ease-in-out_infinite]"
        )}
      />

      {/* Top-right violet blob */}
      <div
        className={cn(
          "absolute w-[500px] h-[500px]",
          "-top-32 -right-32",
          "rounded-full",
          "bg-gradient-to-bl from-violet-500/25 to-indigo-500/15",
          "blur-3xl",
          "animate-[pulse_10s_ease-in-out_infinite]",
          "opacity-80"
        )}
      />

      {/* Bottom-left indigo accent */}
      <div
        className={cn(
          "absolute w-[450px] h-[450px]",
          "bottom-0 -left-32",
          "rounded-full",
          "bg-gradient-to-tr from-indigo-600/20 to-violet-600/10",
          "blur-3xl"
        )}
      />

      {/* Bottom-right soft violet glow */}
      <div
        className={cn(
          "absolute w-[400px] h-[400px]",
          "bottom-12 -right-24",
          "rounded-full",
          "bg-gradient-to-tl from-violet-400/15 to-indigo-400/10",
          "blur-3xl",
          "animate-[pulse_12s_ease-in-out_infinite]"
        )}
      />
    </div>
  );
}
