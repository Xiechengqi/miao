"use client";

import { useEffect, useState } from "react";
import { useRouter, usePathname } from "next/navigation";
import Link from "next/link";
import { Button, ClayBlobs, Input, Modal, ToastContainer } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { useLogs, useTraffic } from "@/hooks";
import { api } from "@/lib/api";
import { VersionInfo } from "@/types/api";
import {
  Share2,
  Terminal,
  Monitor,
  AppWindow,
  FileText,
  LogOut,
  Menu,
  X,
  Box,
  Archive,
  CloudDownload,
  KeyRound,
  Server,
  HelpCircle,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useOnboarding } from "@/components/onboarding";

const navItems = [
  { href: "/dashboard/hosts", label: "主机", icon: Server },
  { href: "/dashboard/proxies", label: "代理", icon: Share2 },
  { href: "/dashboard/tunnels", label: "穿透", icon: Box },
  { href: "/dashboard/terminals", label: "终端", icon: Terminal },
  { href: "/dashboard/vnc", label: "桌面", icon: Monitor },
  { href: "/dashboard/apps", label: "应用", icon: AppWindow },
  { href: "/dashboard/sync", label: "备份", icon: Archive },
  { href: "/dashboard/logs", label: "日志", icon: FileText },
];

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const router = useRouter();
  const pathname = usePathname();
  const { setAuthenticated, toasts, removeToast, addToast } = useStore();
  const { resetOnboarding } = useOnboarding();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [mounted, setMounted] = useState(false);
  const [versionInfo, setVersionInfo] = useState<VersionInfo | null>(null);
  const [upgrading, setUpgrading] = useState(false);
  const [showPasswordModal, setShowPasswordModal] = useState(false);
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [savingPassword, setSavingPassword] = useState(false);

  useLogs();
  useTraffic();

  useEffect(() => {
    setMounted(true);

    const checkAuth = async () => {
      const token = localStorage.getItem("miao_token");
      if (!token) {
        router.push("/login");
        return;
      }

      try {
        api.setToken(token);
        const info = await api.getVersion().catch(() => null);
        setVersionInfo(info);
      } catch {
        localStorage.removeItem("miao_token");
        router.push("/login");
      }
    };

    checkAuth();
  }, [router, setAuthenticated]);

  const handleLogout = () => {
    localStorage.removeItem("miao_token");
    api.clearToken();
    setAuthenticated(false);
    router.push("/login");
  };

  const waitForRestart = async () => {
    const token = localStorage.getItem("miao_token");
    for (let i = 0; i < 30; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      try {
        const res = await fetch("/api/status", {
          headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        });
        if (res.ok || res.status === 401) {
          window.location.reload();
          return;
        }
      } catch {
        // ignore
      }
    }
    setUpgrading(false);
    addToast({ type: "error", message: "更新后未检测到服务恢复，请稍后手动刷新" });
  };

  const handleUpgrade = async () => {
    if (upgrading) return;
    if (!confirm("确定要强制更新到最新版本吗？\n更新过程中服务将短暂中断。")) {
      return;
    }
    setUpgrading(true);
    addToast({ type: "info", message: "正在下载更新..." });
    try {
      await api.upgrade();
      addToast({ type: "success", message: "更新成功，等待服务重启..." });
      await waitForRestart();
    } catch (error) {
      setUpgrading(false);
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "更新请求失败",
      });
    }
  };

  const handleOpenPasswordModal = () => {
    setNewPassword("");
    setConfirmPassword("");
    setShowPasswordModal(true);
  };

  const handleUpdatePassword = async () => {
    if (savingPassword) return;
    if (newPassword.trim().length < 4) {
      addToast({ type: "error", message: "密码至少 4 位" });
      return;
    }
    if (newPassword !== confirmPassword) {
      addToast({ type: "error", message: "两次输入的密码不一致" });
      return;
    }
    setSavingPassword(true);
    try {
      await api.updatePassword(newPassword.trim());
      addToast({ type: "success", message: "密码已更新" });
      setShowPasswordModal(false);
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "更新密码失败",
      });
    } finally {
      setSavingPassword(false);
    }
  };

  if (!mounted) {
    return (
      <div className="min-h-screen bg-slate-50 flex items-center justify-center">
        <ClayBlobs />
        <div className="text-center">
          <div className="w-12 h-12 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
          <p className="mt-4 text-slate-500">加载中...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-slate-50">
      <ClayBlobs />

      {/* Toast Container */}
      <ToastContainer toasts={toasts} onClose={removeToast} />

      {/* Mobile sidebar backdrop */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 bg-black/20 backdrop-blur-sm z-40 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={cn(
          "fixed top-0 left-0 h-full w-64 bg-white/70 backdrop-blur-xl shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] z-50",
          "transform transition-transform duration-300 ease-out",
          "lg:translate-x-0",
          sidebarOpen ? "translate-x-0" : "-translate-x-full"
        )}
      >
        <div className="flex flex-col h-full">
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-slate-200/10">
            <Link
              href="/dashboard"
              className="flex items-center gap-2"
            >
              <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-[#A78BFA] to-[#7C3AED] flex items-center justify-center shadow-[0_4px_14px_0_rgba(79,70,229,0.3)]">
                <Box className="w-5 h-5 text-white" />
              </div>
              <span
                className="text-xl font-black"
              >
                Miao
              </span>
            </Link>
            <button
              onClick={() => setSidebarOpen(false)}
              className="lg:hidden p-2 rounded-full hover:bg-slate-500/10"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          {/* Navigation */}
          <nav className="flex-1 p-4 space-y-1 overflow-y-auto" data-onboarding="nav-section">
            {navItems.map((item) => {
              const isActive = pathname === item.href ||
                (item.href !== "/dashboard" && pathname.startsWith(item.href));
              const isProxies = item.href === "/dashboard/proxies";

              return (
                <Link
                  key={item.href}
                  href={item.href}
                  data-onboarding={isProxies ? "nav-proxies" : undefined}
                  className={cn(
                    "flex items-center gap-3 px-4 py-3 rounded-lg",
                    "transition-all duration-200",
                    isActive
                      ? "bg-gradient-to-br from-[#A78BFA]/20 to-[#7C3AED]/10 text-indigo-600 font-semibold shadow-sm"
                      : "text-slate-500 hover:bg-indigo-50 hover:text-indigo-600 hover:shadow-sm"
                  )}
                >
                  <item.icon className="w-5 h-5" />
                  <span>{item.label}</span>
                </Link>
              );
            })}
          </nav>

          {/* Footer */}
          <div className="p-4 border-t border-slate-200/10 space-y-1">
            <button
              onClick={handleUpgrade}
              disabled={upgrading}
              className={cn(
                "flex items-center gap-3 w-full px-4 py-3 rounded-lg transition-colors cursor-pointer",
                "text-indigo-600 hover:bg-indigo-50/70",
                upgrading && "opacity-60 cursor-not-allowed"
              )}
            >
              <CloudDownload className="w-5 h-5" />
              <span className="font-semibold">
                {upgrading ? "更新中..." : "强制更新"}
              </span>
            </button>
            <button
              onClick={handleOpenPasswordModal}
              className="flex items-center gap-3 w-full px-4 py-3 rounded-lg text-slate-600 hover:bg-indigo-50 hover:text-indigo-600 hover:shadow-sm transition-all cursor-pointer"
            >
              <KeyRound className="w-5 h-5" />
              <span>修改密码</span>
            </button>
            <button
              onClick={resetOnboarding}
              className="flex items-center gap-3 w-full px-4 py-3 rounded-lg text-slate-600 hover:bg-indigo-50 hover:text-indigo-600 hover:shadow-sm transition-all cursor-pointer"
            >
              <HelpCircle className="w-5 h-5" />
              <span>使用引导</span>
            </button>
            <button
              onClick={handleLogout}
              className="flex items-center gap-3 w-full px-4 py-3 rounded-lg text-red-600 hover:bg-red-50 hover:shadow-sm transition-all cursor-pointer"
            >
              <LogOut className="w-5 h-5" />
              <span>退出登录</span>
            </button>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <div className="lg:pl-64">
        {/* Mobile header */}
        <header className="lg:hidden sticky top-0 z-30 bg-slate-50/80 backdrop-blur-xl p-4">
          <div className="flex items-center justify-between">
            <button
              onClick={() => setSidebarOpen(true)}
              className="p-2 rounded-lg bg-white/60 shadow-[0_4px_20px_-2px_rgba(79,70,229,0.1)] hover:shadow-[0_4px_14px_0_rgba(79,70,229,0.3)] transition-shadow"
            >
              <Menu className="w-5 h-5" />
            </button>
            <span
              className="text-lg font-black"
            >
              Miao
            </span>
            <div className="w-10" />
          </div>
        </header>

        {/* Page content */}
        <main className="p-4 lg:p-8 max-w-7xl mx-auto">
          {children}
        </main>
      </div>

      <Modal
        isOpen={showPasswordModal}
        onClose={() => setShowPasswordModal(false)}
        title="修改登录密码"
      >
        <div className="space-y-4">
          <Input
            type="password"
            placeholder="新密码（至少 4 位）"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
          />
          <Input
            type="password"
            placeholder="确认新密码"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
          />
          <div className="flex justify-end gap-3">
            <Button
              variant="secondary"
              onClick={() => setShowPasswordModal(false)}
              disabled={savingPassword}
            >
              取消
            </Button>
            <Button loading={savingPassword} onClick={handleUpdatePassword}>
              保存
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
