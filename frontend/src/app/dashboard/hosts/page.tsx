"use client";

import { useEffect, useMemo, useState } from "react";
import { Card, Button, Badge, Modal, Input, ConfirmModal } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { useProxies } from "@/hooks";
import { api } from "@/lib/api";
import { Host, HostAuthType } from "@/types/api";
import { Plus, Trash2, Pencil, Zap, Server } from "lucide-react";

const defaultHostForm = {
  name: "",
  host: "",
  port: "22",
  username: "root",
  auth_type: "private_key_path" as HostAuthType,
  password: "",
  private_key_path: "",
  private_key_passphrase: "",
};

const HOST_TEST_STORAGE_KEY = "miao_host_test_results";

type HostTestResult = {
  success: boolean;
};

export default function HostsPage() {
  const { setLoading, loading, addToast } = useStore();
  const { fetchProxies } = useProxies();

  const [hosts, setHosts] = useState<Host[]>([]);
  const [hostsLoaded, setHostsLoaded] = useState(false);
  const [testingHostId, setTestingHostId] = useState<string | null>(null);
  const [deletingHostId, setDeletingHostId] = useState<string | null>(null);
  const [pendingDeleteHost, setPendingDeleteHost] = useState<Host | null>(null);
  const [hostTestResults, setHostTestResults] = useState<Record<string, HostTestResult>>({});
  const [defaultPrivateKeyPath, setDefaultPrivateKeyPath] = useState<string | null>(null);
  const [autoFilledKeyPath, setAutoFilledKeyPath] = useState(false);
  const [defaultKeyPathLoaded, setDefaultKeyPathLoaded] = useState(false);

  const [showModal, setShowModal] = useState(false);
  const [editingHostId, setEditingHostId] = useState<string | null>(null);
  const [formData, setFormData] = useState(defaultHostForm);
  const [testingForm, setTestingForm] = useState(false);

  const canSubmit = useMemo(() => {
    if (!formData.host.trim() || !formData.username.trim()) return false;
    const port = Number(formData.port);
    if (!port || port < 1 || port > 65535) return false;
    if (formData.auth_type === "password") {
      return formData.password.trim().length > 0;
    }
    if (formData.auth_type === "private_key_path") {
      return formData.private_key_path.trim().length > 0;
    }
    return false;
  }, [formData]);

  const safeGetItem = <T,>(key: string, fallback: T): T => {
    try {
      const item = localStorage.getItem(key);
      if (!item) return fallback;
      return JSON.parse(item) as T;
    } catch {
      console.warn(`Failed to read ${key} from localStorage`);
      return fallback;
    }
  };

  useEffect(() => {
    loadHosts();
  }, []);

  useEffect(() => {
    api.getHostDefaultKeyPath()
      .then((path) => {
        if (path) setDefaultPrivateKeyPath(path);
      })
      .catch(() => {})
      .finally(() => setDefaultKeyPathLoaded(true));
  }, []);

  useEffect(() => {
    const savedResults = safeGetItem<Record<string, HostTestResult>>(HOST_TEST_STORAGE_KEY, {});
    if (Object.keys(savedResults).length > 0) {
      setHostTestResults(savedResults);
    }
  }, []);

  useEffect(() => {
    if (!showModal) return;
    if (formData.auth_type !== "private_key_path") return;
    if (formData.private_key_path.trim() !== "") return;
    if (!defaultPrivateKeyPath) return;
    setFormData((prev) => {
      if (prev.private_key_path.trim() !== "") return prev;
      return { ...prev, private_key_path: defaultPrivateKeyPath };
    });
    setAutoFilledKeyPath(true);
  }, [defaultPrivateKeyPath, formData.auth_type, formData.private_key_path, showModal]);

  useEffect(() => {
    if (!hostsLoaded) return;
    const allowedIds = new Set(hosts.map((host) => host.id));
    const filteredResults = Object.fromEntries(
      Object.entries(hostTestResults).filter(([id]) => allowedIds.has(id))
    );
    if (Object.keys(filteredResults).length !== Object.keys(hostTestResults).length) {
      setHostTestResults(filteredResults);
    }
  }, [hosts, hostsLoaded, hostTestResults]);

  useEffect(() => {
    try {
      if (Object.keys(hostTestResults).length === 0) {
        localStorage.removeItem(HOST_TEST_STORAGE_KEY);
        return;
      }
      localStorage.setItem(HOST_TEST_STORAGE_KEY, JSON.stringify(hostTestResults));
    } catch (error) {
      console.warn("Failed to save host test results:", error);
    }
  }, [hostTestResults]);

  const loadHosts = async () => {
    try {
      const data = await api.getHosts();
      setHosts(data);
    } catch (error) {
      addToast({ type: "error", message: "获取主机列表失败" });
    } finally {
      setHostsLoaded(true);
    }
  };

  const resetForm = () => {
    setFormData(defaultHostForm);
    setAutoFilledKeyPath(false);
  };

  const openModal = async (host?: Host) => {
    if (host) {
      try {
        const detail = await api.getHost(host.id);
        setEditingHostId(detail.id);
        setFormData({
          name: detail.name || "",
          host: detail.host || "",
          port: detail.port?.toString() || "22",
          username: detail.username || "root",
          auth_type: detail.auth_type || "password",
          password: detail.password || "",
          private_key_path: detail.private_key_path || "",
          private_key_passphrase: detail.private_key_passphrase || "",
        });
        setAutoFilledKeyPath(false);
      } catch (error) {
        addToast({ type: "error", message: error instanceof Error ? error.message : "获取主机失败" });
        return;
      }
    } else {
      setEditingHostId(null);
      resetForm();
    }
    setShowModal(true);
  };

  const closeModal = () => {
    setShowModal(false);
    setEditingHostId(null);
    resetForm();
  };

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setLoading(true, "host-save");
    try {
      const payload: Partial<Host> = {
        name: formData.name.trim() || null,
        host: formData.host.trim(),
        port: Number(formData.port) || 22,
        username: formData.username.trim(),
        auth_type: formData.auth_type,
      };

      if (formData.auth_type === "password") {
        if (formData.password.trim()) {
          payload.password = formData.password.trim();
        }
      } else if (formData.auth_type === "private_key_path") {
        payload.private_key_path = formData.private_key_path.trim();
        if (formData.private_key_passphrase.trim()) {
          payload.private_key_passphrase = formData.private_key_passphrase.trim();
        }
      }

      if (editingHostId) {
        await api.updateHost(editingHostId, payload);
        addToast({ type: "success", message: "主机已更新" });
      } else {
        await api.createHost(payload as Omit<Host, "id">);
        addToast({ type: "success", message: "主机已添加" });
      }
      closeModal();
      loadHosts();
      // 刷新代理节点列表，使新主机作为 SSH 节点可用
      // silent 模式：sing-box 未运行时静默失败，避免用户困惑
      fetchProxies(true);
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "保存失败" });
    } finally {
      setLoading(false);
    }
  };

  const handleTest = async (host: Host) => {
    setTestingHostId(host.id);
    try {
      await api.testHost(host.id);
      setHostTestResults((prev) => ({ ...prev, [host.id]: { success: true } }));
      addToast({ type: "success", message: "连接测试成功" });
    } catch (error) {
      setHostTestResults((prev) => ({ ...prev, [host.id]: { success: false } }));
      addToast({ type: "error", message: error instanceof Error ? error.message : "连接测试失败" });
    } finally {
      setTestingHostId(null);
    }
  };

  const handleFormTest = async () => {
    if (!canSubmit) return;
    setTestingForm(true);
    try {
      await api.testHostConfig({
        name: formData.name.trim() || null,
        host: formData.host.trim(),
        port: Number(formData.port) || 22,
        username: formData.username.trim(),
        auth_type: formData.auth_type,
        password: formData.auth_type === "password" ? formData.password.trim() : undefined,
        private_key_path: formData.auth_type === "private_key_path" ? formData.private_key_path.trim() : undefined,
        private_key_passphrase: formData.auth_type === "private_key_path" ? formData.private_key_passphrase.trim() : undefined,
      });
      addToast({ type: "success", message: "连接测试成功" });
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "连接测试失败" });
    } finally {
      setTestingForm(false);
    }
  };

  const handleDelete = async (host: Host) => {
    const hostName = host.name || host.host;
    setDeletingHostId(host.id);
    try {
      await api.deleteHost(host.id);
      addToast({ type: "success", message: "主机已删除" });
      loadHosts();
      setHostTestResults((prev) => {
        if (!prev[host.id]) return prev;
        const next = { ...prev };
        delete next[host.id];
        return next;
      });
      // 刷新代理节点列表，同步删除 SSH 节点
      // silent 模式：sing-box 未运行时静默失败，避免用户困惑
      fetchProxies(true);
    } catch (error) {
      addToast({ type: "error", message: error instanceof Error ? error.message : "删除失败" });
    } finally {
      setDeletingHostId(null);
    }
  };

  const getAuthTypeLabel = (authType: HostAuthType) => {
    switch (authType) {
      case "password":
        return "密码";
      case "private_key_path":
        return "私钥";
      default:
        return authType;
    }
  };

  return (
    <div className="space-y-8">
      <Card className="p-6" hoverEffect={false}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-2xl font-bold text-slate-900">主机管理</h2>
            <p className="text-slate-500 text-sm mt-1">集中管理 SSH 主机认证配置</p>
          </div>
          <Button onClick={() => void openModal()}>
            <Plus className="w-4 h-4" />
            添加主机
          </Button>
        </div>

        <div className="mt-6">
          {!hostsLoaded ? (
            <div className="text-center py-10">
              <div className="w-10 h-10 border-4 border-indigo-600/30 border-t-indigo-600 rounded-full animate-spin mx-auto" />
              <p className="mt-4 text-slate-500">加载中...</p>
            </div>
          ) : hosts.length === 0 ? (
            <div className="text-center py-8 text-slate-500">
              <Server className="w-12 h-12 mx-auto mb-3 text-slate-300" />
              <p>暂无主机配置</p>
              <p className="text-sm mt-1">添加主机后可在穿透和备份页面中选择使用</p>
            </div>
          ) : (
            <div className="space-y-3">
              {hosts.map((host) => (
                <div
                  key={host.id}
                  className="flex flex-col gap-3 rounded-lg border border-slate-100 bg-white px-4 py-4 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div>
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-semibold text-slate-900">
                        {host.name || host.host}
                      </span>
                      <Badge variant="info">{getAuthTypeLabel(host.auth_type)}</Badge>
                      {hostTestResults[host.id] && (
                        <Badge variant={hostTestResults[host.id].success ? "success" : "error"}>
                          {hostTestResults[host.id].success ? "测试成功" : "测试失败"}
                        </Badge>
                      )}
                    </div>
                    <div className="text-sm text-slate-500 mt-1">
                      {host.username}@{host.host}:{host.port}
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleTest(host)}
                      loading={testingHostId === host.id}
                    >
                      <Zap className="w-4 h-4" />
                      测试
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => void openModal(host)}>
                      <Pencil className="w-4 h-4" />
                      编辑
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setPendingDeleteHost(host)}
                      loading={deletingHostId === host.id}
                    >
                      <Trash2 className="w-4 h-4 text-red-500" />
                      删除
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Card>

      <Modal
        isOpen={showModal}
        onClose={closeModal}
        title={editingHostId ? "编辑主机" : "添加主机"}
        size="lg"
      >
        <div className="space-y-5">
          <Input
            label="名称（可选）"
            placeholder="例如: 生产服务器"
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          />

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            <Input
              label="主机地址"
              placeholder="example.com"
              value={formData.host}
              onChange={(e) => setFormData({ ...formData, host: e.target.value })}
            />
            <Input
              label="端口"
              type="number"
              value={formData.port}
              onChange={(e) => setFormData({ ...formData, port: e.target.value })}
            />
            <Input
              label="用户名"
              placeholder="root"
              value={formData.username}
              onChange={(e) => setFormData({ ...formData, username: e.target.value })}
            />
          </div>

          <div>
            <label className="block text-sm font-semibold text-slate-700 mb-2">认证方式</label>
            <select
              value={formData.auth_type}
              onChange={(e) => setFormData({ ...formData, auth_type: e.target.value as HostAuthType })}
              className="w-full h-11 rounded-lg border border-slate-200 bg-white px-4 text-slate-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 focus:ring-offset-1"
            >
              <option value="password">密码</option>
              <option value="private_key_path">私钥路径</option>
            </select>
          </div>

          {formData.auth_type === "password" && (
            <Input
              label="密码"
              type="password"
              placeholder="请输入密码"
              value={formData.password}
              onChange={(e) => setFormData({ ...formData, password: e.target.value })}
            />
          )}

          {formData.auth_type === "private_key_path" && (
            <>
              <Input
                label="私钥路径"
                placeholder="/root/.ssh/id_ed25519"
                value={formData.private_key_path}
                onChange={(e) => {
                  setFormData({ ...formData, private_key_path: e.target.value });
                  setAutoFilledKeyPath(false);
                }}
              />
              {autoFilledKeyPath && (
                <p className="text-xs text-amber-700">
                  已自动填充默认私钥路径：{defaultPrivateKeyPath}
                </p>
              )}
              {!autoFilledKeyPath
                && defaultKeyPathLoaded
                && !defaultPrivateKeyPath
                && formData.private_key_path.trim().length === 0 && (
                  <p className="text-xs text-red-600">
                    未检测到默认私钥路径，请手动填写。
                  </p>
                )}
              <Input
                label="私钥口令（可选）"
                type="password"
                placeholder="无口令留空"
                value={formData.private_key_passphrase}
                onChange={(e) => setFormData({ ...formData, private_key_passphrase: e.target.value })}
              />
            </>
          )}

          <div className="flex flex-wrap justify-end gap-3 pt-2">
            <Button variant="secondary" onClick={handleFormTest} loading={testingForm} disabled={!canSubmit}>
              测试连接
            </Button>
            <Button variant="secondary" onClick={closeModal}>
              取消
            </Button>
            <Button onClick={handleSubmit} loading={loading} disabled={!canSubmit}>
              保存
            </Button>
          </div>
        </div>
      </Modal>

      <ConfirmModal
        isOpen={pendingDeleteHost !== null}
        onClose={() => setPendingDeleteHost(null)}
        onConfirm={async () => {
          if (!pendingDeleteHost) return;
          await handleDelete(pendingDeleteHost);
          setPendingDeleteHost(null);
        }}
        title="确认删除主机"
        message={pendingDeleteHost ? `确定要删除主机 ${pendingDeleteHost.name || pendingDeleteHost.host} 吗？` : ""}
        variant="danger"
        loading={pendingDeleteHost ? deletingHostId === pendingDeleteHost.id : false}
      />
    </div>
  );
}
