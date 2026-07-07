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

const KEY_SHOW_BG = "showBgSessions";

const KEY_RESUME_LOCAL = "resumeCommandLocal";
const KEY_RESUME_REMOTE = "resumeCommandRemote";

export interface BehaviorConfig {
  /** 用户在 claude 里敲键发送消息时自动切到对应 monitor tab。默认 true。 */
  autoFollowUserActive: boolean;
  /**
   * 自动切 tab 时是否同时把 monitor 主窗口拉到前台（unminimize + set_focus）。
   * 默认 false，避免打断用户看其他窗口（浏览器 / IDE）。
   * autoFollowUserActive=false 时此项无意义。
   */
  bringMonitorToFrontOnUserActive: boolean;
  /**
   * Batch7-F24：显示 bg 后台任务会话（⚙ 标识 + 树状挂宿主后）。默认 true。
   * **重启生效**（后端启动时读一次：本地扫描过滤 + 远端 daemon --with-bg）。
   */
  showBgSessions: boolean;
  /**
   * F34：本地历史 resume 用的启动命令（如 `cc` / `cct`）。空 = 默认行为
   * （自动检测 PowerShell 的 cc 函数，回退 claude）。后端做防注入校验。
   */
  resumeCommandLocal: string;
  /** F34：远端 resume 复制命令用的启动命令（如 `cct`）。空 = 默认 `claude`。 */
  resumeCommandRemote: string;
}

const DEFAULTS: BehaviorConfig = {
  autoFollowUserActive: true,
  bringMonitorToFrontOnUserActive: false,
  showBgSessions: true,
  resumeCommandLocal: "",
  resumeCommandRemote: "",
};

/** 读行为字段；缺失 / 类型不对走默认值，永不抛。 */
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
      showBgSessions:
        typeof cfg[KEY_SHOW_BG] === "boolean"
          ? (cfg[KEY_SHOW_BG] as boolean)
          : DEFAULTS.showBgSessions,
      resumeCommandLocal:
        typeof cfg[KEY_RESUME_LOCAL] === "string"
          ? (cfg[KEY_RESUME_LOCAL] as string)
          : DEFAULTS.resumeCommandLocal,
      resumeCommandRemote:
        typeof cfg[KEY_RESUME_REMOTE] === "string"
          ? (cfg[KEY_RESUME_REMOTE] as string)
          : DEFAULTS.resumeCommandRemote,
    };
  } catch (e) {
    console.warn("getBehavior failed:", e);
    return { ...DEFAULTS };
  }
}

/** 保存行为字段。merge 进现有 config 顶层，不动 theme / diagnostics 等。 */
export async function setBehavior(next: BehaviorConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg[KEY_AUTO_FOLLOW] = next.autoFollowUserActive;
  cfg[KEY_BRING_FRONT] = next.bringMonitorToFrontOnUserActive;
  cfg[KEY_SHOW_BG] = next.showBgSessions;
  cfg[KEY_RESUME_LOCAL] = next.resumeCommandLocal;
  cfg[KEY_RESUME_REMOTE] = next.resumeCommandRemote;
  await saveConfig(cfg);
}
