/**
 * 远端会话的 ↗ 前两问：此刻显示这个会话的，是这台电脑上哪一串进程。原因都来自两个后端的回话 ——
 * 界面不解析连接、不判对不对得上，只按顺序把一方的回话交给另一方：
 * ① 那台 `session-terminals {sid}` ⇒ 此刻连着这个会话的终端，或一条原因；
 * ② 那台回话里的 `terminals` 原样交 monitor 按窗口标签找（`byLabel`，对上就已经拉到前台）；
 * ③ 没对上 ⇒ 本机后端 `terminal-processes {terminals}`（同样原样交）⇒ 开着那条连接的进程往上的进程链，或一条原因。
 * 最后一跳（沿链找窗口、校验、拉前）归 monitor（`bring_remote_terminal_to_front`）。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, isOldBackend } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { FrontResult } from "./front-result";
import type { Link } from "./generated/Link";
import type { Processes } from "./generated/Processes";
import type { Showing } from "./generated/Showing";
import type { Terminal } from "./generated/Terminal";

/** 每一问的期限：那台读 `/proc` ＋ 问一次 tmux；本机读一次系统连接表与进程表。 */
const ASK_BUDGET_MS = 15_000;

/** 这几问的结局：进程链（原样交 monitor），或一个结局族（按窗口标签已经拉到的那一次也是一个结局族）。 */
export type RemoteFrontPlan = { chain: Link[] } | { result: FrontResult };

/** 形状不认（回包类型对，但内容自相矛盾：`elsewhere` 却没带地址 · 对上了却是空链）。 */
class BadShape extends Error {}

/** 一次经通道的问失败了 ⇒ 结局族（按通道那一跳的归因，不看文字）。 */
export function failedAsk(e: unknown): FrontResult {
  if (e instanceof BadShape) return { kind: "bad-shape", detail: e.message };
  if (isOldBackend(e)) return { kind: "too-old" };
  if (e instanceof ChanError && e.error.layer === "hop") return e.error.why === "Overrun" ? { kind: "timeout" } : { kind: "offline" };
  return { kind: "unknown", detail: e instanceof Error ? e.message : String(e) };
}

/** 那台的原因 ⇒ 结局族。 */
function shownResult(why: NonNullable<Showing["why"]>): FrontResult {
  switch (why) {
    case "detached":
      return { kind: "detached" };
    case "no-terminal":
      return { kind: "no-terminal" };
    case "unreadable":
      return { kind: "unreadable" };
  }
}

/** 本机后端的原因 ⇒ 结局族。 */
function localResult(why: NonNullable<Processes["why"]>, addr: string | undefined): FrontResult {
  switch (why) {
    case "not-ssh":
      return { kind: "not-ssh" };
    case "elsewhere":
      if (typeof addr === "string") return { kind: "elsewhere", addr };
      throw new BadShape("terminal-processes: elsewhere without addr");
    case "mismatch":
      return { kind: "mismatch" };
    case "query-failed":
      return { kind: "query-failed", detail: "terminal-processes: query-failed" };
  }
}

/**
 * 问那台、按窗口标签找、再问本机：进程链或一个结局族。问不到 / 形状不认也落成结局族（不抛）。
 * `byLabel`：那台回的 `terminals` 原样交 monitor 按窗口标签找 ⇒ 对上了那一次的结局；没对上 ⇒ `null`（接着按连接对）。
 */
export async function planRemoteFront(
  origin: Origin,
  sid: string,
  byLabel: (terminals: Terminal[]) => Promise<FrontResult | null>,
): Promise<RemoteFrontPlan> {
  try {
    const shown = await callTyped(origin, "session-terminals", { sid }, budgetWithin(ASK_BUDGET_MS));
    if (shown.why !== undefined) return { result: shownResult(shown.why) };
    const labeled = await byLabel(shown.terminals);
    if (labeled !== null) return { result: labeled };
    const found = await callTyped(LOCAL_ORIGIN, "terminal-processes", { terminals: shown.terminals }, budgetWithin(ASK_BUDGET_MS));
    if (found.why !== undefined) return { result: localResult(found.why, found.addr) };
    if (found.chain.length === 0) throw new BadShape("terminal-processes: empty chain");
    return { chain: found.chain };
  } catch (e) {
    return { result: failedAsk(e) };
  }
}

/**
 * 本机会话按窗口标签找：问本机后端 `session-terminals {sid}`（谁在显示它 · 带不带窗口标签），回话里的 `terminals` 原样交 `byLabel`
 * ⇒ 对上了那一次的结局；没有终端 / 后端说了原因 / 问不到 / 没对上 ⇒ `null`（调用方照旧用壳按缓存的那一句）。
 * Linux 上本机会话在 tmux 里，claude 的进程链到不了终端窗口；tmux 客户端带着 bash / zsh 接入块设的 `LC_CCM_WINDOW`。
 */
export async function frontByLocalLabel(sid: string, byLabel: (terminals: Terminal[]) => Promise<FrontResult | null>): Promise<FrontResult | null> {
  try {
    const shown = await callTyped(LOCAL_ORIGIN, "session-terminals", { sid }, budgetWithin(ASK_BUDGET_MS));
    if (shown.why !== undefined || shown.terminals.length === 0) return null;
    return await byLabel(shown.terminals);
  } catch (e) {
    console.warn("local terminal front by label failed:", e);
    return null;
  }
}
