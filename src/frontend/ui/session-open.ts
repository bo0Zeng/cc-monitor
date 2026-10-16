/**
 * **起会话 · 恢复 · 接回：界面这一侧只有这一个家**（帧命令 `session-open`）。意图交那台，那台判完、做完，回「落在哪 ＋ 要开终端就跑这一行」；
 * 这里只做四件：发请求 · 失败按码挑按钮（那一句是那台写的，原样上屏）· 照 `landed` 开那一行（开不了就复制）· 交等报到。
 * 「说不清不恢复」归标签页那一侧：连接断了是界面唯一自己知道的事。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, machineName, refusalOf } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { saidOfTransport } from "./control-said";
import { detailOf } from "./kit/detail";
import { isObj } from "./ipc/decode";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { writeClipboard } from "./clipboard";
import { NoTerminalWindow, openLocalTerminal, openTerminal, sayNoTerminal } from "./terminal-open";
import { arrivedBody, expectArrival } from "./launch-arrival";
import { chosenAccount } from "./launch-account";
import { offerResyncRetry } from "./resync";
import { NEW_BUDGET_MS } from "./new-session-reads";
import type { SessionOpenReq } from "./generated/SessionOpenReq";
import type { OpenWhat } from "./generated/OpenWhat";
import type { Opened } from "./generated/Opened";
import type { Landed } from "./generated/Landed";
import type { AccountUnavailable } from "./generated/AccountUnavailable";

export type { SessionOpenReq, Opened, Landed };

/** 一问的结局：成了 · 那台拒了（码 · 那一句 · `data` · 复制详情）· 同一张票那一趟还在起 · 期限到（结果未知）· 够不着那台。 */
export type OpenEnd =
  | { kind: "opened"; opened: Opened }
  | { kind: "refused"; code: string; said: string; data: unknown; detail: string }
  | { kind: "pending"; said: string }
  | { kind: "timeout"; detail: string }
  | { kind: "unreachable"; said: string; detail: string };

/** 只问、只分结局，不出声。 */
export async function askOpen(origin: Origin, req: SessionOpenReq): Promise<OpenEnd> {
  try {
    return { kind: "opened", opened: await callTyped(origin, "session-open", req, budgetWithin(NEW_BUDGET_MS)) };
  } catch (e) {
    const detail = detailOf(e);
    if (!(e instanceof ChanError)) return { kind: "unreachable", said: String(e), detail };
    const err = e.error;
    if (err.layer === "peer" && err.why === "refused") {
      const r = refusalOf(err.body);
      if (r?.code === "launch_pending") return { kind: "pending", said: r.message };
      if (r) return { kind: "refused", code: r.code, said: r.message, data: r.data, detail };
    }
    // 发出去了、没等到回话 ⇒ 结果未知，不说失败。
    if (err.layer === "hop" && err.reach !== "NotSent") return { kind: "timeout", detail };
    return { kind: "unreachable", said: saidOfTransport(origin, err), detail };
  }
}

/**
 * 照 `landed` 开不开那一行（批量那一列）· 等不等报到（单个）。单个一律开：`window` 不跑就没起；`started` · `in_place` 开了是接进去；
 * `running` · `idle` 本来就在，开了当场说接上。批量只开 `window`（后台起的不一个个接进去）。
 */
export const ON_LANDED: Record<Landed, { batch: boolean; wait: boolean }> = {
  window: { batch: true, wait: true },
  started: { batch: false, wait: true },
  in_place: { batch: false, wait: true },
  running: { batch: false, wait: false },
  idle: { batch: false, wait: false },
};

export interface OpenHow {
  /** 那台说了记录在不在（恢复那一形；标签页据此画「记录已不在」）。 */
  onRecord?(present: boolean): void;
}

/** 在飞的那几条（`origin` ＋ sid / 终端名）：再点不重发。 */
const inFlight = new Set<string>();

function flightKey(origin: Origin, what: OpenWhat): string | null {
  const id = what.kind === "resume" ? what.sid : what.kind === "look" ? (what.sid ?? what.session) : undefined;
  return id === undefined ? null : `${origin}\u0000${what.kind}\u0000${id}`;
}

/** 那一句的第一行当标题、其余当正文。 */
function lines(said: string): [string, string] {
  const i = said.indexOf("\n");
  return i < 0 ? [said, ""] : [said.slice(0, i), said.slice(i + 1)];
}

function unavailableOf(data: unknown): AccountUnavailable | null {
  if (!isObj(data) || !isObj(data.unavailable)) return null;
  const u = data.unavailable;
  return typeof u.requested === "string" && typeof u.listKnown === "boolean" ? (u as unknown as AccountUnavailable) : null;
}

/** 起 · 恢复 · 接回一次：问、说、开、交等报到。回「那一行真开出来了吗」（没开的那一路已经出过声）。 */
export async function openSession(origin: Origin, req: SessionOpenReq, how: OpenHow = {}): Promise<boolean> {
  const key = flightKey(origin, req.what);
  if (key !== null) {
    if (inFlight.has(key)) return false;
    inFlight.add(key);
  }
  let end: OpenEnd;
  try {
    end = await askOpen(origin, req);
  } finally {
    if (key !== null) inFlight.delete(key);
  }
  if (end.kind !== "opened") {
    sayEnd(origin, req, how, end);
    return false;
  }
  const o = end.opened;
  if (req.what.kind === "resume") how.onRecord?.(true);
  if (o.note !== null) toast(o.note, "", { level: "info" });
  if (o.line === null) return false;
  if (!(await openLine(origin, o.line, o.cwd))) return false;
  if (!ON_LANDED[o.landed].wait) {
    const body = o.session === null ? "" : copyText("sessionOpen.attached.body", { machine: machineName(origin), name: o.session });
    toast(copyText("sessionOpen.attached.title"), body, { level: "info" });
  } else if (o.agent !== null && (o.sid !== null || o.cwd !== null)) {
    expectArrival({
      origin,
      match: o.sid !== null ? { sid: o.sid } : { cwd: o.cwd! },
      tmuxName: o.session,
      arrived: {
        title: req.what.kind === "new" ? copyText("sessionOpen.arrived.new") : copyText("sessionOpen.arrived.resume"),
        body: arrivedBody(origin),
      },
      ...(o.cwd !== null ? { slot: { cwd: o.cwd, agent: o.agent } } : {}),
    });
  }
  return true;
}

/** 没成的那几形各说一句；按钮只有两种：选不了号 ⇒ 改用那台给的替代（没有 ⇒ 不指定账号）· 不是这个会话的 ⇒ 对齐后重试。 */
function sayEnd(origin: Origin, req: SessionOpenReq, how: OpenHow, end: Exclude<OpenEnd, { kind: "opened" }>): void {
  switch (end.kind) {
    case "pending":
      toast(...lines(end.said), { level: "info" });
      return;
    case "timeout":
      toast(copyText("sessionOpen.timeout.title", { machine: machineName(origin) }), "", { detail: end.detail });
      return;
    case "unreachable":
      toast(end.said, "", { detail: end.detail });
      return;
    case "refused":
      break;
  }
  const [title, body] = lines(end.said);
  const again = (r: SessionOpenReq): Promise<void> => openSession(origin, r, how).then(() => undefined);
  switch (end.code) {
    case "account_unavailable": {
      const u = unavailableOf(end.data);
      const alt = u?.listKnown ? u.alternative : null;
      toast(title, body, {
        detail: end.detail,
        action: {
          label: alt === null ? copyText("sessionOpen.account.useBase") : copyText("sessionOpen.account.useAlt", { alt }),
          run: () => void again({ ...req, account: chosenAccount(alt) }),
        },
      });
      return;
    }
    case "wrong_owner": {
      const sid = req.what.kind === "resume" || req.what.kind === "look" ? req.what.sid : undefined;
      offerResyncRetry(origin, sid, title, body, () => again(req));
      return;
    }
    case "record_gone":
      how.onRecord?.(false);
      break;
  }
  toast(title, body, { detail: end.detail });
}

/** 开那一行：本机带起始目录（`null` ⇒ 壳照家目录开）、远端经 ssh。开不了 ⇒ 那一行进剪贴板、说一句（找不到终端 ⇒ 照实说、给设置入口）。 */
async function openLine(origin: Origin, line: string, cwd: string | null): Promise<boolean> {
  try {
    if (isLocalOrigin(origin)) await openLocalTerminal(line, cwd);
    else await openTerminal(origin, line);
    return true;
  } catch (err) {
    if (err instanceof NoTerminalWindow) {
      sayNoTerminal(err);
      return false;
    }
    let copied = true;
    try {
      await writeClipboard(line);
    } catch {
      copied = false;
    }
    const where = isLocalOrigin(origin)
      ? copyText("sessionOpen.copy.runLocal")
      : copyText("sessionOpen.copy.runRemote", { machine: origin });
    toast(copied ? copyText("sessionOpen.copy.copied") : copyText("sessionOpen.copy.manual"), `${where}\n${line}`, {
      detail: detailOf(err),
      level: "info",
    });
    return false;
  }
}
