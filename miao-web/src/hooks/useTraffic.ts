"use client";

import { useEffect, useRef, useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { getTrafficWsUrl } from "@/lib/api";
import { TrafficData } from "@/types/api";

export function useTraffic() {
  const { traffic, setTraffic } = useStore();
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectAttemptsRef = useRef(0);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    const wsUrl = getTrafficWsUrl();
    const ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      reconnectAttemptsRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const data: TrafficData = JSON.parse(event.data);
        setTraffic(data);

        // Update page title with current speed
        if (typeof document !== "undefined") {
          const upSpeed = formatSpeed(data.up);
          const downSpeed = formatSpeed(data.down);
          document.title = `↑${upSpeed} ↓${downSpeed}`;
        }
      } catch (error) {
        console.error("Failed to parse traffic data:", error);
      }
    };

    ws.onclose = () => {
      // Attempt to reconnect after delay
      const delay = Math.min(1000 * Math.pow(2, reconnectAttemptsRef.current), 30000);
      reconnectTimeoutRef.current = setTimeout(() => {
        reconnectAttemptsRef.current++;
        connect();
      }, delay);
    };

    ws.onerror = (error) => {
      console.error("Traffic WebSocket error:", error);
    };

    wsRef.current = ws;
  }, [setTraffic]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
    }
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

  return {
    traffic,
    connect,
    disconnect,
  };
}

function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytesPerSecond) / Math.log(k));
  return `${(bytesPerSecond / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}
