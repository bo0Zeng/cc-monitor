/**
 * 前端访问 Claude 数据目录设置的桥接。
 *
 * 设计：`claudeDir` 字段与 `theme` 字段平级，都存在同一个 config.json 的顶层。
 * 改动后**需要重启 monitor 才会生效**——watcher / session_map 都在 setup()
 * 时一次性 resolve，运行时不再读 env / config。
 */

import { patchConfig, removeAt, setAt } from "./config";

const KEY = "claudeDir";

/** 从一份已读回的配置里取 claudeDir 覆盖（设置窗「读一次配置派生三格」共用这一处）。无字段 → null。 */
export function claudeDirIn(cfg: Record<string, unknown>): string | null {
  const v = cfg[KEY];
  return typeof v === "string" && v.trim() ? v : null;
}

/** 保存 claudeDir 字段。传 null 删除字段（回到 env / 默认）。 */
export async function setClaudeDirOverride(dir: string | null): Promise<void> {
  await patchConfig([
    dir === null || dir.trim() === "" ? removeAt([KEY]) : setAt([KEY], dir.trim()),
  ]);
}
