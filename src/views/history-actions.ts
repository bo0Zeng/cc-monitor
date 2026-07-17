/**
 * 历史条目「共享动作表」的判定核心（F96 · issue #62）。SS-4 共享面③块。
 *
 * 抽成**无 import 的纯模块**便于 node 轨断言（`history.ts` 依赖 DOM/IPC 测不了）。平行
 * `history-cache.ts`（①块）/ `history-prefs.ts`（②块）。
 *
 * 这里只放「**有哪些动作 + 每个动作何时可用（enabled）+ 菜单文案（label）**」的判定；每个动作的
 * **副作用体（run）留在 `history.ts`**（要 invoke / prompt / mutate 条目+项目+缓存，非纯）。
 * 条目行右键菜单与行尾 inline 按钮走同一张表 → 天然不漂移（呼应 SS-9「右键+命令栏共用一套动作」）。
 *
 * F85 搜索卡片复用本表：搜索卡片 ctx `hasEntry:false` → `actionsFor` 天然只放 resume/new-session
 * （star/rename/hide/delete 需要活的 entry+project 引用做缓存同步，搜索卡片没有）。
 */

export type HistoryActionId =
  | "resume"
  | "new-session"
  | "star"
  | "rename"
  | "hide"
  | "delete";

/**
 * 动作上下文（纯数据）。identity 段（sessionId/jsonlPath/cwd/origin）在条目行与搜索卡片都填得起；
 * `hasEntry` 标记 ctx 是否携带活的 entry+project 引用（条目行 true / 搜索卡片 false）——决定
 * star/rename/hide/delete 是否可用。starred/hidden/isLive 供菜单文案/未来判定用（纯布尔）。
 */
export interface HistoryActionCtx {
  sessionId: string;
  jsonlPath: string;
  cwd: string;
  origin?: string;
  isLive?: boolean;
  starred?: boolean;
  hidden?: boolean;
  hasEntry: boolean;
}

export interface HistoryActionDef {
  id: HistoryActionId;
  label(ctx: HistoryActionCtx): string;
  danger?: boolean;
  enabled(ctx: HistoryActionCtx): boolean;
}

/** 需要活的 entry+project 引用（缓存同步）的动作——搜索卡片（hasEntry:false）不出。 */
const needsEntry = (ctx: HistoryActionCtx): boolean => ctx.hasEntry;

/** 动作表（稳定顺序 = 菜单顺序）。resume/new-session 恒可用；star/rename/hide/delete 需 entry。 */
export const HISTORY_ACTION_DEFS: HistoryActionDef[] = [
  { id: "resume", label: () => "在新终端 resume", enabled: () => true },
  { id: "new-session", label: () => "在该目录起新会话", enabled: () => true },
  { id: "star", label: (c) => (c.starred ? "取消标星" : "标星"), enabled: needsEntry },
  { id: "rename", label: () => "重命名", enabled: needsEntry },
  { id: "hide", label: (c) => (c.hidden ? "取消隐藏" : "隐藏"), enabled: needsEntry },
  { id: "delete", label: () => "删除…", danger: true, enabled: needsEntry },
];

/** 某上下文下可用的动作（按 DEFS 稳定顺序过滤）。 */
export function actionsFor(ctx: HistoryActionCtx): HistoryActionDef[] {
  return HISTORY_ACTION_DEFS.filter((d) => d.enabled(ctx));
}
