/**
 * **界面说终端管理那几条读命令**：终端名单（`terminals-list`）· 抓一屏（`terminal-preview`）。经通道直接问那台后端，本机远端同一条路。
 *
 * 目标只认名单里的终端（`terminal` 句柄）或会话 ID（`sid`），不收 tmux 目标串。只有 tmux 会话名在手的（起会话之后等它报出来的那一方）
 * 先在名单里按名字认出那一行，再按句柄抓。
 *
 * 收法：名单与预览都是只加不改的成品（手机端也吃）⇒ 这边要用的那几格缺 / 类型不对才抛「两端契约对不上」，多出来的格照收。
 * 预览只要纯文本（`color: false`），每行 `text` 用换行接起来，与那一屏逐行相同。
 *
 * 期限：抓一屏 20 秒，名单 15 秒（远端没连着还要握手）。
 */
import { copyText } from "./copy-table";
import { isObj, settle, unreadable, type Refusals } from "./control-said";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** 抓一屏的期限。 */
const PREVIEW_BUDGET_MS = 20_000;
/** 列名单的期限。 */
const LIST_BUDGET_MS = 15_000;

/** 名单里这边用到的那几格。 */
export interface TerminalRow {
  /** 句柄（不透明，只拿来回指）。 */
  terminal: string;
  /** 这一版宿主是 tmux：所在的 tmux 会话名。 */
  tmuxName: string;
}

/** 怎么指一个终端：名单里的句柄，或挂在它上面的会话 ID。 */
export type TerminalTarget = { terminal: string } | { sid: string };

/** 抓屏的拒绝码 ⇒ 一句话（`target` 是给人看的那个名字）。认不出的码原样带出去。 */
function previewRefusals(target: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "no_tmux":
          return copyText("terminalReads.preview.noTmux", { target, detail });
        case "no_server":
          return copyText("terminalReads.preview.noServer", { target, detail });
        case "no_such_session":
          return copyText("terminalReads.preview.noSuchSession", { target, detail });
        case "capture_failed":
          return copyText("terminalReads.preview.failed", { target, detail });
        case "bad_target":
        case "invalid_args":
          return copyText("terminalReads.preview.badTarget", { target, detail });
        case "not_known":
          return copyText("terminalReads.preview.notKnown", { target, detail });
        case "ambiguous":
          return copyText("terminalReads.preview.ambiguous", { target, detail });
        case "unobservable":
          return copyText("terminalReads.preview.unobservable", { target, detail });
        default:
          return detail.trim() !== ""
            ? copyText("terminalReads.preview.otherCode", { target, detail })
            : copyText("terminalReads.preview.noReason", { target });
      }
    },
    noReason: () => copyText("terminalReads.preview.noReason", { target }),
  };
}

/** `terminal-preview`（`color: false`）的成品 ⇒ 那一屏的文本。`lines` 缺 / 某行没有字符串 `text` ⇒ 抛。空屏是合法的成功。 */
export function decodePreview(origin: Origin, v: unknown): string {
  const lines = isObj(v) ? v.lines : undefined;
  if (!Array.isArray(lines) || !lines.every((l) => isObj(l) && typeof l.text === "string")) {
    throw unreadable(origin, "terminal-preview", "has no `lines: [{text}]`");
  }
  return (lines as { text: string }[]).map((l) => l.text).join("\n");
}

/** 抓一次那个终端此刻的一屏（只读快照，不过身份门）。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function previewText(origin: Origin, target: TerminalTarget, label: string): Promise<string> {
  const body = jsonBody({ ...target, color: false });
  const budget = budgetWithin(PREVIEW_BUDGET_MS);
  const v = await settle(origin, "terminal-preview", chan.call(origin, "terminal-preview", body, budget), previewRefusals(label));
  return decodePreview(origin, v);
}

/** `terminals-list` 的成品 ⇒ 这边要的那几格。 */
export function decodeTerminals(origin: Origin, v: unknown): TerminalRow[] {
  const rows = isObj(v) ? v.terminals : undefined;
  if (!Array.isArray(rows) || !rows.every((r) => isObj(r) && typeof r.terminal === "string" && typeof r.tmux_name === "string")) {
    throw unreadable(origin, "terminals-list", "has no `terminals: [{terminal, tmux_name}]`");
  }
  return (rows as { terminal: string; tmux_name: string }[]).map((r) => ({ terminal: r.terminal, tmuxName: r.tmux_name }));
}

/** 那台此刻的终端名单（为抓 `label` 那一屏而问：拒绝的话按抓屏那一套说）。 */
async function listTerminals(origin: Origin, label: string): Promise<TerminalRow[]> {
  const body = jsonBody({});
  const budget = budgetWithin(LIST_BUDGET_MS);
  const v = await settle(origin, "terminals-list", chan.call(origin, "terminals-list", body, budget), previewRefusals(label));
  return decodeTerminals(origin, v);
}

/** 按 tmux 会话名抓一屏：先在名单里认出那一行（第一个同名的），再按句柄抓。名单里没有 ⇒ 抛那句「不在名单」。 */
export async function previewByTmuxName(origin: Origin, tmuxName: string): Promise<string> {
  const row = (await listTerminals(origin, tmuxName)).find((r) => r.tmuxName === tmuxName);
  if (row === undefined) throw new Error(copyText("terminalReads.preview.notKnown", { target: tmuxName, detail: "terminals-list" }));
  return previewText(origin, { terminal: row.terminal }, tmuxName);
}
