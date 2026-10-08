/**
 * 消息流里不是人说的那几类（「谁说的」稿方案 A · 事件条）：agent 交回 / 来话 · 另一个会话的来话 · 主会话后来的话 ·
 * 后台任务通知（相邻的并成一条）· 中断标记。来源只看后端给的 `userText.speaker`，正文是后端补的 `speaker.body`；界面不看正文。
 *
 * - 交回默认展开（是要读的）；途中来话默认收起；另一个会话的来话左条琥珀。
 * - 「打开窗口 ›」：点了把那个 agent 交给宿主（`ccm:reveal-run` 事件，带运行 id）。
 * - 后台通知：一条一行；相邻的由管线并成一条 `后台任务 ×3 · 14:02–14:07 · 失败 1`，展开看逐条（[`mergeNotice`]）。
 *   同一个子运行已经交回了（会话事实 `handedBack`，那台配的对）⇒ 它那条收场通知不画（[`applyHandedBack`]），只报交回那一次。
 */
import type { Speaker } from "../generated/Speaker";
import { copyText } from "../copy-table";
import { renderMarkdown } from "../render";

type Of<K extends Speaker["kind"]> = Extract<Speaker, { kind: K }>;

/** 点「打开窗口 ›」时发的事件名（宿主接住：把那个 agent 摆到眼前）。 */
export const REVEAL_RUN_EVENT = "ccm:reveal-run";

function bar(kind: string, title: string, tag: string, time: string, body: string | undefined, open: boolean, link?: { text: string; run: string }): HTMLElement {
  const d = document.createElement("details");
  d.className = "card card-speaker";
  d.dataset.kind = kind;
  d.open = open && !!body;
  const s = document.createElement("summary");
  s.className = "speaker-head";
  const t = document.createElement("span");
  t.className = "speaker-title";
  t.textContent = title;
  const g = document.createElement("span");
  g.className = "speaker-tag";
  g.textContent = tag;
  const ts = document.createElement("span");
  ts.className = "speaker-ts";
  ts.textContent = time;
  s.append(t, g, ts);
  if (link) {
    const a = document.createElement("button");
    a.type = "button";
    a.className = "speaker-link";
    a.textContent = link.text;
    a.addEventListener("click", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      a.dispatchEvent(new CustomEvent(REVEAL_RUN_EVENT, { bubbles: true, detail: { run: link.run } }));
    });
    s.appendChild(a);
  }
  d.appendChild(s);
  if (body) {
    const b = document.createElement("div");
    b.className = "speaker-body block-body-md";
    b.innerHTML = renderMarkdown(body);
    d.appendChild(b);
  }
  return d;
}

/** agent 交回 / 途中来话。`label` ＝ 运行表给的标签（查不到 ⇒ 来话自带的名字 ⇒「agent」）。 */
export function buildAgentBar(sp: Of<"agentMessage">, time: string, label: string | undefined): HTMLElement {
  const name = label || sp.name || copyText("speaker.agent.unnamed");
  return bar(
    "agent",
    copyText("speaker.agent.title", { label: name }),
    sp.handback ? copyText("speaker.tag.handback") : copyText("speaker.tag.message"),
    time,
    sp.body,
    sp.handback,
    sp.from ? { text: copyText("speaker.link.openRun"), run: sp.from } : undefined,
  );
}

/** 另一个会话发来的话（左条琥珀）。 */
export function buildPeerBar(sp: Of<"peerSession">, time: string): HTMLElement {
  return bar("peer", copyText("speaker.peer.title", { from: sp.from ?? "?" }), copyText("speaker.tag.message"), time, sp.body, true);
}

/** （agent 那一侧）主会话后来发给它的话：默认收起。 */
export function buildCoordinatorBar(sp: Of<"coordinator">, time: string): HTMLElement {
  return bar("coordinator", copyText("speaker.coordinator.title"), copyText("speaker.tag.message"), time, sp.body, false);
}

export function buildInjectedLine(body: string, time: string): HTMLDetailsElement {
  const d = document.createElement("details");
  d.className = "card card-injected";
  const s = document.createElement("summary");
  s.className = "injected-head";
  s.textContent = copyText("speaker.system.title", { time });
  const b = document.createElement("pre");
  b.className = "injected-body";
  b.textContent = body;
  d.append(s, b);
  return d;
}

/** 中断标记：一行细线。 */
export function buildInterruptLine(time: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "card card-event-line";
  el.dataset.kind = "interrupt";
  el.textContent = copyText("speaker.interrupt.line", { time });
  return el;
}

/** 后台任务通知：一条（相邻的由管线并进前一条）。`at` ＝ 记录的时刻（只用来排先后），`time` ＝ 它的钟面（后端写好的 `timeText`）。 */
export function buildNoticeLine(sp: Of<"taskNotification">, at: string, time: string): HTMLElement {
  const d = document.createElement("details");
  d.className = "card card-notice";
  d.dataset.from = at;
  d.dataset.to = at;
  const s = document.createElement("summary");
  s.className = "notice-head";
  d.appendChild(s);
  const list = document.createElement("div");
  list.className = "notice-list";
  d.appendChild(list);
  list.appendChild(noticeRow(sp, at, time));
  paintNotice(d);
  return d;
}

export function isNoticeLine(el: Element | null | undefined): el is HTMLDetailsElement {
  return el instanceof HTMLDetailsElement && el.classList.contains("card-notice");
}

/** 又一条通知紧挨着来了：并进前一条（时段取两头，逐条留在展开里）。 */
export function mergeNotice(into: HTMLDetailsElement, from: HTMLDetailsElement): void {
  const rows = from.querySelectorAll(".notice-row");
  const list = into.querySelector(".notice-list");
  rows.forEach((r) => list?.appendChild(r));
  if ((from.dataset.from ?? "") < (into.dataset.from ?? "")) into.dataset.from = from.dataset.from;
  if ((from.dataset.to ?? "") > (into.dataset.to ?? "")) into.dataset.to = from.dataset.to;
  paintNotice(into);
}

function noticeRow(sp: Of<"taskNotification">, at: string, time: string): HTMLElement {
  const r = document.createElement("div");
  r.className = "notice-row";
  const failed = sp.status === "failed" || sp.status === "killed";
  r.dataset.failed = String(failed);
  if (sp.taskId) r.dataset.task = sp.taskId;
  r.dataset.at = at;
  r.dataset.time = time;
  r.textContent = copyText("speaker.notice.row", {
    what: sp.summary ?? sp.taskId ?? "?",
    state: failed ? copyText("speaker.notice.failed") : copyText("speaker.notice.done"),
    time,
  });
  return r;
}

/**
 * 会话事实说这几个子运行已经交回了 ⇒ 它们的收场通知行收起（整条都是 ⇒ 整条收起），计数与时段只算还露着的。幂等：
 * 每到一份事实就按那一份重摆一遍（交回在通知之后才到也照样收）。
 */
export function applyHandedBack(root: HTMLElement, handedBack: ReadonlySet<string>): void {
  for (const d of root.querySelectorAll<HTMLDetailsElement>("details.card-notice")) {
    let changed = false;
    for (const r of d.querySelectorAll<HTMLElement>(".notice-row")) {
      const gone = r.dataset.task !== undefined && handedBack.has(r.dataset.task);
      if (r.hidden !== gone) {
        r.hidden = gone;
        changed = true;
      }
    }
    if (changed) paintNotice(d);
  }
}

function paintNotice(d: HTMLDetailsElement): void {
  const rows = [...d.querySelectorAll<HTMLElement>(".notice-row")].filter((r) => !r.hidden);
  const head = d.querySelector<HTMLElement>(".notice-head");
  if (!head) return;
  d.hidden = rows.length === 0;
  if (rows.length === 0) return;
  const fails = rows.filter((r) => r.dataset.failed === "true").length;
  d.dataset.failed = String(fails > 0);
  if (rows.length === 1) {
    head.textContent = rows[0].textContent;
    d.dataset.single = "true";
    return;
  }
  delete d.dataset.single;
  // 时段取露着的那几行的两头（按时刻排先后，写的是各自的钟面）。
  const byAt = [...rows].sort((a, b) => ((a.dataset.at ?? "") < (b.dataset.at ?? "") ? -1 : (a.dataset.at ?? "") > (b.dataset.at ?? "") ? 1 : 0));
  const from = byAt[0].dataset.time ?? "";
  const to = byAt[byAt.length - 1].dataset.time ?? "";
  head.textContent = copyText("speaker.notice.many", {
    n: rows.length,
    span: from === to ? from : `${from}–${to}`,
    fail: fails > 0 ? copyText("speaker.notice.fails", { n: fails }) : "",
  });
}
