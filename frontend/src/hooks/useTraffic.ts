"use client";

import { useEffect, useRef, useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { getTrafficWsUrl } from "@/lib/api";
import { TrafficData } from "@/types/api";

export function useTraffic() {
  const { traffic, setTraffic, status } = useStore();
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectAttemptsRef = useRef(0);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    try {
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
        wsRef.current = null;
        // Don't auto-reconnect here, let the effect handle it based on status
      };

      ws.onerror = () => {
        // Silently handle error, will be closed and effect will decide reconnect
      };

      wsRef.current = ws;
    } catch {
      // If no token is available, don't attempt to connect
    }
  }, [setTraffic]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    reconnectAttemptsRef.current = 0;
  }, []);

  // Connect/disconnect based on sing-box running status
  useEffect(() => {
    if (status.running) {
      // sing-box is running, connect if not connected
      if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
        connect();
      }
    } else {
      // sing-box is not running, disconnect and reset
      disconnect();
      setTraffic({ up: 0, down: 0 });
      if (typeof document !== "undefined") {
        document.title = "Miao";
      }
    }

    return () => disconnect();
  }, [status.running, connect, disconnect, setTraffic]);

  // Reconnect on connection loss when sing-box is running
  useEffect(() => {
    if (!status.running) return;

    const checkConnection = setInterval(() => {
      if (status.running && (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN)) {
        connect();
      }
    }, 5000);

    return () => clearInterval(checkConnection);
  }, [status.running, connect]);

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
