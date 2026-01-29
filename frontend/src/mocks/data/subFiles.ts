import type { SubFilesResponse, SubscriptionItem } from "@/types/api";

export const mockSubFiles: SubFilesResponse = {
  sub_dir: "/app/sub",
  sub_source: {
    type: "git",
    url: "https://example.com/miao-sub.git",
  },
  files: [
    {
      file_path: "/app/sub/main.yaml",
      file_name: "main.yaml",
      node_count: 38,
      loaded: true,
    },
    {
      file_path: "/app/sub/backup.yaml",
      file_name: "backup.yaml",
      node_count: 0,
      loaded: false,
      error: "解析失败",
    },
  ],
};

export const mockSubscriptions: SubscriptionItem[] = [
  {
    id: "sub-001",
    name: "主订阅",
    enabled: true,
    source: {
      type: "url",
      url: "https://example.com/sub.yaml",
    },
    updated_at: Math.floor(Date.now() / 1000) - 120,
    files: [
      {
        file_path: "/app/sub/sub-001/subscription.yaml",
        file_name: "subscription.yaml",
        node_count: 38,
        loaded: true,
        subscription_id: "sub-001",
      },
    ],
  },
  {
    id: "sub-002",
    name: "GitHub 订阅",
    enabled: true,
    source: {
      type: "git",
      repo: "https://github.com/example/miao-sub",
      workdir: "/app/sub/sub-002",
    },
    updated_at: Math.floor(Date.now() / 1000) - 600,
    files: [
      {
        file_path: "/app/sub/sub-002/main.yaml",
        file_name: "main.yaml",
        node_count: 22,
        loaded: true,
        subscription_id: "sub-002",
      },
    ],
  },
  {
    id: "sub-003",
    name: "本地目录",
    enabled: false,
    source: {
      type: "path",
      path: "/app/subs",
    },
    last_error: "目录不存在",
    files: [],
  },
];
