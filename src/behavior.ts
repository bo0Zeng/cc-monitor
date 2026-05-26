/**
 * v2.4 issue #2：行为类设置的桥接。
 *
 * `autoFollowUserActive` 和 `bringMonitorToFrontOnUserActive` 与 `theme` /
 * `claudeDir` / `diagnostics` 字段平级，存在同一个 config.json 顶层。
 *
 * 这两个 toggle **运行时可热更**（不像 claudeDir 需要重启）。设置面板 toggle
 * 改了 → 立即 save → tabs.ts 下次 userActive 触发时读最新值。
 *
 * 设计：跟 paths.ts 模式一致 —— Rust config 命令通透（serde_json::Value）
 * 不解释 schema，所有 schema 收敛在前端 TS。
 */

import { loadConfig, saveConfig } from "./config";

const KEY_AUTO_FOLLOW = "autoFollowUserActive";
const KEY_BRING_FRONT = "bringMonitorToFrontOnUserActive";

export interface BehaviorConfig {
  /** 用户在 claude 里敲键发送消息时自动切到对应 monitor tab。默认 true。 */
  autoFollowUserActive: boolean;
  /**
   * 自动切 tab 时是否同时把 monitor 主窗口拉到前台（unminimize + set_focus）。
   * 默认 false，避免打断用户看其他窗口（浏览器 / IDE）。
   * autoFollowUserActive=false 时此项无意义。
   */
  bringMonitorToFrontOnUserActive: boolean;
}

const DEFAULTS: BehaviorConfig = {
  autoFollowUserActive: true,
  bringMonitorToFrontOnUserActive: false,
};

/** 读两个字段；缺失 / 类型不对走默认值，永不抛。 */
export async function getBehavior(): Promise<BehaviorConfig> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    return {
      autoFollowUserActive:
        typeof cfg[KEY_AUTO_FOLLOW] === "boolean"
          ? (cfg[KEY_AUTO_FOLLOW] as boolean)
          : DEFAULTS.autoFollowUserActive,
      bringMonitorToFrontOnUserActive:
        typeof cfg[KEY_BRING_FRONT] === "boolean"
          ? (cfg[KEY_BRING_FRONT] as boolean)
          : DEFAULTS.bringMonitorToFrontOnUserActive,
    };
  } catch (e) {
    console.warn("getBehavior failed:", e);
    return { ...DEFAULTS };
  }
}

/** 保存两个字段。merge 进现有 config 顶层，不动 theme / diagnostics 等。 */
export async function setBehavior(next: BehaviorConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg[KEY_AUTO_FOLLOW] = next.autoFollowUserActive;
  cfg[KEY_BRING_FRONT] = next.bringMonitorToFrontOnUserActive;
  await saveConfig(cfg);
}
