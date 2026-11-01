/**
 * **界面说终端管理那几条命令**：终端名单（`terminals-list`）· 抓一屏（`terminal-preview`）· 送字送键（`terminal-input`）·
 * 接管 · 交还输入（`terminal-take-input` · `terminal-release-input`，托管那一形）经通道直接问那台后端，本机远端同一条路；
 * 结束（`terminals-end`）只交本机后端一条，它按台分组问到各台、合好那一句。
 *
 * 目标只认名单里的终端（`terminal` 句柄）或会话 ID（`sid`），不收 tmux 目标串。
 *
 * 收法：三条都是 typed 命令（入参 · 回包从 Rust 类型生成，`generated/TypedCommands.ts`）⇒ 形状由握手指纹管，这边只挑要用的那几格。
 * 实时预览的帧（`terminal_screen`）的 `view` 就是 `terminal-preview` 的回包类型（帧的形状指纹核过），同一个 [`shotOf`] 收。
 * 预览要带颜色的成品（`color: true`）：每行 `text` ＋ 颜色段交给画面（`terminal-screen.ts`）；纯文字那一份是每行 `text` 用换行接起来，与那一屏逐行相同。
 *
 * 期限：抓一屏 · 送字送键 · 接管交还 20 秒，结束 20 秒 ＋ 每项 6 秒，名单 15 秒（远端没连着还要握手）。
 */
import { copyText } from "./copy-table";
import { settleTyped } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { screenText, type ScreenLine } from "./terminal-screen";
import type { Preview } from "./generated/Preview";
import type { Sent } from "./generated/Sent";
import type { SentAct } from "./generated/SentAct";
import type { TerminalsList } from "./generated/TerminalsList";
import type { TermInput } from "./generated/TermInput";
import type { TerminalItem } from "./generated/TerminalItem";
import type { TerminalsEndReply } from "./generated/TerminalsEndReply";

/** 抓一屏的期限。 */
const PREVIEW_BUDGET_MS = 20_000;
/** 列名单的期限。 */
const LIST_BUDGET_MS = 15_000;

/** 名单里这边用到的那几格。 */
export interface TerminalRow {
  /** 句柄（不透明，只拿来回指）。 */
  terminal: string;
  /** 终端的名字（tmux：会话名；托管：`ccm --attach <名>` 认的那个名）。 */
  name: string;
  /** 里面跑着的会话（没有 ⇒ `null`）。 */
  sid: string | null;
  /** 此刻连着它的终端窗口几个（0 ＝ 后台）。 */
  clients: number;
  /** 输入怎么进：`shared` ＝ 各端都能打字（tmux）· `held` ＝ 托管那一形，只有持有者的进（谁持有在 `holder`）。 */
  input: TerminalsList["terminals"][number]["input"]["kind"];
  /** 托管那一形：此刻谁在输入（后端写好的那一句 ＋ 是不是这一端持有）；tmux 那一形没有这一格。 */
  holder?: TerminalHolder;
  /** Claude 已退出、终端还开着。 */
  programExited: boolean;
  /** 能不能送字 / 送键：`null` ＝ 能；否则为什么不能（后端写好的那一句）。 */
  inputNo: string | null;
}

/** 托管终端此刻的输入持有者：给人看的那一句（后端写好）· 是不是这一端。 */
export interface TerminalHolder {
  said: string;
  mine: boolean;
}

/** 输入那一格 ⇒ 持有者（tmux 那一形 ⇒ 没有）。名单那一行与接管 · 交还的回包同一个读法。 */
export function holderOf(input: TermInput): TerminalHolder | undefined {
  return input.kind === "held" ? { said: input.holder.said, mine: input.holder.mine } : undefined;
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

/** 抓一次那个终端此刻的一屏（只读快照，不过身份门），连指纹与抓的时刻一起（终端页要拿指纹送字）。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function previewShot(origin: Origin, target: TerminalTarget): Promise<TerminalShot> {
  const budget = budgetWithin(PREVIEW_BUDGET_MS);
  const v = await settleTyped(origin, "terminal-preview", callTyped(origin, "terminal-preview", { ...target, color: true }, budget));
  return shotOf(v);
}

/** `terminal-preview` 的回包（实时预览每一帧的 `view` 也是它）⇒ 那一屏（带颜色段）＋ 指纹 ＋ 几点抓的。 */
export function shotOf(v: Preview): TerminalShot {
  const lines = v.lines.map((l) => ({ text: l.text, spans: l.spans ?? [] }));
  return { lines, text: screenText(lines), screen: v.screen, atText: v.captured_at_text };
}

/** 后端没给那一句（契约外）时落的那一个词：不按码猜。 */
const noSaid = (): string => copyText("reason.code.refused");

/** `terminals-list` 的回包 ⇒ 这边要的那几格。 */
export function rowsOf(v: TerminalsList): TerminalRow[] {
  return v.terminals.map((r) => ({
    terminal: r.terminal,
    name: r.name,
    sid: r.session?.sid ?? null,
    clients: r.clients.length,
    input: r.input.kind,
    holder: holderOf(r.input),
    programExited: r.state === "program_exited",
    inputNo: r.can.input === true ? null : r.can.input.said,
  }));
}

/** 那台此刻的终端名单。 */
export async function listTerminals(origin: Origin): Promise<TerminalRow[]> {
  const budget = budgetWithin(LIST_BUDGET_MS);
  return rowsOf(await settleTyped(origin, "terminals-list", callTyped(origin, "terminals-list", {}, budget)));
}

/** 送什么：一段字（`enter` ＝ 之后补一个回车）或一颗键（有限键表，后端定）。 */
export type TerminalSend = { text: string; enter: boolean } | { key: string };

/** 送字 / 送键的回话：送到了 · 不知道送没送到 · 被拒（那一句后端写好；画面变了时带新指纹）；`acts` 是后端判好的下一步（再发一次 · 重抓 · 重读名单）。 */
export type TerminalSent =
  | { result: "delivered"; acts: SentAct[] }
  | { result: "unsure"; acts: SentAct[] }
  | { result: "refused"; said: string; screen: string | null; acts: SentAct[] };

/** 送字 / 送键的期限（与抓一屏同）。 */
const INPUT_BUDGET_MS = 20_000;

/** `terminal-input` 的回包 ⇒ 结局（被拒的那一句后端写好；缺了落「被拒」，不按码猜）。 */
export function sentOf(v: Sent): TerminalSent {
  const acts = v.acts ?? [];
  if (v.result !== "refused") return { result: v.result, acts };
  return { result: "refused", said: v.said ?? noSaid(), screen: v.screen ?? null, acts };
}

/** 往那个终端送一段字或一颗键；带上看到的那一屏的指纹（画面已经变了 ⇒ 后端不送）。失败 ⇒ 抛 `ControlError`。 */
export async function sendToTerminal(origin: Origin, terminal: string, what: TerminalSend, seen: string | null): Promise<TerminalSent> {
  const req = { terminal, ...what, ...(seen !== null ? { seen_screen: seen } : {}) };
  const budget = budgetWithin(INPUT_BUDGET_MS);
  return sentOf(await settleTyped(origin, "terminal-input", callTyped(origin, "terminal-input", req, budget)));
}

/** 结束 · 接管 · 交还的期限（与送字同）。 */
const HOLD_BUDGET_MS = 20_000;

/** 结束一批时每项多给的那一段（那台逐项结束）。 */
const END_EACH_MS = 6_000;

/**
 * 结束一批终端（**破坏性**：调用方先确认）：交本机后端一条 `terminals-end`（`items` 照交进来的先后，跨几台也只这一发）；
 * 按台分组 · 每台结束（两种宿主同一条，身份门 · 窗口门在那台先过、对句柄下手）· 合那一句都在后端，回包照摆。问不到本机后端 ⇒ 抛。
 */
export async function endTerminals(items: readonly TerminalItem[]): Promise<TerminalsEndReply> {
  const budget = budgetWithin(HOLD_BUDGET_MS + END_EACH_MS * items.length);
  return settleTyped(LOCAL_ORIGIN, "terminals-end", callTyped(LOCAL_ORIGIN, "terminals-end", { items: [...items] }, budget));
}

/** 替这一端接管托管终端的输入（之后送字经它）；回那一行此刻的输入那一格。失败 ⇒ 抛（那一句后端写好）。 */
export async function takeInput(origin: Origin, terminal: string): Promise<TermInput> {
  return (await settleTyped(origin, "terminal-take-input", callTyped(origin, "terminal-take-input", { terminal }, budgetWithin(HOLD_BUDGET_MS)))).input;
}

/** 交还托管终端的输入（交给最近连上的真终端，没有就空着）；回那一行此刻的输入那一格。 */
export async function releaseInput(origin: Origin, terminal: string): Promise<TermInput> {
  return (await settleTyped(origin, "terminal-release-input", callTyped(origin, "terminal-release-input", { terminal }, budgetWithin(HOLD_BUDGET_MS)))).input;
}
