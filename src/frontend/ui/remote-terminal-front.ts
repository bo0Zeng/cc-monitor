/**
 * 远端会话的 ↗ 前两问：此刻显示这个会话的，是这台电脑上哪一串进程。原因都来自两个后端的回话 ——
 * 界面不解析连接、不判对不对得上，只按顺序把一方的回话交给另一方：
 * ① 那台 `session-terminals {sid}` ⇒ 此刻连着这个会话的终端，或一条原因；
 * ② 本机后端 `terminal-processes {terminals}`（那台回话里的 `terminals` 原样交）⇒ 开着那条连接的进程往上的进程链，或一条原因。
 * 第三跳（沿链找窗口、校验、拉前）归 monitor（`bring_remote_terminal_to_front`）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 每一问的期限：那台读 `/proc` ＋ 问一次 tmux；本机读一次系统连接表与进程表。 */
const ASK_BUDGET_MS = 15_000;

/** 前两问的结局：进程链（原样交 monitor），或一句话（`reattach` = 可以在新终端里接回）。 */
export type RemoteFrontPlan = { chain: unknown[] } | { said: string; reattach: boolean };

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);

function badShape(): never {
  throw new Error(copyText("tabSessionActions.remoteFront.badShape"));
}

/** 一问的回话 ⇒ 对象；问不到 ⇒ 抛一句人话，形状不认 ⇒ 抛「两端版本对不上」。 */
async function answered(ask: Promise<Uint8Array>): Promise<Obj> {
  let reply: Uint8Array;
  try {
    reply = await ask;
  } catch (e) {
    throw new Error(saidOf(e, copyText("tabSessionActions.remoteFront.tooOld")));
  }
  const v = readJson(reply);
  return isObj(v) ? v : badShape();
}

/** 那台的原因 ⇒ 那句话。 */
function shownSaid(why: unknown): { said: string; reattach: boolean } {
  switch (why) {
    case "detached":
      return { said: copyText("tabSessionActions.remoteFront.detached"), reattach: true };
    case "no-terminal":
      return { said: copyText("tabSessionActions.remoteFront.noTerminal"), reattach: false };
    case "unreadable":
      return { said: copyText("tabSessionActions.remoteFront.unreadable"), reattach: false };
    default:
      return badShape();
  }
}

/** 本机后端的原因 ⇒ 那句话。 */
function localSaid(why: unknown, addr: unknown): string {
  switch (why) {
    case "not-ssh":
      return copyText("tabSessionActions.remoteFront.notSsh");
    case "elsewhere":
      return typeof addr === "string" ? copyText("tabSessionActions.remoteFront.elsewhere", { addr }) : badShape();
    case "mismatch":
      return copyText("tabSessionActions.remoteFront.mismatch");
    case "query-failed":
      return copyText("tabSessionActions.remoteFront.queryFailed");
    default:
      return badShape();
  }
}

/** 问那台、再问本机：进程链或一句话。问不到 / 形状不认 ⇒ 抛一句人话。 */
export async function planRemoteFront(origin: Origin, sid: string): Promise<RemoteFrontPlan> {
  const body = jsonBody({ sid });
  const budget = budgetWithin(ASK_BUDGET_MS);
  const shown = await answered(chan.call(origin, "session-terminals", body, budget));
  if (!Array.isArray(shown.terminals)) badShape();
  if (shown.why !== undefined) return shownSaid(shown.why);
  const localBody = jsonBody({ terminals: shown.terminals });
  const localBudget = budgetWithin(ASK_BUDGET_MS);
  const found = await answered(chan.call(LOCAL_ORIGIN, "terminal-processes", localBody, localBudget));
  if (!Array.isArray(found.chain)) badShape();
  if (found.why !== undefined) return { said: localSaid(found.why, found.addr), reattach: false };
  return found.chain.length > 0 ? { chain: found.chain } : badShape();
}
