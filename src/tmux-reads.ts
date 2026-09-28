/**
 * 〔MIG-1 续 · `设计/99 §2.1 ⑬`〕「那台机器此刻有哪些 tmux 会话」**走通道，那台后端出成品**：`chan.call(origin, "tmux-list")`
 * ⇒ `{installed, sessions}`（后端 `observe/tmux_list.rs`，解析从 monitor 的 `parse_tmux_ls`〔散文墓碑〕 搬过去）。本机远端同一形、同一问
 * （monitor 那两条 Tauri 命令 `list_local_tmux` / `list_remote_tmux` 退役）。
 *
 * 回值三态，不许压成两态：
 * - 列表（含空表）—— 知道：那台装了 tmux，这些会话（零个也是知道）；
 * - `null` —— 知道：那台**没装 tmux** ⇒ 一个名字都没占；
 * - 抛 —— **不知道**（那台的后端不在 / 太旧 / 观测无效 `unobservable`：通道被改写、超时）。
 *
 * 按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」）；跨语言金样 `tests/__fixtures__/tmux-list.golden.json`。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 一个 tmux 会话（后端 `observe/tmux_list.rs::TmuxRow`）。 */
export interface TmuxSession {
  name: string;
  /** `#{pane_current_path}`。 */
  path: string;
  /** `#{pane_current_command}`。 */
  command: string;
  attached: boolean;
  windows: number;
  /**
   * F74：`@ccm_sid` —— 此 tmux 当前所跑 claude 会话的 sid（随 `/branch` 漂移实时更新）。未设置 ⇒ `null`。
   * 用它精确认「哪个 tmux 跑目标 sid」，取代按目录 / 名字取第一个（同目录多 claude 会撞错会话，`INVARIANTS §30`）。
   */
  sid: string | null;
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Obj, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};

function bad(): never {
  throw new Error(copyText("tmuxReads.reply.badShape"));
}

function decodeRow(v: unknown): TmuxSession {
  if (
    !isObj(v) ||
    !sameKeys(v, ["name", "path", "command", "attached", "windows", "sid"]) ||
    typeof v.name !== "string" ||
    typeof v.path !== "string" ||
    typeof v.command !== "string" ||
    typeof v.attached !== "boolean" ||
    typeof v.windows !== "number" ||
    !Number.isInteger(v.windows) ||
    !(v.sid === null || typeof v.sid === "string")
  ) {
    bad();
  }
  return { name: v.name, path: v.path, command: v.command, attached: v.attached, windows: v.windows, sid: v.sid };
}

/** `tmux-list` 的成品 ⇒ 列表 / `null`（没装 tmux）。严格收。 */
export function decodeTmuxList(v: unknown): TmuxSession[] | null {
  if (!isObj(v) || !sameKeys(v, ["installed", "sessions"]) || typeof v.installed !== "boolean" || !Array.isArray(v.sessions)) {
    bad();
  }
  const sessions = (v.sessions as unknown[]).map(decodeRow);
  return v.installed ? sessions : null;
}

/** 期限：后端那一趟 `tmux ls` 自带 5 s 上界，15 s 盖住回程（远端没连着还要握手）。 */
const TMUX_LIST_BUDGET_MS = 15_000;

/** 那台后端比这一问老（成品形状不认）时的那句话。 */
const OLD_BACKEND = copyText("tmuxReads.backend.tooOld");

/** 问一次那台机器的 tmux 会话（见头注三态）。问不到 ⇒ 抛一句人话。 */
export async function listTmux(origin: Origin): Promise<TmuxSession[] | null> {
  let reply: Uint8Array;
  try {
    const body = jsonBody({});
    const budget = budgetWithin(TMUX_LIST_BUDGET_MS);
    reply = await chan.call(origin, "tmux-list", body, budget);
  } catch (e) {
    throw new Error(saidOf(e, OLD_BACKEND));
  }
  return decodeTmuxList(readJson(reply));
}
