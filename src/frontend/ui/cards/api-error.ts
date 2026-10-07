/**
 * 报错卡与重试细条。两种记录：
 *
 * 1. `assistant` + `isApiErrorMessage` —— 重试耗尽 / 不可重试的最终失败 ⇒ 报错卡：「本轮中断 · {原因}」＋ 下一步 ＋「原文」折叠。
 * 2. `system` + `subtype:"api_error"` —— 这一次失败、CLI 要重试 ⇒ 重试细条；相邻的几条**合成一条、原地更新**
 *    （第几次 / 最多几次 / 起始时刻）。一串的结局是会话事实（`retries`，后端按首条的 uuid 给，[`applyRetries`]）：
 *    接上了 ⇒ 变淡「重试 ×N 后恢复」· 没接上 ⇒ 细条收起、次数进紧跟着的报错卡 · 人打断了 ⇒ 变淡「重试 ×N 后中断」。
 *
 * 原因一词由后端判（记录成品的 `apiReason`：服务器过载 · 额度满 · 网络中断 · 需登录 · 上下文超长 · 原因不明）；
 * 界面不认状态码、不读报错对象的形状。相邻合并是排版（`render-stream-record.ts` 按时间线邻居做）。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import type { ApiReason } from "../generated/ApiReason";
import type { RetryOutcome } from "../session-reads";

/** 原因一词（后端没给 ⇒ 原因不明）。 */
export function reasonWord(reason: ApiReason | undefined): string {
  switch (reason) {
    case "overloaded":
      return copyText("apiError.reason.overloaded");
    case "quota":
      return copyText("apiError.reason.quota");
    case "network":
      return copyText("apiError.reason.network");
    case "auth":
      return copyText("apiError.reason.auth");
    case "context":
      return copyText("apiError.reason.context");
    default:
      return copyText("apiError.reason.unknown");
  }
}

/** 报错卡。`text` 是报错原文（进「原文」折叠，含状态码）。 */
export function buildApiErrorCard(args: { timeLabel: string; reason?: ApiReason; text: string; status?: number | null }): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-api-error";

  const head = document.createElement("div");
  head.className = "api-error-head";
  const label = document.createElement("span");
  label.className = "api-error-label";
  label.textContent = copyText("apiError.card.title", { reason: reasonWord(args.reason) });
  const ts = document.createElement("span");
  ts.className = "api-error-ts";
  ts.textContent = args.timeLabel;
  head.append(label, ts);

  const next = document.createElement("div");
  next.className = "api-error-next";
  const nextText = document.createElement("span");
  nextText.className = "api-error-next-text";
  nextText.textContent = copyText("apiError.card.nextBare");
  next.appendChild(nextText);

  const raw = document.createElement("details");
  raw.className = "api-error-raw";
  const sum = document.createElement("summary");
  sum.textContent = copyText("apiError.card.raw");
  const body = document.createElement("pre");
  body.className = "api-error-body";
  const original = args.text || copyText("apiError.card.noDetail");
  body.textContent = typeof args.status === "number" ? `${args.status} · ${original}` : original;
  raw.append(sum, body);

  // 去它的终端那两颗：卡只摆出来，看不看得见由所在消息流根上那两个标记管（宿主按会话头那一道设，`data-can-front` · `data-can-attach`）。
  const acts = document.createElement("span");
  acts.className = "api-error-acts";
  for (const [act, label] of [
    ["front", copyText("terminal.head.front")],
    ["attach", copyText("sessionHead.act.openTerm")],
  ] as const) {
    const b = button({ label, size: "compact" });
    b.dataset.act = act;
    acts.appendChild(b);
  }
  next.appendChild(acts);

  card.append(head, next, raw);
  return card;
}

/**
 * 重试细条（一条 = 相邻的一串重试）。状态住 `data-*`：结局（`data-state`，会话事实给）· 首条的 id（`data-run`，按它读结局）。
 * `outcome` ＝ 建卡时会话事实已经说了的结局；没说 ⇒ 在重试。
 */
export function buildApiRetryCard(args: {
  timeLabel: string;
  reason?: ApiReason;
  retryAttempt?: number | null;
  maxRetries?: number | null;
  id?: string;
  outcome?: RetryOutcome;
}): HTMLElement {
  const line = document.createElement("div");
  line.className = "card card-api-retry";
  line.dataset.state = "retrying";
  if (args.id) line.dataset.run = args.id;
  line.dataset.reason = args.reason ?? "unknown";
  line.dataset.since = args.timeLabel;
  line.dataset.last = args.timeLabel;
  // typeof：serde 把 Option::None 序列化成显式 null。
  line.dataset.attempt = typeof args.retryAttempt === "number" ? String(args.retryAttempt) : "";
  line.dataset.max = typeof args.maxRetries === "number" ? String(args.maxRetries) : "";
  if (args.outcome) line.dataset.state = args.outcome;
  paintRetry(line);
  return line;
}

export function isRetryBar(el: Element | null | undefined): el is HTMLElement {
  return el instanceof HTMLElement && el.classList.contains("card-api-retry");
}

export function isApiErrorCard(el: Element | null | undefined): el is HTMLElement {
  return el instanceof HTMLElement && el.classList.contains("card-api-error");
}

/** 又来一次重试：并进前一条（次数取后来的，起始时刻留前一条的）。 */
export function mergeRetry(into: HTMLElement, from: HTMLElement): void {
  if (from.dataset.attempt) into.dataset.attempt = from.dataset.attempt;
  if (from.dataset.max) into.dataset.max = from.dataset.max;
  into.dataset.reason = from.dataset.reason ?? into.dataset.reason;
  into.dataset.last = from.dataset.last ?? into.dataset.last;
  paintRetry(into);
}

/**
 * 会话事实说的结局（首条 id → 结局）照画到 `root` 里的每条细条上。幂等：每到一份事实重摆一遍。没接上 ⇒ 细条收起、
 * 次数写进紧跟着它的报错卡（那张卡还没画出来 ⇒ 下一份事实再写）。
 */
export function applyRetries(root: HTMLElement, outcomes: ReadonlyMap<string, RetryOutcome>): void {
  for (const bar of root.querySelectorAll<HTMLElement>(".card-api-retry[data-run]")) {
    const o = outcomes.get(bar.dataset.run ?? "");
    if (!o) continue;
    if (bar.dataset.state !== o) {
      bar.dataset.state = o;
      paintRetry(bar);
    }
    const card = bar.nextElementSibling;
    if (o !== "failed" || !isApiErrorCard(card)) continue;
    const text = card.querySelector<HTMLElement>(".api-error-next-text");
    const a = bar.dataset.attempt;
    const m = bar.dataset.max;
    if (text && a && m) text.textContent = copyText("apiError.card.next", { a, b: m });
  }
}

function paintRetry(line: HTMLElement): void {
  const reason = reasonWord(line.dataset.reason as ApiReason);
  const a = line.dataset.attempt ?? "";
  const b = line.dataset.max ?? "";
  if (line.dataset.state === "recovered") {
    line.textContent = copyText("apiError.retry.recovered", { reason, a: a || "?", time: line.dataset.last ?? "" });
    return;
  }
  if (line.dataset.state === "interrupted") {
    line.textContent = copyText("apiError.retry.interrupted", { reason, a: a || "?", time: line.dataset.last ?? "" });
    return;
  }
  const retry = a && b ? copyText("apiError.retry.count", { retryAttempt: a, maxRetries: b }) : "";
  line.textContent = copyText("apiError.retry.line", { reason, retry, time: line.dataset.since ?? "" });
}
