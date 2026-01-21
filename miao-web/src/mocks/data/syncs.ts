import type { SyncConfig } from "@/types/api";

export const mockSyncs: SyncConfig[] = [
  {
    id: "1",
    name: "备份主目录",
    enabled: true,
    local_paths: ["/home/root"],
    remote_path: "/backup/root",
    ssh: {
      host: "backup.example.com",
      port: 22,
      username: "root",
    },
    auth: {
      type: "password",
      password: null,
    },
    options: {
      delete: true,
      verify: false,
      compress: true,
      bwlimit: "1MB",
      exclude: ["*.log"],
      include: [],
      parallel: 2,
      watch: false,
      extra_args: [],
    },
    schedule: {
      enabled: true,
      cron: "0 2 * * *",
      timezone: "Asia/Shanghai",
    },
    status: {
      state: "stopped",
    },
  },
  {
    id: "2",
    name: "日志备份",
    enabled: true,
    local_paths: ["/var/log/app.log", "/var/log/nginx"],
    remote_path: null,
    ssh: {
      host: "10.0.0.8",
      port: 2222,
      username: "backup",
    },
    auth: {
      type: "password",
      password: "******",
    },
    options: {
      delete: false,
      verify: true,
      compress: false,
      bwlimit: null,
      exclude: ["node_modules/"],
      include: ["important.txt"],
      parallel: null,
      watch: true,
      extra_args: ["--dry-run"],
    },
    schedule: {
      enabled: false,
    },
    status: {
      state: "running",
    },
  },
  {
    id: "3",
    name: "配置备份",
    enabled: false,
    local_paths: ["/etc/miao/config.yaml"],
    remote_path: "/backup/config.yaml",
    ssh: {
      host: "backup.internal",
      port: 22,
      username: "root",
    },
    auth: {
      type: "password",
      password: null,
    },
    options: {
      delete: false,
      verify: false,
      compress: false,
      bwlimit: null,
      exclude: [],
      include: [],
      parallel: null,
      watch: false,
      extra_args: [],
    },
    schedule: {
      enabled: true,
      cron: "0 */6 * * *",
      timezone: "Asia/Shanghai",
    },
    status: {
      state: "idle",
      last_error: {
        message: "远程连接失败",
      },
    },
  },
];
