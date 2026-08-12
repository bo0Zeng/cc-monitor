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
const KEY_RESUME_LOCAL_PRESETS = "resumeCommandLocalPresets";
const KEY_RESUME_REMOTE_PRESETS = "resumeCommandRemotePresets";

/**
 * P6c：预设条数上界。
 *
 * ⚠ **有上界这件事必须被判据守着，不能只写在这里。** 本工作区 `P0c` 的 `M-D2` 就是
 * 「缓存有上界」当时没有任何东西守着 —— 去掉裁剪的变异**第一次跑照样绿**。
 * 守它的是 `behavior.vitest.ts` 里那条灌满再验最老被挤出的判据。
 */
export const RESUME_PRESET_CAP = 12;

/**
 * P6c：把一条命令并入预设列表 —— **纯函数**，好让判据打在这里。
 *
 * · `trim` 后为空 ⇒ 原样返回（空不是一条命令，是「用默认」）；
 * · 已经在列表里 ⇒ **上浮到最前**（刚用过的最可能再用），不产生重复项；
 * · 超过上界 ⇒ 从**尾部**丢（尾部是最久没用的）。
 */
export function withResumePreset(list: readonly string[], cmd: string): string[] {
  const v = cmd.trim();
  if (!v) return [...list];
  const rest = list.filter((x) => x !== v);
  return [v, ...rest].slice(0, RESUME_PRESET_CAP);
}

/**
 * P6c：从盘上读回来的预设 —— **盘上可能是任何东西**（用户手改 / 旧版本 / 半截写入）。
 *
 * 照本文件既有的宽容读法办：认不出就回缺省，不抛。
 * ⚠ 不是 `Array.isArray` 就完事：数组里也可能混着数字/对象。逐项筛。
 */
function readPresets(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  for (const x of raw) {
    if (typeof x !== "string") continue;
    const v = x.trim();
    if (!v || out.includes(v)) continue;
    out.push(v);
    if (out.length >= RESUME_PRESET_CAP) break;
  }
  return out;
}

const KEY_NOTIFY_TURN_END = "notifyTurnEnd";

const KEY_FORCE_LEGACY_LAUNCH_RENDERER = "forceLegacyLaunchRenderer";

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
  /**
   * P6c（#69 b/c）：**用过的**本地 resume 命令，最近用的在最前。
   *
   * 它只影响「那个字符串怎么被填进去」——**生效值仍然只有 `resumeCommandLocal` 一个**，
   * 拉起链那 6 处解析点一行没动。
   */
  resumeCommandLocalPresets: string[];
  /** P6c：同上，远端那格。⚠ 点选时必须走 `diagnoseRemoteLauncher` 那条诊断（见 `settings/panel.ts`）。 */
  resumeCommandRemotePresets: string[];
  /**
   * Batch14-F42：Claude 完成一轮（stop_reason==end_turn）且窗口在后台时发系统通知。
   * 默认 true。热更：turn-notify.ts 每次判定读缓存，设置保存时刷新缓存。
   */
  notifyTurnEnd: boolean;
  /**
   * F03（unify-launch）：手动兜底开关——强制远端启动走裸 shell 兜底渲染器，绕开 ccm CLI 探测/
   * 渲染路径。默认 false（自动探测：探测失败/未装/能力不足已自动 fail-open 到兜底，本开关只是
   * "even if 探测说能，我也不想走" 的人工逃生口，见 MASTERPLAN R2）。无 UI 暴露，需手改 config.json。
   */
  forceLegacyLaunchRenderer: boolean;
}

const DEFAULTS: BehaviorConfig = {
  autoFollowUserActive: true,
  bringMonitorToFrontOnUserActive: false,
  showBgSessions: true,
  resumeCommandLocal: "",
  resumeCommandRemote: "",
  resumeCommandLocalPresets: [],
  resumeCommandRemotePresets: [],
  notifyTurnEnd: true,
  forceLegacyLaunchRenderer: false,
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
      resumeCommandLocalPresets: readPresets(cfg[KEY_RESUME_LOCAL_PRESETS]),
      resumeCommandRemotePresets: readPresets(cfg[KEY_RESUME_REMOTE_PRESETS]),
      notifyTurnEnd:
        typeof cfg[KEY_NOTIFY_TURN_END] === "boolean"
          ? (cfg[KEY_NOTIFY_TURN_END] as boolean)
          : DEFAULTS.notifyTurnEnd,
      forceLegacyLaunchRenderer:
        typeof cfg[KEY_FORCE_LEGACY_LAUNCH_RENDERER] === "boolean"
          ? (cfg[KEY_FORCE_LEGACY_LAUNCH_RENDERER] as boolean)
          : DEFAULTS.forceLegacyLaunchRenderer,
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
  cfg[KEY_RESUME_LOCAL_PRESETS] = next.resumeCommandLocalPresets;
  cfg[KEY_RESUME_REMOTE_PRESETS] = next.resumeCommandRemotePresets;
  cfg[KEY_NOTIFY_TURN_END] = next.notifyTurnEnd;
  cfg[KEY_FORCE_LEGACY_LAUNCH_RENDERER] = next.forceLegacyLaunchRenderer;
  await saveConfig(cfg);
}
