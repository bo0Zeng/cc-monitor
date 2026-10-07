/**
 * **删一个会话** —— 界面经通道直说那台机器的后端；另有分叉源会话那三格的解码（起新会话框读它）。
 *
 * 本机远端同一条 `chan.call(origin, …)`：
 * - 删：`files-delete-session {sid} → {path}`（只收 sid，落点由后端按 sid 在它自己的记录树里找、必须恰是 `<sid>.jsonl`）。
 * - 分叉源会话的三格（工作目录 · 号 · 终端）：每格「知道（值 ＋ 来源码）」或「不知道（原因码）」，那台推（`control/fork_launch.rs`），界面零推断；
 *   分叉本身随起新会话那一个请求写（`session-new` 带 `forkFrom`）。
 *
 * monitor 那两条转交的 Tauri 命令删了；成品按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不返回空壳：分叉已经落盘而这边读不出结果时，重试会多出一份孤儿分支）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** 一格：知道（值 ＋ 从哪知道）或不知道（为什么）。码由那台定，句子由界面照码说。 */
export type ForkSlot<T> =
  | { kind: "known"; value: T; from: "record" | "process" | "terminal_list" }
  | { kind: "unknown"; why: "exited" | "no_cwd" | "live_no_account" };

/** 源会话所在的终端（同会话容器那一格）：`host: "none"` ＝ 不在任何终端里。 */
export type ForkTerminal = { host: string; terminal?: string };

/** 起分叉出来的新会话要的三格。号的值 `null` ＝ 账号 0，串 ＝ 号名。 */
export interface ForkLaunch {
  cwd: ForkSlot<string>;
  account: ForkSlot<string | null>;
  terminal: ForkSlot<ForkTerminal>;
}

/** 删一份文件 ＋ 回程。 */
const DELETE_BUDGET_MS = 30_000;

function exactly(v: unknown, what: string, keys: string[]): Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) throw new Error(`${what} reply shape mismatch: not an object`);
  const o = v as Record<string, unknown>;
  const got = Object.keys(o).sort().join(",");
  if (got !== [...keys].sort().join(",")) throw new Error(`${what} reply shape mismatch: keys ${got}`);
  return o;
}

const FROM = ["record", "process", "terminal_list"];
const WHY = ["exited", "no_cwd", "live_no_account"];

/** 一格 → {@link ForkSlot}（值由 `value` 判；码只认那几个）。 */
function slotOf<T>(v: unknown, what: string, value: (x: unknown) => x is T): ForkSlot<T> {
  const bad = (): never => {
    throw new Error(`session-fork reply shape mismatch: launch.${what}`);
  };
  if (v === null || typeof v !== "object" || Array.isArray(v)) return bad();
  const o = v as Record<string, unknown>;
  if (o.kind === "known") {
    const k = exactly(o, `launch.${what}`, ["kind", "value", "from"]);
    if (!value(k.value) || !FROM.includes(k.from as string)) return bad();
    return { kind: "known", value: k.value, from: k.from as "record" };
  }
  const u = exactly(o, `launch.${what}`, ["kind", "why"]);
  if (u.kind !== "unknown" || !WHY.includes(u.why as string)) return bad();
  return { kind: "unknown", why: u.why as "exited" };
}

const isStr = (x: unknown): x is string => typeof x === "string";
const isAccount = (x: unknown): x is string | null => x === null || typeof x === "string";
const isTerminal = (x: unknown): x is ForkTerminal => {
  if (x === null || typeof x !== "object" || Array.isArray(x)) return false;
  const t = x as Record<string, unknown>;
  const keys = Object.keys(t).sort().join(",");
  return typeof t.host === "string" && (keys === "host" || (keys === "host,terminal" && typeof t.terminal === "string"));
};

/** `launch` 那一格 → {@link ForkLaunch}（恰好三格）。起新会话框读分叉源会话那三格也用它。 */
export function decodeForkLaunch(v: unknown): ForkLaunch {
  const o = exactly(v, "session-fork launch", ["cwd", "account", "terminal"]);
  return {
    cwd: slotOf(o.cwd, "cwd", isStr),
    account: slotOf(o.account, "account", isAccount),
    terminal: slotOf(o.terminal, "terminal", isTerminal),
  };
}

/** 删 `origin` 那台机器上的会话 `sid`。成品恰好一格 `path`（删掉的那份；非 UTF-8 路径是 `{b16}` 形，界面不用它）。 */
export async function deleteSession(origin: Origin, sid: string): Promise<void> {
  const budget = budgetWithin(DELETE_BUDGET_MS);
  const body = jsonBody({ sid });
  const reply = await chan.call(origin, "files-delete-session", body, budget);
  exactly(readJson(reply), "files-delete-session", ["path"]);
}
