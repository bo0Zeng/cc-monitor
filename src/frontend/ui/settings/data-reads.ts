/**
 * 「文件与数据」那一份成品（那台后端的 `data-report`）：按机器问、严格收。
 * 本机那一栏先问 monitor 一次它自己进程的那几条事实（`footprint_client_facts`），原样带给本机后端（同足迹那一问）。
 * 设置窗左栏角标与主窗口状态栏那一枚读同一个数（[`choresOf`]），界面不另数。
 */
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "../ipc/chan-caller";
import { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";

/** 一次 stat 一批、读几份小文件：给 30 秒。 */
const DATA_BUDGET_MS = 30_000;

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

export interface NeedsInstall {
  id: string;
  name: string;
  what: string;
  /** 缺了起不了会话（进角标）。 */
  required: boolean;
  howUrl: string | null;
}

export interface DataReport {
  home: string;
  changedFiles: ChangedFile[];
  needsInstall: NeedsInstall[];
  /** 这台有没有 tmux；查不动 ⇒ `null`。 */
  tmux: boolean | null;
  /** 「要你动手」里进角标的件数。 */
  chores: number;
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, keys: string[]): boolean => {
  const got = Object.keys(o).sort();
  const want = [...keys].sort();
  return got.length === want.length && got.every((k, i) => k === want[i]);
};
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

/** `data-report` 的应答 ⇒ 成品；多一格缺一格 · 类型不对 ⇒ 抛。 */
export function decodeDataReport(v: unknown): DataReport {
  if (!isObj(v) || !sameKeys(v, ["home", "changedFiles", "needsInstall", "tmux", "chores"])) return bad();
  if (!Array.isArray(v.changedFiles) || !Array.isArray(v.needsInstall)) return bad();
  if (v.tmux !== null && typeof v.tmux !== "boolean") return bad();
  if (typeof v.chores !== "number" || !Number.isInteger(v.chores) || v.chores < 0) return bad();
  const changedFiles = v.changedFiles.map((c): ChangedFile => {
    if (!isObj(c) || !sameKeys(c, ["path", "what", "undo"])) return bad();
    return { path: str(c.path), what: str(c.what), undo: decodeUndo(c.undo) };
  });
  const needsInstall = v.needsInstall.map((n): NeedsInstall => {
    if (!isObj(n) || !sameKeys(n, ["id", "name", "what", "required", "howUrl"])) return bad();
    if (typeof n.required !== "boolean" || (n.howUrl !== null && typeof n.howUrl !== "string")) return bad();
    return { id: str(n.id), name: str(n.name), what: str(n.what), required: n.required, howUrl: n.howUrl as string | null };
  });
  return { home: str(v.home), changedFiles, needsInstall, tmux: v.tmux as boolean | null, chores: v.chores as number };
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
