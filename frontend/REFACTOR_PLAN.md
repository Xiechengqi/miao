# Miao Web 页面重构规划

## 概述
将概览页面的功能整合到代理页面，精简导航结构。

---

## 一、代理页面重构（/dashboard/proxies）

### 1.1 新增内容（从概览页迁移）

#### Sing-box 状态卡片
**位置**: 页面顶部，标题下方第一个卡片

**内容**:
- 标题: "Sing-box 状态"
- 运行状态指示器（运行中/已停止）
- 电源开关按钮（TogglePower 组件）
- 运行信息：
  - PID (进程 ID)
  - 运行时间 (uptime)
  - DNS 状态（当前活跃 DNS）
  - DNS 刷新按钮
- DNS 候选列表：
  - 下拉选择器（切换 DNS）
  - 健康状态标签（ok/bad/cooldown）

**依赖 Hooks**:
- `useStatus()` - 获取服务状态、DNS 状态
- API 调用: `api.getStatus()`, `api.getDnsStatus()`

---

#### 流量统计卡片
**位置**: Sing-box 状态卡片下方

**内容**:
- 标题: "流量统计"
- 实时上传速度（绿色图标）
- 实时下载速度（蓝色图标）

**依赖 Hooks**:
- `useTraffic()` - WebSocket 实时流量数据
- 使用 `formatSpeed()` 格式化速度

---

### 1.2 页面布局结构（从上到下）

```
1. 页面标题区
   - 标题: "代理节点"
   - 副标题: "管理代理节点和延迟测试"
   - 操作按钮: 刷新、测试全部延迟

2. Sing-box 状态卡片（新增）
   - 运行状态 + 电源开关
   - 运行信息（PID、运行时间）
   - DNS 状态和切换
   - DNS 候选健康状态

3. 流量统计卡片（新增）
   - 上传速度
   - 下载速度

4. 搜索框（保持原有）

5. 代理组列表（保持原有）
   - 按组显示
   - 节点延迟测试
   - 节点切换
```

---

### 1.3 需要修改的文件

**文件**: `src/app/dashboard/proxies/page.tsx`

**新增导入**:
```typescript
// 组件
import { TogglePower, Badge } from "@/components/ui";

// Hooks
import { useStatus, useTraffic } from "@/hooks";

// 图标
import { Activity, Clock, Cpu, Wifi, Zap } from "lucide-react";

// 工具函数
import { formatUptime, formatSpeed } from "@/lib/utils";
```

**新增状态管理**:
```typescript
const { status, dnsStatus, loadingAction, checkDnsNow, switchDnsActive } = useStatus();
const { traffic } = useTraffic();
```

**组件结构调整**:
- 在搜索框之前插入两个新卡片
- 复用概览页的卡片代码结构

---

## 二、概览页面调整（/dashboard）

### 2.1 处理方案选项

#### 方案 A: 重定向到代理页（推荐）
- 访问 `/dashboard` 自动跳转到 `/dashboard/proxies`
- 修改导航菜单，移除"概览"项
- 代理页成为默认首页

**优点**:
- 避免功能重复
- 简化导航结构
- 代码维护更清晰

**实现**:
```typescript
// src/app/dashboard/page.tsx
"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";

export default function DashboardPage() {
  const router = useRouter();

  useEffect(() => {
    router.replace("/dashboard/proxies");
  }, [router]);

  return null;
}
```

**导航菜单调整** (`src/app/dashboard/layout.tsx`):
```typescript
const navItems = [
  { href: "/dashboard/proxies", label: "代理", icon: Share2 },  // 作为首页
  { href: "/dashboard/tunnels", label: "穿透", icon: Box },
  { href: "/dashboard/sync", label: "备份", icon: RefreshCw },  // 改名
  { href: "/dashboard/terminals", label: "终端", icon: Terminal },
  { href: "/dashboard/vnc", label: "桌面", icon: Monitor },
  { href: "/dashboard/apps", label: "应用", icon: AppWindow },
  { href: "/dashboard/logs", label: "日志", icon: FileText },
];
```

---

#### 方案 B: 保留简化版概览页
- 保留概览页，仅显示摘要信息
- 提供快速跳转链接到各功能页

**内容**:
- Sing-box 状态摘要（精简版）
- 各模块快捷入口（卡片式）
- 最近活动/日志摘要

**优点**:
- 提供整体概览视图
- 作为功能导航入口

**缺点**:
- 功能重复
- 增加维护成本

---

### 2.2 推荐方案
**采用方案 A（重定向）**，理由：
1. 避免信息重复展示
2. 用户直接进入最常用的代理管理页
3. 减少页面数量，简化维护
4. 移动端友好（减少导航层级）

---

## 三、同步页面改名（/dashboard/sync）

### 3.1 术语替换映射

| 原术语 | 新术语 |
|--------|--------|
| 订阅同步 | 订阅备份 |
| 同步配置 | 备份配置 |
| 添加同步 | 添加备份 |
| 编辑同步 | 编辑备份 |
| 同步配置已创建 | 备份配置已创建 |
| 同步配置已更新 | 备份配置已更新 |
| 同步配置已删除 | 备份配置已删除 |
| 立即同步 | 立即备份 |
| 同步已触发 | 备份已触发 |
| 同步失败 | 备份失败 |
| 同步间隔 | 备份间隔 |
| 启用自动同步 | 启用自动备份 |
| 暂无同步配置 | 暂无备份配置 |

### 3.2 需要修改的位置

**文件**: `src/app/dashboard/sync/page.tsx`

**修改点**:
1. 页面标题（第 149 行）
2. 页面副标题（第 152 行）
3. 按钮文本（第 155 行）
4. Toast 消息（第 52, 55, 60, 97 行）
5. Modal 标题（第 248 行）
6. 确认对话框（第 92 行）
7. 表单标签（第 273, 316 行）
8. 空状态提示（第 239 行）

**导航菜单调整** (`src/app/dashboard/layout.tsx`):
- 第 29 行: `{ href: "/dashboard/sync", label: "备份", icon: RefreshCw }`

### 3.3 注意事项
- **不修改变量名**：保持 `syncs`, `SyncConfig` 等内部命名不变
- **不修改 API 路径**：后端接口路径保持 `/api/syncs`
- **仅修改用户可见文本**：只改界面显示的中文文本

---

## 四、文件修改清单

| 文件 | 修改内容 | 影响范围 |
|------|---------|---------|
| `src/app/dashboard/page.tsx` | 改为重定向到 `/dashboard/proxies` | 概览页 |
| `src/app/dashboard/proxies/page.tsx` | 新增 Sing-box 状态卡片、流量统计卡片 | 代理页 |
| `src/app/dashboard/sync/page.tsx` | "同步" → "备份" 术语替换 | 同步页 |
| `src/app/dashboard/layout.tsx` | 移除"概览"导航项，"同步"改"备份" | 侧边导航 |

---

## 五、实施步骤（建议顺序）

### 第一步：代理页面增强
1. 复制概览页的 Sing-box 状态卡片代码
2. 复制概览页的流量统计卡片代码
3. 调整代理页面布局，插入新卡片
4. 测试状态切换、DNS 切换、流量显示功能

### 第二步：概览页重定向
1. 修改 `src/app/dashboard/page.tsx` 为重定向组件
2. 测试访问 `/dashboard` 是否正确跳转

### 第三步：导航菜单调整
1. 修改 `src/app/dashboard/layout.tsx` 的 `navItems`
2. 移除"概览"，"同步"改"备份"
3. 测试导航菜单显示和路由跳转

### 第四步：同步页改名
1. 全局搜索替换用户可见的"同步"文本为"备份"
2. 保持内部变量名和 API 不变
3. 测试所有功能是否正常

### 第五步：验证测试
1. Mock 模式测试所有功能
2. 检查响应式布局（移动端）
3. 验证所有交互功能正常

---

## 六、潜在问题和解决方案

### 6.1 代理页面过长
**问题**: 新增两个卡片后页面内容较多

**解决方案**:
- 将状态和流量卡片设计为可折叠（可选）
- 使用 sticky header 固定操作按钮
- 优化卡片高度，使用更紧凑布局

### 6.2 状态同步问题
**问题**: 多个页面可能同时更新状态

**解决方案**:
- 使用 Zustand 全局状态管理
- 已有的 `useStore()` 保证状态一致性
- WebSocket 流量数据自动更新

### 6.3 初始加载性能
**问题**: 代理页面需要加载更多数据

**解决方案**:
- 已有的并行数据加载 (`Promise.all`)
- 显示加载骨架屏
- 流量数据通过 WebSocket 异步加载

---

## 七、Mock 数据验证

### 需要确保 Mock 数据包含：
✅ `mockStatus` - Sing-box 状态
✅ `mockDnsStatus` - DNS 状态
✅ `mockProxies` - 代理组和节点
✅ `mockTraffic` - 流量数据（需要 WebSocket 或轮询）

### 流量数据 Mock 处理：
由于 WebSocket 在 mock 模式下无法工作，可以：
1. 使用定时器模拟流量更新
2. 在客户端 `useTraffic` hook 中检测 mock 模式
3. 返回模拟的随机流量数据

---

## 八、UI/UX 优化建议

### 8.1 视觉层次
```
代理页面结构：
┌─────────────────────────────────┐
│ 标题 + 操作按钮                  │
├─────────────────────────────────┤
│ ⚡ Sing-box 状态（醒目展示）     │
├─────────────────────────────────┤
│ 📊 流量统计（实时数据）          │
├─────────────────────────────────┤
│ 🔍 搜索框                        │
├─────────────────────────────────┤
│ 📡 代理组（主要内容）            │
└─────────────────────────────────┘
```

### 8.2 移动端适配
- 状态卡片在小屏幕上垂直堆叠
- 流量统计使用图标 + 简写单位
- DNS 候选列表使用滚动容器

### 8.3 交互优化
- 电源开关带确认提示（可选）
- DNS 切换后自动刷新状态
- 流量数据使用平滑过渡动画

---

## 九、后续优化方向（可选）

### 9.1 数据可视化
- 流量趋势图表（使用 Chart.js 或 Recharts）
- 节点延迟历史记录
- DNS 健康检查时间线

### 9.2 高级功能
- 批量测试节点延迟
- 节点分组和标签
- 自定义排序规则

### 9.3 性能优化
- 虚拟滚动处理大量节点
- 节点延迟测试队列管理
- WebSocket 连接池优化

---

## 十、验收标准

### 功能完整性
- [ ] 代理页面显示 Sing-box 状态
- [ ] 代理页面显示流量统计
- [ ] 电源开关功能正常
- [ ] DNS 状态显示和切换正常
- [ ] 流量数据实时更新（或 mock 模式下模拟更新）
- [ ] 代理组和节点功能不受影响
- [ ] 概览页重定向到代理页
- [ ] 导航菜单更新正确
- [ ] 同步页所有"同步"改为"备份"

### 用户体验
- [ ] 页面布局美观、层次清晰
- [ ] 移动端响应式正常
- [ ] 加载状态和错误提示完善
- [ ] 交互动画流畅

### 代码质量
- [ ] 无 TypeScript 类型错误
- [ ] 无 ESLint 警告（关键）
- [ ] 代码复用良好
- [ ] 注释清晰

---

**规划完成时间**: 2026-01-21
**预计实施时间**: 1-2 小时
**风险等级**: 低（主要是页面重组，不涉及后端修改）
