/**
 * **起会话 · 恢复 · 接回：界面这一侧只有这一个家**（帧命令 `session-open`）。意图交那台，那台判完、做完，回「落在哪 ＋ 要开终端就跑这一行」；
 * 这里只做四件：发请求 · 失败按码挑按钮（那一句是那台写的，原样上屏）· 照 `landed` 开那一行（开不了就复制）·
 * 回包带 `arrival` ⇒ 问那台等它报到（`session-arrive`：认法 · 期限内没见到那一句 · 那一屏原话都是那台的）。
 * 新建框的失败按格落在框里（它自己排），成了同样交 [`landOpened`]。「说不清不恢复」归标签页那一侧：连接断了是界面唯一自己知道的事。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, machineName, refusalIn } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { detailOf, saidOf } from "./kit/detail";
import { exactKeys, isObj } from "./ipc/decode";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { writeClipboard } from "./clipboard";
import { NoTerminalWindow, openLocalTerminal, openTerminal, sayNoTerminal } from "./terminal-open";
import { chosenAccount } from "./launch-account";
import { offerResyncRetry } from "./resync";
import type { SessionOpenReq } from "./generated/SessionOpenReq";
import type { OpenWhat } from "./generated/OpenWhat";
import type { Opened } from "./generated/Opened";
import type { Landed } from "./generated/Landed";
import type { AccountUnavailable } from "./generated/AccountUnavailable";
import type { OpenField } from "./generated/OpenField";
import type { ArriveAsk } from "./generated/ArriveAsk";
import type { ArriveEnd } from "./generated/ArriveEnd";

export type { SessionOpenReq, Opened, Landed };

/** 一问的期限：铸名 ＋ 起 ccm（那台自己的期限 15 s）＋ 回程。到了 ⇒ 结果未知（同一张票再问一次那台认得出）。 */
const OPEN_BUDGET_MS = 25_000;
/** 等报到愿意等多久（那台照它减余量等）：慢机器上 ssh 握手 ＋ 冷启动在这之内；到了那台答「没看到」那一句，不说失败。 */
export const ARRIVE_BUDGET_MS = 45_000;

/**
 * 一问的结局：成了 · 那台拒了（码 · 那一句 · 哪一格不行 · 选不了号那一形）· 同一张票那一趟还在起 · 期限到（结果未知）· 够不着那台。
 * 没成的几形都带那一句与复制详情（没有 ⇒ 空串）。
 */
export type OpenEnd =
  | { kind: "opened"; opened: Opened }
  | { kind: "refused"; code: string; said: string; field: OpenField | null; unavailable: AccountUnavailable | null; detail: string }
  | { kind: "pending"; said: string; detail: string }
  | { kind: "timeout"; said: string; detail: string }
  | { kind: "unreachable"; said: string; detail: string };

const FIELDS: ReadonlySet<string> = new Set<OpenField>(["agent", "command", "cwd", "account", "place", "tmuxName"]);

/** 拒绝体的 `data`（`{field, unavailable}`）；读不出 ⇒ 当整体不行。 */
function refusalData(d: unknown): { field: OpenField | null; unavailable: AccountUnavailable | null } {
  if (!isObj(d) || !exactKeys(d, ["field", "unavailable"])) return { field: null, unavailable: null };
  const f = d.field;
  const u = d.unavailable;
  return {
    field: typeof f === "string" && FIELDS.has(f) ? (f as OpenField) : null,
    unavailable: isObj(u) && typeof u.requested === "string" && typeof u.listKnown === "boolean" ? (u as unknown as AccountUnavailable) : null,
  };
}

/** 只问、只分结局，不出声。 */
export async function askOpen(origin: Origin, req: SessionOpenReq): Promise<OpenEnd> {
  try {
    return { kind: "opened", opened: await callTyped(origin, "session-open", req, budgetWithin(OPEN_BUDGET_MS)) };
  } catch (e) {
    const detail = detailOf(e);
    const r = refusalIn(e);
    if (r?.code === "launch_pending") return { kind: "pending", said: saidOf(e), detail };
    if (r) return { kind: "refused", code: r.code, said: saidOf(e), ...refusalData(r.data), detail };
    // 发出去了、没等到回话 ⇒ 结果未知，不说失败。
    if (e instanceof ChanError && e.error.layer === "hop" && e.error.reach !== "NotSent") return { kind: "timeout", said: saidOf(e), detail };
    return { kind: "unreachable", said: saidOf(e), detail };
  }
}

/**
 * 照 `landed` 开不开那一行：单个恢复 · 接回一律开（`window` 不跑就没起；`started` · `in_place` 开了是接进去；`running` · `idle` 本来就在、开了当场说接上）。
 * 后台起的（批量 · 新起）只开 `window`，不一个个接进去。
 */
export const BACKGROUND_OPENS: ReadonlySet<Landed> = new Set<Landed>(["window"]);

export interface OpenHow {
  /** 报到了交给发起方自己说（新起会话给［切过去］）；缺 ⇒ 说「已启动」。主窗口里有目录的一律交占位标签页。 */
  onArrived?(sid: string, opened: Opened): void;
}

/** 占位标签页那一件（`launch-slot.ts` 画它）：哪台 · 起在哪个目录（那台展开过的）· 起在哪个 tmux 会话（开窗起的 ⇒ `null`）· 等它报到那一问（回包原样）。 */
export interface SlotSpec {
  origin: Origin;
  cwd: string;
  tmuxName: string | null;
  arrival: ArriveAsk;
}

/** 主窗口长得出占位标签页（`main.ts` 装一次）：接了 ⇒ `true`（它自己问报到、照回包画）。 */
let slots: ((spec: SlotSpec) => boolean) | null = null;
export function setLaunchSlots(fn: ((spec: SlotSpec) => boolean) | null): void {
  slots = fn;
}

/** 等报到的结局：那台答了（报到了 / 没见到那一句）· 问不到那台（那一句，说不清起没起来）。 */
export type ArriveSaid = ArriveEnd | { kind: "unknown"; said: string; detail: string };

/** 问那台等这一条报到（`arrival` 原样交回）。只问、不出声。 */
export async function askArrive(origin: Origin, ask: ArriveAsk, within: number = ARRIVE_BUDGET_MS): Promise<ArriveSaid> {
  try {
    return await callTyped(origin, "session-arrive", ask, budgetWithin(within));
  } catch (e) {
    return { kind: "unknown", said: saidOf(e), detail: detailOf(e) };
  }
}

/**
 * 起了一家之后：主窗口里有目录 ⇒ 交占位标签页（它自己问、照回包画）；否则问那台等报到 ——
 * 报到了 ⇒ 交 `onArrived`（缺 ⇒ 说 `title` ＋「已报出此会话」）；没见到 ⇒ 那台那一句原样上屏。
 */
export function awaitArrival(origin: Origin, ask: ArriveAsk, cwd: string | null, title: string, onArrived?: (sid: string) => void): void {
  if (cwd !== null && slots?.({ origin, cwd, tmuxName: ask.session, arrival: ask })) return;
  void askArrive(origin, ask).then((end) => {
    if (end.kind === "arrived") {
      if (onArrived) onArrived(end.sid);
      else toast(title, copyText("sessionOpen.arrived.body", { machine: machineName(origin) }), { level: "info" });
    } else if (end.kind === "missed") {
      toast(...lines(end.said), { level: "error" });
    } else {
      toast(end.said, "", { detail: end.detail });
    }
  });
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

/** 起 · 恢复 · 接回一次：问、说、开、交等报到。回「成了没有」（没成的那一路已经出过声）。 */
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
  return landOpened(origin, req, end.opened, how);
}

/** 那台答了「成了」之后：说附带的那一句 · 照 `landed` 开那一行 · 起了一家就等它报到。回「成了没有」（要开的那一行开不出 ⇒ 已说过、`false`）。 */
export async function landOpened(origin: Origin, req: SessionOpenReq, o: Opened, how: OpenHow = {}): Promise<boolean> {
  if (o.note !== null) toast(o.note, "", { level: "info" });
  const open = o.line !== null && (req.what.kind !== "new" || BACKGROUND_OPENS.has(o.landed));
  if (open && !(await openLine(origin, o.line!, o.cwd))) return false;
  if (o.arrival !== null) {
    const done = how.onArrived;
    awaitArrival(origin, o.arrival, o.cwd, copyText("sessionOpen.arrived.title"), done && ((sid) => done(sid, o)));
  } else if (open && (o.landed === "running" || o.landed === "idle")) {
    const body = o.session === null ? "" : copyText("sessionOpen.attached.body", { machine: machineName(origin), name: o.session });
    toast(copyText("sessionOpen.attached.title"), body, { level: "info" });
  }
  return true;
}

/** 没成的那几形各说一句；按钮只有两种：选不了号 ⇒ 改用那台给的替代（没有 ⇒ 不指定账号）· 不是这个会话的 ⇒ 对齐后重试。 */
function sayEnd(origin: Origin, req: SessionOpenReq, how: OpenHow, end: Exclude<OpenEnd, { kind: "opened" }>): void {
  if (end.kind === "pending") {
    toast(...lines(end.said), { level: "info" });
    return;
  }
  if (end.kind !== "refused") {
    toast(end.said, "", { detail: end.detail });
    return;
  }
  const [title, body] = lines(end.said);
  const again = (r: SessionOpenReq): Promise<void> => openSession(origin, r, how).then(() => undefined);
  switch (end.code) {
    case "account_unavailable": {
      const u = end.unavailable;
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
