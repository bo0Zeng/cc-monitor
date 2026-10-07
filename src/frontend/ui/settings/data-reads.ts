/**
 * 「文件与数据」那一份成品（那台后端的 `data-report`）：按机器问、严格收。
 * 本机那一栏先问 monitor 一次它自己进程的那几条事实（`footprint_client_facts`），原样带给本机后端（同足迹那一问）。
 * 设置窗左栏角标与主窗口状态栏那一枚读同一个数（[`choresOf`]），界面不另数。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "../ipc/chan-caller";
import { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { exactKeys, isObj } from "../ipc/decode";

/** 一次 stat 一批、读几份小文件：给 30 秒。 */
const DATA_BUDGET_MS = 30_000;
/** 记一个选择：读—改—写一份小文件，10 秒。 */
const MARK_BUDGET_MS = 10_000;

/** 撤回在哪：设置窗的页 · 栏 · 锚点（与「带目的地打开」同一种形状）。 */
export interface UndoAt {
  page: string;
  tab?: string;
  anchor?: string;
}

export interface ChangedFile {
  path: string;
  what: string;
  undo: UndoAt | null;
}

/** 「要你动手」一件的类（角标只数前三类）。 */
export type ChoreKind = "must" | "install" | "decide" | "installOptional" | "optional";
/** 那台判出的态（「已复制」只住界面）。 */
export type ChoreState = "todo" | "done" | "expired" | "blocked" | "declined";
/** 收着时那颗主按钮。 */
export type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
export interface DiffRow {
  n: number | null;
  op: "same" | "del" | "add";
  text: string;
}
export interface Chore {
  id: string;
  kind: ChoreKind;
  state: ChoreState;
  name: string;
  loc: string;
  said: string;
  why: string;
  steps: string[];
  diff: DiffRow[];
  copy: string | null;
  whole: string | null;
  wholeCovers: string[];
  file: string | null;
  go: UndoAt | null;
  howUrl: string | null;
  mask: string | null;
  action: ChoreAction;
}

export interface DataReport {
  home: string;
  changedFiles: ChangedFile[];
  todo: Chore[];
  /** 这台有没有 tmux；查不动 ⇒ `null`。 */
  tmux: boolean | null;
  /** 「要你动手」里进角标的件数（要做 ＋ 要装 ＋ 要你定，还没做完的）。 */
  chores: number;
}

const bad = (): never => {
  throw new ReplyUnreadable("data-report reply shape");
};
const str = (v: unknown): string => (typeof v === "string" ? v : bad());

function decodeUndo(v: unknown): UndoAt | null {
  if (v === null) return null;
  if (!isObj(v) || typeof v.page !== "string") return bad();
  const out: UndoAt = { page: v.page };
  for (const k of Object.keys(v)) {
    if (k === "page") continue;
    if ((k !== "tab" && k !== "anchor") || typeof v[k] !== "string") return bad();
    out[k] = v[k] as string;
  }
  return out;
}

const KINDS: readonly string[] = ["must", "install", "decide", "installOptional", "optional"];
const STATES: readonly string[] = ["todo", "done", "expired", "blocked", "declined"];
const ACTIONS: readonly string[] = ["copyCommand", "copySnippet", "decide", "locate", "how", "installFirst"];
const CHORE_KEYS = ["id", "kind", "state", "name", "loc", "said", "why", "steps", "diff", "copy", "whole", "wholeCovers", "file", "go", "howUrl", "mask", "action"];
const strOrNull = (v: unknown): string | null => (v === null ? null : str(v));
const oneOf = <T extends string>(v: unknown, set: readonly string[]): T => (typeof v === "string" && set.includes(v) ? (v as T) : bad());
const strs = (v: unknown): string[] => (Array.isArray(v) ? v.map(str) : bad());

function decodeChore(c: unknown): Chore {
  if (!isObj(c) || !exactKeys(c, CHORE_KEYS) || !Array.isArray(c.diff)) return bad();
  const diff = c.diff.map((d): DiffRow => {
    if (!isObj(d) || !exactKeys(d, ["n", "op", "text"])) return bad();
    if (d.n !== null && !(typeof d.n === "number" && Number.isInteger(d.n) && d.n > 0)) return bad();
    return { n: d.n as number | null, op: oneOf(d.op, ["same", "del", "add"]), text: str(d.text) };
  });
  return {
    id: str(c.id),
    kind: oneOf(c.kind, KINDS),
    state: oneOf(c.state, STATES),
    name: str(c.name),
    loc: str(c.loc),
    said: str(c.said),
    why: str(c.why),
    steps: strs(c.steps),
    diff,
    copy: strOrNull(c.copy),
    whole: strOrNull(c.whole),
    wholeCovers: strs(c.wholeCovers),
    file: strOrNull(c.file),
    go: decodeUndo(c.go),
    howUrl: strOrNull(c.howUrl),
    mask: strOrNull(c.mask),
    action: oneOf(c.action, ACTIONS),
  };
}

/** 记下「要你动手」里的一个选择（那台后端写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function markChore(origin: Origin, args: { op: "decline" | "undecline"; id: string } | { op: "selfPaste"; rc: string } | { op: "unselfPaste" }): Promise<void> {
  const target = isLocalOrigin(origin) ? LOCAL_ORIGIN : origin;
  try {
    const body = jsonBody(args);
    const budget = budgetWithin(MARK_BUDGET_MS);
    await chan.call(target, "chores-mark", body, budget);
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** `data-report` 的应答 ⇒ 成品；多一格缺一格 · 类型不对 ⇒ 抛。 */
export function decodeDataReport(v: unknown): DataReport {
  if (!isObj(v) || !exactKeys(v, ["home", "changedFiles", "todo", "tmux", "chores"])) return bad();
  if (!Array.isArray(v.changedFiles) || !Array.isArray(v.todo)) return bad();
  if (v.tmux !== null && typeof v.tmux !== "boolean") return bad();
  if (typeof v.chores !== "number" || !Number.isInteger(v.chores) || v.chores < 0) return bad();
  const changedFiles = v.changedFiles.map((c): ChangedFile => {
    if (!isObj(c) || !exactKeys(c, ["path", "what", "undo"])) return bad();
    return { path: str(c.path), what: str(c.what), undo: decodeUndo(c.undo) };
  });
  const todo = v.todo.map(decodeChore);
  return { home: str(v.home), changedFiles, todo, tmux: v.tmux as boolean | null, chores: v.chores as number };
}

/** 问那台（本机带 monitor 那几条事实）。问不到 / 形状不对 ⇒ 抛一句人话。 */
export async function readDataReport(origin: Origin): Promise<DataReport> {
  const local = isLocalOrigin(origin);
  const body = jsonBody(local ? { client: await commands.footprint_client_facts() } : {});
  const target = local ? LOCAL_ORIGIN : origin;
  let reply: Uint8Array;
  try {
    const budget = budgetWithin(DATA_BUDGET_MS);
    reply = await chan.call(target, "data-report", body, budget);
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
  try {
    return decodeDataReport(readJson(reply));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** 那台进角标的件数（问不到 ⇒ `null`，不当 0）。设置窗角标与主窗口状态栏读这一个。 */
export async function choresOf(origin: Origin): Promise<number | null> {
  try {
    return (await readDataReport(origin)).chores;
  } catch {
    return null;
  }
}
