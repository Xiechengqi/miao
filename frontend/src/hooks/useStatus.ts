"use client";

import { useCallback } from "react";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";

export function useStatus() {
  const { status, dnsStatus, loading, loadingAction, setStatus, setDnsStatus, setLoading, addToast } = useStore();

  const refreshStatus = useCallback(async () => {
    try {
      const data = await api.getStatus();
      setStatus(data);
    } catch (error) {
      console.error("Failed to fetch status:", error);
    }
  }, [setStatus]);

  const refreshDnsStatus = useCallback(async () => {
    try {
      const data = await api.getDnsStatus();
      setDnsStatus(data);
    } catch (error) {
      console.error("Failed to fetch DNS status:", error);
    }
  }, [setDnsStatus]);

  const toggleService = useCallback(async () => {
    const action = status.running ? "stop" : "start";
    setLoading(true, action);

    try {
      const data = status.running ? await api.stopService() : await api.startService();
      setStatus(data);
      addToast({
        type: "success",
        message: `服务${status.running ? "已停止" : "已启动"}`,
      });
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "操作失败",
      });
    } finally {
      setLoading(false);
    }
  }, [status.running, setStatus, setLoading, addToast]);

  const checkDnsNow = useCallback(async () => {
    setLoading(true, "dns-check");
    try {
      const data = await api.checkDns();
      setDnsStatus(data);
      addToast({
        type: "success",
        message: "DNS 检测完成",
      });
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "DNS 检测失败",
      });
    } finally {
      setLoading(false);
    }
  }, [setDnsStatus, setLoading, addToast]);

  const switchDnsActive = useCallback(async (name: string) => {
    setLoading(true, "dns-switch");
    try {
      await api.switchDns(name);
      await refreshDnsStatus();
      addToast({
        type: "success",
        message: `已切换 DNS 到 ${name}`,
      });
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "DNS 切换失败",
      });
    } finally {
      setLoading(false);
    }
  }, [setLoading, addToast, refreshDnsStatus]);

  return {
    status,
    dnsStatus,
    loading,
    loadingAction,
    refreshStatus,
    refreshDnsStatus,
    toggleService,
    checkDnsNow,
    switchDnsActive,
  };
}
