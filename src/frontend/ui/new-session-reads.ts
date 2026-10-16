/**
 * 起新会话框的三问（那台后端判，界面只读）：`session-new-facts`（框打开时：最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格）·
 * `session-new-dir`（目录失焦：在不在 · 那台会铸的终端名）· `session-new`（点［新建］：起）。三问都是 typed 帧命令（入参 · 回包类型从 Rust 生成，形状由握手指纹核过）。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { detailOf, saidOf } from "./kit/detail";
import { exactKeys, isObj } from "./ipc/decode";
import type { DirChecked } from "./generated/DirChecked";
import type { NewFacts } from "./generated/NewFacts";
import type { NewRequest } from "./generated/NewRequest";
import type { SessionNew } from "./generated/SessionNew";
import type { SessionNewField } from "./generated/SessionNewField";
import type { AccountUnavailable } from "./generated/AccountUnavailable";

/** 读两问：扫记录目录 ＋ 列一次 tmux；15 s 盖住远端握手。 */
const READ_BUDGET_MS = 15_000;
/** 起：铸名 ＋ 起 ccm（那台自己的期限 15 s）＋ 回程。到了 ⇒「启动无应答」，按结果未知处理。 */
export const NEW_BUDGET_MS = 25_000;

/** 框打开时那一问的回包（分叉源会话的三格同 `session-fork` 的 `launch`）。 */
export type { NewFacts };

/** 框打开时问一次：typed 帧命令（回包类型从 Rust 生成，形状由握手指纹核过）。 */
export async function askFacts(origin: Origin, fork: { sid: string; uuid: string } | null): Promise<NewFacts> {
  return callTyped(origin, "session-new-facts", fork ? { forkOf: fork.sid, at: fork.uuid } : {}, budgetWithin(READ_BUDGET_MS));
}

/** 目录失焦：typed 帧命令（回包类型从 Rust 生成，形状由握手指纹核过）。 */
export async function askDir(origin: Origin, cwd: string, forkOf: string | null): Promise<DirChecked> {
  return callTyped(origin, "session-new-dir", forkOf ? { cwd, forkOf } : { cwd }, budgetWithin(READ_BUDGET_MS));
}

/** 点［新建］交给那台的那一份（typed：入参类型从 Rust 生成）。 */
export type { NewRequest };

/**
 * 起的结局：起了 · 某一格不行 / 整体不行（那台说的码与那一句）· 同一张票那一趟还在起 · 期限到（结果未知）· 够不着那台。
 * 不行的几形都带复制详情（`detail`，没有 ⇒ 空串）。
 */
export type NewResult =
  | { kind: "ok"; reply: SessionNew }
  | { kind: "refused"; code: string; said: string; field: SessionNewField | null; unavailable: AccountUnavailable | null; detail: string }
  | { kind: "pending"; said: string; detail: string }
  | { kind: "timeout"; detail: string }
  | { kind: "unreachable"; said: string; detail: string };

/** 拒绝体的 `data`（`{field, unavailable}`）；读不出 ⇒ 当整体不行。 */
function refusalData(d: unknown): { field: SessionNewField | null; unavailable: AccountUnavailable | null } {
  if (!isObj(d) || !exactKeys(d, ["field", "unavailable"])) return { field: null, unavailable: null };
  const f = d.field;
  const fields: SessionNewField[] = ["agent", "command", "cwd", "account", "place", "tmuxName"];
  const u = d.unavailable;
  return {
    field: typeof f === "string" && (fields as string[]).includes(f) ? (f as SessionNewField) : null,
    unavailable: isObj(u) && typeof u.requested === "string" ? (u as unknown as AccountUnavailable) : null,
  };
}

export async function askNew(origin: Origin, req: NewRequest): Promise<NewResult> {
  const budget = budgetWithin(NEW_BUDGET_MS);
  let reply: SessionNew;
  try {
    reply = await callTyped(origin, "session-new", req, budget);
  } catch (e) {
    const detail = detailOf(e);
    const r = refusalIn(e);
    if (r?.code === "launch_pending") return { kind: "pending", said: saidOf(e), detail };
    if (r) return { kind: "refused", code: r.code, said: saidOf(e), ...refusalData(r.data), detail };
    // 发出去了、没等到回话（期限到 · 半路断了）⇒ 结果未知：同一张票再问一次（那台认得出是不是同一趟），不说失败。
    if (e instanceof ChanError && e.error.layer === "hop" && e.error.reach !== "NotSent") return { kind: "timeout", detail };
    return { kind: "unreachable", said: saidOf(e), detail };
  }
  return { kind: "ok", reply };
}
