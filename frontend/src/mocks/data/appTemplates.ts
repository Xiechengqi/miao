import type { AppTemplate } from "@/types/api";

export const mockAppTemplates: AppTemplate[] = [
  {
    id: "chromium",
    name: "Chromium",
    command: "chromium",
    args: ["--no-sandbox"],
    env: { LANG: "en_US.UTF-8" },
  },
  {
    id: "firefox",
    name: "Firefox",
    command: "firefox",
    args: [],
    env: {},
  },
];
