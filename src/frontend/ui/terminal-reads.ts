/**
 * **界面说终端管理那几条命令**：终端名单（`terminals-list`）· 抓一屏（`terminal-preview`）· 送字送键（`terminal-input`）。经通道直接问那台后端，本机远端同一条路。
 *
 * 目标只认名单里的终端（`terminal` 句柄）或会话 ID（`sid`），不收 tmux 目标串。只有 tmux 会话名在手的（起会话之后等它报出来的那一方）
 * 先在名单里按名字认出那一行，再按句柄抓。
 *
 * 收法：三条都是 typed 命令（入参 · 回包从 Rust 类型生成，`generated/TypedCommands.ts`）⇒ 形状由握手指纹管，这边只挑要用的那几格。
 * 实时预览的帧（`terminal_screen`）不是命令回包，`view` 仍按「要用的那几格缺 / 类型不对才抛」收（[`decodeShot`]）。
 * 预览要带颜色的成品（`color: true`）：每行 `text` ＋ 颜色段交给画面（`terminal-screen.ts`）；纯文字那一份是每行 `text` 用换行接起来，与那一屏逐行相同。
 *
 * 期限：抓一屏 20 秒，送字送键 20 秒，名单 15 秒（远端没连着还要握手）。
 */
import { copyText } from "./copy-table";
import { asSaid, settleTyped, unreadable, type Refusals } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { decodeScreenLines, screenText, type ScreenLine } from "./terminal-screen";
import type { Sent } from "./generated/Sent";
import type { TerminalsList } from "./generated/TerminalsList";

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
  /** 里面跑着的会话（没有 ⇒ `null`）。 */
  sid: string | null;
  /** 此刻连着它的终端窗口几个（0 ＝ 后台）。 */
  clients: number;
  /** 输入在谁手里：`shared` ＝ 各端都能打字（tmux）· `nobody` · 别的（托管那一形的持有者）原样带着。 */
  input: string;
  /** Claude 已退出、终端还开着。 */
  programExited: boolean;
  /** 能不能送字 / 送键：`null` ＝ 能；否则为什么不能（后端写好的那一句）。 */
  inputNo: string | null;
}

/** 抓到的一屏：每一行（带颜色段）· 纯文字 · 指纹（送字时带回去）· 几点抓的（秒）。 */
export interface TerminalShot {
  lines: ScreenLine[];
  text: string;
  screen: string;
  /** 几点抓的：那台本地钟上的 `HH:MM:SS`（后端写好，界面照抄）。 */
  atText: string;
}

/** 怎么指一个终端：名单里的句柄，或挂在它上面的会话 ID。 */
export type TerminalTarget = { terminal: string } | { sid: string };

/** 抓屏被拒：那一句由那台后端写好（`src/backend/control/terminals_wire.rs::preview_said`）；拒绝体读不出 ⇒ 无原因那一句（`target` 是给人看的那个名字）。 */
function previewRefusals(target: string): Refusals {
  return asSaid(() => copyText("terminalReads.preview.noReason", { target }));
}

/** `terminal-preview` 的回包（实时预览那一帧的 `view` 是同一份）⇒ 那一屏 ＋ 指纹 ＋ 几点抓的。`screen` / `captured_at_text` 缺 ⇒ 抛。 */
export function decodeShot(origin: Origin, v: unknown): TerminalShot {
  const lines = decodeScreenLines(origin, v);
  const o = v as Record<string, unknown>;
  if (typeof o.screen !== "string" || typeof o.captured_at_text !== "string") throw unreadable(origin, "terminal-preview", "has no `screen` / `captured_at_text`");
  return { lines, text: screenText(lines), screen: o.screen, atText: o.captured_at_text };
}

/** 抓一次那个终端此刻的一屏（只读快照，不过身份门）。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function previewText(origin: Origin, target: TerminalTarget, label: string): Promise<string> {
  return (await previewShot(origin, target, label)).text;
}

/** 同上，连指纹与抓的时刻一起（终端页要拿指纹送字）。 */
export async function previewShot(origin: Origin, target: TerminalTarget, label: string): Promise<TerminalShot> {
  const budget = budgetWithin(PREVIEW_BUDGET_MS);
  const v = await settleTyped(origin, "terminal-preview", callTyped(origin, "terminal-preview", { ...target, color: true }, budget), previewRefusals(label));
  return decodeShot(origin, v);
}

/** 后端没给那一句（契约外）时落的那一个词：不按码猜。 */
const noSaid = (): string => copyText("terminal.why.other");

/** `terminals-list` 的回包 ⇒ 这边要的那几格。 */
export function rowsOf(v: TerminalsList): TerminalRow[] {
  return v.terminals.map((r) => ({
    terminal: r.terminal,
    tmuxName: r.tmux_name,
    sid: r.session?.sid ?? null,
    clients: r.clients.length,
    input: r.input,
    programExited: r.state === "program_exited",
    inputNo: r.can.input === true ? null : r.can.input.said,
  }));
}

/** 那台此刻的终端名单（为抓 `label` 那一屏而问：拒绝的话按抓屏那一套说）。 */
export async function listTerminals(origin: Origin, label: string): Promise<TerminalRow[]> {
  const budget = budgetWithin(LIST_BUDGET_MS);
  return rowsOf(await settleTyped(origin, "terminals-list", callTyped(origin, "terminals-list", {}, budget), previewRefusals(label)));
}

/** 按 tmux 会话名抓一屏：先在名单里认出那一行（第一个同名的），再按句柄抓。名单里没有 ⇒ 抛那句「不在名单」。 */
export async function previewByTmuxName(origin: Origin, tmuxName: string): Promise<string> {
  const row = (await listTerminals(origin, tmuxName)).find((r) => r.tmuxName === tmuxName);
  if (row === undefined) throw new Error(copyText("terminalReads.preview.notKnown"));
  return previewText(origin, { terminal: row.terminal }, tmuxName);
}

/** 送什么：一段字（`enter` ＝ 之后补一个回车）或一颗键（有限键表，后端定）。 */
export type TerminalSend = { text: string; enter: boolean } | { key: string };

/** 送字 / 送键的回话：送到了 · 不知道送没送到 · 被拒（原因码给程序认 · 那一句后端写好；画面变了时带新指纹）。 */
export type TerminalSent = { result: "delivered" } | { result: "unsure" } | { result: "refused"; why: string; said: string; screen: string | null };

/** 送字 / 送键的期限（与抓一屏同）。 */
const INPUT_BUDGET_MS = 20_000;

/** 送字 / 送键被拒：那一句由那台后端写好；拒绝体读不出 ⇒ 无原因那一句。 */
function inputRefusals(target: string): Refusals {
  return asSaid(() => copyText("terminalReads.input.noReason", { target }));
}

/** `terminal-input` 的回包 ⇒ 结局（被拒的那一句后端写好；缺了落「被拒」，不按码猜）。 */
export function sentOf(v: Sent): TerminalSent {
  if (v.result !== "refused") return { result: v.result };
  return { result: "refused", why: v.why ?? "", said: v.said ?? noSaid(), screen: v.screen ?? null };
}

/** 往那个终端送一段字或一颗键；带上看到的那一屏的指纹（画面已经变了 ⇒ 后端不送）。失败 ⇒ 抛 `ControlError`。 */
export async function sendToTerminal(origin: Origin, terminal: string, what: TerminalSend, seen: string | null, label: string): Promise<TerminalSent> {
  const req = { terminal, ...what, ...(seen !== null ? { seen_screen: seen } : {}) };
  const budget = budgetWithin(INPUT_BUDGET_MS);
  return sentOf(await settleTyped(origin, "terminal-input", callTyped(origin, "terminal-input", req, budget), inputRefusals(label)));
}
