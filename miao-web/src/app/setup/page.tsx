"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Input, ClayBlobs } from "@/components/ui";
import { api } from "@/lib/api";
import { Settings, AlertCircle, Info } from "lucide-react";

export default function SetupPage() {
  const router = useRouter();
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");

    if (password !== confirmPassword) {
      setError("两次输入的密码不一致");
      return;
    }

    if (password.length < 4) {
      setError("密码长度至少为4位");
      return;
    }

    setLoading(true);

    try {
      await api.setup(password);
      await api.login(password);
      router.push("/dashboard");
    } catch (err) {
      setError(err instanceof Error ? err.message : "初始化失败");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-slate-50 flex items-center justify-center p-4">
      <ClayBlobs />

      <div className="relative w-full max-w-md">
        <div className="bg-white rounded-xl shadow-[0_20px_50px_-12px_rgba(79,70,229,0.25)] border border-slate-100 p-8">
          {/* Header */}
          <div className="text-center mb-8">
            <div className="inline-flex items-center justify-center w-16 h-16 rounded-xl bg-gradient-to-r from-indigo-600 to-violet-600 shadow-[0_4px_14px_0_rgba(79,70,229,0.3)] mb-4">
              <Settings className="w-8 h-8 text-white" />
            </div>
            <h1 className="text-3xl font-extrabold text-slate-900">
              初始化设置
            </h1>
            <p className="mt-2 text-slate-500">创建您的管理密码</p>
          </div>

          {/* Form */}
          <form onSubmit={handleSubmit} className="space-y-6">
            <div>
              <Input
                type="password"
                placeholder="设置登录密码"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={loading}
                autoFocus
              />
            </div>

            <div>
              <Input
                type="password"
                placeholder="确认密码"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                disabled={loading}
              />
            </div>

            {error && (
              <div className="flex items-center gap-2 p-3 rounded-lg bg-red-50 border border-red-100 text-red-700 text-sm">
                <AlertCircle className="w-4 h-4 flex-shrink-0" />
                <span>{error}</span>
              </div>
            )}

            {/* Info box */}
            <div className="flex items-start gap-3 p-4 rounded-lg bg-indigo-50 border border-indigo-100">
              <Info className="w-5 h-5 text-indigo-600 flex-shrink-0 mt-0.5" />
              <div className="text-sm text-slate-600">
                <p>初始化完成后，将订阅文件放入 <code className="px-1.5 py-0.5 rounded bg-white text-slate-900 border border-slate-200">sub/</code> 目录，再在面板中点击"重载"。</p>
                <p className="mt-2">也可使用 <code className="px-1.5 py-0.5 rounded bg-white text-slate-900 border border-slate-200">--sub &lt;GIT_URL&gt;</code> 启动参数。</p>
              </div>
            </div>

            <Button
              type="submit"
              className="w-full"
              loading={loading}
              disabled={!password || !confirmPassword || loading}
            >
              完成初始化
            </Button>
          </form>
        </div>
      </div>
    </div>
  );
}
