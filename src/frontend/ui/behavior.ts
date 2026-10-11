/**
 * 行为类设置的桥接。
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

import { loadConfig, patchConfig, setAt } from "./config";

const KEY_AUTO_FOLLOW = "autoFollowUserActive";
const KEY_BRING_FRONT = "bringMonitorToFrontOnUserActive";

const KEY_SHOW_BG = "showBgSessions";

const KEY_RESUME = "resumeCommand";
const KEY_RESUME_PRESETS = "resumeCommandPresets";
const KEY_RESUME_IN_TMUX = "resumeInTmux";

/**
 * 预设条数上界（`behavior.vitest.ts` 有一条灌满再验最老被挤出的判据守着）。
 */
export const RESUME_PRESET_CAP = 12;

/**
 * 把一条命令并入预设列表（纯函数）。
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
 * 从盘上读回来的预设 —— 盘上可能是任何东西（用户手改 / 旧版本 / 半截写入）。
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
const KEY_NOTIFY_NEEDS = "notifyNeeds";

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
   * 显示 bg 后台任务会话（⚙ 标识；平铺为普通 tab）。默认 true。
   * **重启生效**（壳起流时读一次：本机远端两条流都按它藏 bg 会话）。
   */
  showBgSessions: boolean;
  /**
   * 恢复命令的默认值（通用页那一格，各台都用它；如 `ccm` / `cc`）。空 = 那台后端的默认
   *（本机：检测 cc，回退 claude；远端：claude）。每台可在「这台上的 cc-monitor」里单独设一格盖过它
   *（远端在机器表那台的 `resumeCommand`，本机在 `local-machine-prefs.ts`），解析只在 `remote-config.ts::resumeCommandFor`。
   */
  resumeCommand: string;
  /** 用过的恢复命令，最近用的在最前（通用页那一格的下拉）；只影响怎么填，生效值仍是上面那一格。 */
  resumeCommandPresets: string[];
  /** 历史页［恢复 ▾］与标签页「恢复 ▸」默认「运行于」：`true` ＝ tmux 里（那台没 tmux 时仍落「不用 tmux」）。默认 false。 */
  resumeInTmux: boolean;
  /**
   * Claude 完成一轮（stop_reason==end_turn）且窗口在后台时发系统通知。
   * 默认 true。热更：turn-notify.ts 每次判定读缓存，设置保存时刷新缓存。
   */
  notifyTurnEnd: boolean;
  /** 有会话开始等你（批准 / 回答 / 计划）且主窗口不在前台时发系统通知。默认 true；与上一格分开。 */
  notifyNeeds: boolean;
}

const DEFAULTS: BehaviorConfig = {
  autoFollowUserActive: true,
  bringMonitorToFrontOnUserActive: false,
  showBgSessions: true,
  resumeCommand: "",
  resumeCommandPresets: [],
  resumeInTmux: false,
  notifyTurnEnd: true,
  notifyNeeds: true,
};

/** 读行为字段；缺失 / 类型不对走默认值，永不抛。 */
export async function getBehavior(): Promise<BehaviorConfig> {
  try {
    return behaviorIn((await loadConfig()) as Record<string, unknown>);
  } catch (e) {
    console.warn("getBehavior failed:", e);
    return { ...DEFAULTS };
  }
}

/** 从一份已读回的配置里派生行为那一格；缺失 / 类型不对走默认值（设置窗「读一次配置派生三格」共用这一处）。 */
export function behaviorIn(cfg: Record<string, unknown>): BehaviorConfig {
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
    resumeCommand: typeof cfg[KEY_RESUME] === "string" ? (cfg[KEY_RESUME] as string) : DEFAULTS.resumeCommand,
    resumeCommandPresets: readPresets(cfg[KEY_RESUME_PRESETS]),
    resumeInTmux: typeof cfg[KEY_RESUME_IN_TMUX] === "boolean" ? (cfg[KEY_RESUME_IN_TMUX] as boolean) : DEFAULTS.resumeInTmux,
    notifyTurnEnd:
      typeof cfg[KEY_NOTIFY_TURN_END] === "boolean"
        ? (cfg[KEY_NOTIFY_TURN_END] as boolean)
        : DEFAULTS.notifyTurnEnd,
    notifyNeeds:
      typeof cfg[KEY_NOTIFY_NEEDS] === "boolean"
        ? (cfg[KEY_NOTIFY_NEEDS] as boolean)
        : DEFAULTS.notifyNeeds,
  };
}

/** 保存行为字段。只交这几个顶层键（按键补丁），不动 theme / diagnostics 等。 */
export async function setBehavior(next: BehaviorConfig): Promise<void> {
  await patchConfig([
    setAt([KEY_AUTO_FOLLOW], next.autoFollowUserActive),
    setAt([KEY_BRING_FRONT], next.bringMonitorToFrontOnUserActive),
    setAt([KEY_SHOW_BG], next.showBgSessions),
    setAt([KEY_RESUME], next.resumeCommand),
    setAt([KEY_RESUME_PRESETS], next.resumeCommandPresets),
    setAt([KEY_RESUME_IN_TMUX], next.resumeInTmux),
    setAt([KEY_NOTIFY_TURN_END], next.notifyTurnEnd),
    setAt([KEY_NOTIFY_NEEDS], next.notifyNeeds),
  ]);
}

