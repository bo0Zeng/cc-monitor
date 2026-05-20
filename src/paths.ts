/**
 * 前端访问 Claude 数据目录设置的桥接。
 *
 * 设计：`claudeDir` 字段与 `theme` 字段平级，都存在同一个 config.json 的顶层。
 * 改动后**需要重启 monitor 才会生效**——watcher / session_map 都在 setup()
 * 时一次性 resolve，运行时不再读 env / config。
 */

import { loadConfig, saveConfig } from "./config";

const KEY = "claudeDir";

/** 读取当前持久化的 claudeDir（设置面板里填的）。无字段 → null。 */
export async function getClaudeDirOverride(): Promise<string | null> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const v = cfg[KEY];
    return typeof v === "string" && v.trim() ? v : null;
  } catch {
    return null;
  }
}

/** 保存 claudeDir 字段。传 null 删除字段（回到 env / 默认）。 */
export async function setClaudeDirOverride(dir: string | null): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  if (dir === null || dir.trim() === "") {
    delete cfg[KEY];
  } else {
    cfg[KEY] = dir.trim();
  }
  await saveConfig(cfg);
}
