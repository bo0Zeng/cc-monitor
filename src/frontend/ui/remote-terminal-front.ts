/**
 * 远端会话的 ↗ 前两问：此刻显示这个会话的，是这台电脑上哪一串进程。原因都来自两个后端的回话 ——
 * 界面不解析连接、不判对不对得上，只按顺序把一方的回话交给另一方：
 * ① 那台 `session-terminals {sid}` ⇒ 此刻连着这个会话的终端，或一条原因；
 * ② 本机后端 `terminal-processes {terminals}`（那台回话里的 `terminals` 原样交）⇒ 开着那条连接的进程往上的进程链，或一条原因。
 * 第三跳（沿链找窗口、校验、拉前）归 monitor（`bring_remote_terminal_to_front`）。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, isOldBackend, jsonBody, readJson } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { FrontResult } from "./front-result";

/** 每一问的期限：那台读 `/proc` ＋ 问一次 tmux；本机读一次系统连接表与进程表。 */
const ASK_BUDGET_MS = 15_000;

/** 前两问的结局：进程链（原样交 monitor），或一个结局族。 */
export type RemoteFrontPlan = { chain: unknown[] } | { result: FrontResult };

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);

/** 形状不认（两端版本对不上）。 */
class BadShape extends Error {}

/** 一次经通道的问失败了 ⇒ 结局族（按通道那一跳的归因，不看文字）。 */
export function failedAsk(e: unknown): FrontResult {
  if (e instanceof BadShape) return { kind: "bad-shape", detail: e.message };
  if (isOldBackend(e)) return { kind: "too-old" };
  if (e instanceof ChanError && e.error.layer === "hop") return e.error.why === "Overrun" ? { kind: "timeout" } : { kind: "offline" };
  return { kind: "unknown", detail: e instanceof Error ? e.message : String(e) };
}

/** 一问的回话 ⇒ 对象；形状不认 ⇒ 抛 `BadShape`。 */
async function answered(op: string, ask: Promise<Uint8Array>): Promise<Obj> {
  const reply = await ask;
  let v: unknown;
  try {
    v = readJson(reply);
  } catch {
    throw new BadShape(`${op}: reply is not JSON`);
  }
  if (!isObj(v)) throw new BadShape(`${op}: reply is not an object`);
  return v;
}

/** 那台的原因 ⇒ 结局族。 */
function shownResult(why: unknown): FrontResult {
  switch (why) {
    case "detached":
      return { kind: "detached" };
    case "no-terminal":
      return { kind: "no-terminal" };
    case "unreadable":
      return { kind: "unreadable" };
    default:
      throw new BadShape(`session-terminals: why=${JSON.stringify(why)}`);
  }
}

/** 本机后端的原因 ⇒ 结局族。 */
function localResult(why: unknown, addr: unknown): FrontResult {
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
    default:
      throw new BadShape(`terminal-processes: why=${JSON.stringify(why)}`);
  }
}

/** 问那台、再问本机：进程链或一个结局族。问不到 / 形状不认也落成结局族（不抛）。 */
export async function planRemoteFront(origin: Origin, sid: string): Promise<RemoteFrontPlan> {
  try {
    const body = jsonBody({ sid });
    const budget = budgetWithin(ASK_BUDGET_MS);
    const shown = await answered("session-terminals", chan.call(origin, "session-terminals", body, budget));
    if (!Array.isArray(shown.terminals)) throw new BadShape("session-terminals: no terminals");
    if (shown.why !== undefined) return { result: shownResult(shown.why) };
    const localBody = jsonBody({ terminals: shown.terminals });
    const localBudget = budgetWithin(ASK_BUDGET_MS);
    const found = await answered("terminal-processes", chan.call(LOCAL_ORIGIN, "terminal-processes", localBody, localBudget));
    if (!Array.isArray(found.chain)) throw new BadShape("terminal-processes: no chain");
    if (found.why !== undefined) return { result: localResult(found.why, found.addr) };
    if (found.chain.length === 0) throw new BadShape("terminal-processes: empty chain");
    return { chain: found.chain };
  } catch (e) {
    return { result: failedAsk(e) };
  }
}
