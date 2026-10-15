/**
 * 行为类设置的桥接。
 *
 * `autoFollowUserActive` 和 `bringMonitorToFrontOnUserActive` 与 `theme` /
 * `diagnostics` 字段平级，存在同一个 config.json 顶层。
 *
 * 这两个 toggle **运行时可热更**（不像 Claude 数据目录需要重启）。设置面板 toggle
 * 改了 → 立即 save → tabs.ts 下次 userActive 触发时读最新值。
 *
 * 设计：跟 theme.ts 模式一致 —— Rust config 命令通透（serde_json::Value）
 * 不解释 schema，所有 schema 收敛在前端 TS。
 */

import { loadConfig, patchConfig, setAt } from "./config";

const KEY_AUTO_FOLLOW = "autoFollowUserActive";
const KEY_BRING_FRONT = "bringMonitorToFrontOnUserActive";

const KEY_RESUME_PRESETS = "resumeCommandPresets";

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
   * 用过的恢复命令，最近用的在最前（每台「恢复会话的启动程序」那一格的下拉）：只是这扇界面的输入历史。
   * 生效值住各台后端（`prefs.json` 的 `launchers`，经 `machine-prefs.ts`），恢复进不进 tmux 也在那里。
   */
  resumeCommandPresets: string[];
}

const DEFAULTS: BehaviorConfig = {
  autoFollowUserActive: true,
  bringMonitorToFrontOnUserActive: false,
  resumeCommandPresets: [],
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
    resumeCommandPresets: readPresets(cfg[KEY_RESUME_PRESETS]),
  };
}

/** 保存行为字段。只交这几个顶层键（按键补丁），不动 theme / diagnostics 等。 */
export async function setBehavior(next: BehaviorConfig): Promise<void> {
  await patchConfig([
    setAt([KEY_AUTO_FOLLOW], next.autoFollowUserActive),
    setAt([KEY_BRING_FRONT], next.bringMonitorToFrontOnUserActive),
    setAt([KEY_RESUME_PRESETS], next.resumeCommandPresets),
  ]);
}


/** 用过一条恢复命令：并进预设（只写这一键）、回并好的那一份。空 ⇒ 不记、原样回。 */
export async function rememberResumePreset(cmd: string): Promise<string[]> {
  const before = (await getBehavior()).resumeCommandPresets;
  const next = withResumePreset(before, cmd);
  if (next.length === before.length && next.every((v, i) => v === before[i])) return before;
  await patchConfig([setAt([KEY_RESUME_PRESETS], next)]);
  return next;
}
