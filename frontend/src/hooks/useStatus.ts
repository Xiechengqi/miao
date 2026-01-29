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
      if (status.running) {
        await api.stopService();
      } else {
        await api.startService();
      }
      await refreshStatus();
      addToast({
        type: "success",
        message: `服务${status.running ? "已停止" : "已启动"}`,
      });
    } catch (error) {
      let errorMessage = "操作失败";
      if (error instanceof Error) {
        // 处理常见的 sing-box 启动错误，提供更友好的提示
        const msg = error.message;
        errorMessage = msg;

        // 如果是内部错误，尝试提供更友好的提示
        if (msg.includes("Internal Server Error")) {
          errorMessage = "服务启动失败，请查看服务器日志获取详细信息";
        }
      }
      addToast({
        type: "error",
        message: errorMessage,
      });
    } finally {
      setLoading(false);
    }
  }, [status.running, setLoading, addToast, refreshStatus]);

  const checkDnsNow = useCallback(async () => {
    setLoading(true, "dns-check");
    try {
      await api.checkDns();
      await refreshDnsStatus();
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
  }, [refreshDnsStatus, setLoading, addToast]);

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
