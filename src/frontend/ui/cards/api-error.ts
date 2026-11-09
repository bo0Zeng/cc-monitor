/**
 * 报错卡与重试细条。两种记录：
 *
 * 1. `assistant` + `isApiErrorMessage` —— 重试耗尽 / 不可重试的最终失败 ⇒ 报错卡：后端写好的卡头（`error.title`）＋ 下一步（会话事实那一串的整句，没有 ⇒ 空着）＋「原文」折叠。
 * 2. `system` + `subtype:"api_error"` —— 这一次失败、CLI 要重试 ⇒ 重试细条；一串只画首条（接在后面的那几条后端出 `draw: none`，不建卡）。
 *    细条上的字：建卡时照那一条记录写好的那一行（`retry.text`）；会话事实到了照那一串写好的整句（`retries[].text`，后端按首条的 uuid 给，[`applyRetries`]）。
 *    接上了 · 人打断了 ⇒ 变淡那一句 · 没接上 ⇒ 细条收起、那一串的整句进紧跟着的报错卡。
 *
 * 原因 · 次数 · 时刻怎么拼成一句全在后端；界面不认状态码、不读报错对象、不自己拼字。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import type { RetryRun } from "../generated/RetryRun";

/** 报错卡。`title` 是后端写好的卡头；`text` 是报错原文（进「原文」折叠，含状态码）。 */
export function buildApiErrorCard(args: { timeLabel: string; title: string; text: string; status?: number | null }): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-api-error";

  const head = document.createElement("div");
  head.className = "api-error-head";
  const label = document.createElement("span");
  label.className = "api-error-label";
  label.textContent = args.title;
  const ts = document.createElement("span");
  ts.className = "api-error-ts";
  ts.textContent = args.timeLabel;
  head.append(label, ts);

  const next = document.createElement("div");
  next.className = "api-error-next";
  // 「下一步」那一句只有会话事实那一路写（[`applyRetries`]：没接上的那一串的整句）；建卡时空着。
  const nextText = document.createElement("span");
  nextText.className = "api-error-next-text";
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

  // 去它的终端那两颗（拉前 · 在终端里打开，同右键那两项）：卡只摆出来，看不看得见由所在消息流根上那两个标记管
  // （宿主照脸上那两格设，`data-can-front` · `data-can-attach`）。
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
 * 重试细条（一条 = 相邻的一串重试）。状态住 `data-*`：结局（`data-state`，会话事实给）· 首条的 id（`data-run`，按它读那一串）。
 * `text` ＝ 那一条记录写好的那一行；`run` ＝ 建卡时会话事实已经说了的那一串（有就照它的整句画）。
 */
export function buildApiRetryCard(args: { text: string; id?: string; run?: RetryRun }): HTMLElement {
  const line = document.createElement("div");
  line.className = "card card-api-retry one-line";
  line.dataset.state = "retrying";
  if (args.id) line.dataset.run = args.id;
  line.textContent = args.text;
  if (args.run) paintRun(line, args.run);
  return line;
}

export function isApiErrorCard(el: Element | null | undefined): el is HTMLElement {
  return el instanceof HTMLElement && el.classList.contains("card-api-error");
}

/**
 * 会话事实说的那几串（首条 id → 那一串）照画到 `root` 里的每条细条上：结局进 `data-state`、整句照摆。幂等：每到一份事实重摆一遍。
 * 没接上 ⇒ 细条收起、那一串的整句写进紧跟着它的报错卡（事实先到、卡后画 ⇒ 宿主在卡落进消息流那一下照手上那一份再摆一遍）。
 */
export function applyRetries(root: HTMLElement, runs: ReadonlyMap<string, RetryRun>): void {
  for (const bar of root.querySelectorAll<HTMLElement>(".card-api-retry[data-run]")) {
    const run = runs.get(bar.dataset.run ?? "");
    if (!run) continue;
    paintRun(bar, run);
    const card = bar.nextElementSibling;
    if (run.outcome !== "failed" || !isApiErrorCard(card)) continue;
    const text = card.querySelector<HTMLElement>(".api-error-next-text");
    if (text) text.textContent = run.text;
  }
}

function paintRun(line: HTMLElement, run: RetryRun): void {
  line.dataset.state = run.outcome;
  if (line.textContent !== run.text) line.textContent = run.text;
}
