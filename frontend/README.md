# Miao Web - Next.js Frontend

基于 TypeScript + Next.js 的 Miao 控制面板前端。

## 技术栈

- **框架**: Next.js 15 (App Router)
- **语言**: TypeScript 5
- **样式**: Tailwind CSS v4 + Clay Design System
- **状态管理**: Zustand
- **动画**: Framer Motion
- **图标**: Lucide React

## 设计系统

采用 **High-Fidelity Claymorphism** 设计系统：
- 浅色背景 (#F4F1FA)
- 4层阴影系统模拟物理深度
- Nunito (标题) + DM Sans (正文)
- 大圆角设计 (最小 20px)
- 动画浮动背景 blobs

## 开发

```bash
# 安装依赖
npm install

# 启动开发服务器
npm run dev

# 构建生产版本
npm run build

# 启动生产服务器
npm start
```

## 环境配置

复制 `.env.local.example` 为 `.env.local` 并配置：

```env
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_WS_URL=ws://localhost:8080
API_URL=http://127.0.0.1:8080
```

## 项目结构

```
src/
├── app/                    # Next.js App Router
│   ├── (auth)/            # 认证页面
│   ├── dashboard/         # 仪表盘页面
│   └── api/               # API 路由代理
├── components/
│   └── ui/                # Clay UI 组件库
├── hooks/                 # 自定义 Hooks
├── lib/                   # 工具函数
│   ├── api.ts            # API 客户端
│   └── utils.ts          # 工具函数
├── stores/                # Zustand 状态管理
├── types/                 # TypeScript 类型
└── styles/
    └── globals.css       # 全局样式 + Design Tokens
```

## 功能页面

- `/login` - 登录
- `/setup` - 初始化设置
- `/dashboard` - 主仪表盘
- `/dashboard/proxies` - 代理/订阅/节点
- `/dashboard/tunnels` - TCP 穿透
- `/dashboard/sync` - 备份同步
- `/dashboard/terminals` - 终端管理
- `/dashboard/vnc` - 远程桌面
- `/dashboard/apps` - 应用管理
- `/dashboard/logs` - 日志查看

## License

MIT
