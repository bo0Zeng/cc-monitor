/**
 * 快捷键覆盖的持久化。
 *
 * 存 config.json 顶层 `keybindings` 字段。schema：
 *
 * ```json
 * {
 *   "keybindings": {
 *     "tab.next": "Ctrl+Tab",            // 用户显式绑了一个（同 default 时存不存都行）
 *     "behavior.toggle-auto-follow": "Ctrl+Shift+KeyF",
 *     "tab.close-archived": null         // 用户显式解绑（即使有 default 也不绑）
 *   }
 * }
 * ```
 *
 * - 缺失的 id → 用 ACTIONS 表 default
 * - 已知 id 但 value 不是 string|null → 视为不存在（兜底脏数据）
 * - 未知 id → 静默丢（用户从老版本升级或手动改坏 config）
 */

import { loadConfig, saveConfig } from "../config";
import { findAction } from "./actions";

const KEY = "keybindings";

/** 读所有覆盖（已过滤无效）。空对象 = 全部走默认。 */
export async function getKeybindings(): Promise<Record<string, string | null>> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const raw = cfg[KEY];
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
    const out: Record<string, string | null> = {};
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      if (!findAction(k)) continue; // 未知 id
      if (v === null) out[k] = null;
      else if (typeof v === "string") out[k] = v;
      // 其他类型静默丢
    }
    return out;
  } catch (e) {
    console.warn("getKeybindings failed:", e);
    return {};
  }
}

/** 全量保存覆盖。merge 进 config 顶层不动其他字段。 */
export async function setKeybindings(value: Record<string, string | null>): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg[KEY] = value;
  await saveConfig(cfg);
}
