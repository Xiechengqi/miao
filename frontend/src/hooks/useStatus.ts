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

  const checkDnsNow = useCallback(async () => {
    try {
      const data = await api.getDnsStatus();
      setDnsStatus(data);
    } catch (error) {
      console.error("Failed to fetch DNS status:", error);
    }
  }, [setDnsStatus]);

  const toggleService = useCallback(async () => {
    const needsRestart = status.running && !!status.pending_restart;
    const action = needsRestart ? "restart" : status.running ? "stop" : "start";
    setLoading(true, action);

    try {
      if (needsRestart) {
        await api.restartService();
      } else if (status.running) {
        await api.stopService();
      } else {
        await api.startService();
      }
      await refreshStatus();
      addToast({
        type: "success",
        message: needsRestart
          ? "服务已重启，配置已生效"
          : `服务${status.running ? "已停止" : "已启动"}`,
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
  }, [status.running, status.pending_restart, setLoading, addToast, refreshStatus]);

  const switchDnsActive = useCallback(async (name: string) => {
    setLoading(true, "dns-switch");
    try {
      await api.switchDns(name);
      // 刷新 DNS 状态
      const data = await api.getDnsStatus();
      setDnsStatus(data);
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
  }, [setLoading, addToast, setDnsStatus]);

  return {
    status,
    dnsStatus,
    loading,
    loadingAction,
    refreshStatus,
    toggleService,
    switchDnsActive,
    checkDnsNow,
  };
}
