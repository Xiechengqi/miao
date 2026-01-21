import type { SubFilesResponse } from "@/types/api";

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
