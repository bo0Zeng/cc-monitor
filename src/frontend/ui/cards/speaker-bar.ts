/**
 * 消息流里不是人说的那几类（事件条）：agent 交回 / 来话 · 另一个会话的来话 · 主会话后来的话 ·
 * 后台任务通知 · 中断标记。来源只看后端给的 `userText.speaker`，正文是后端补的 `speaker.body`；界面不看正文。
 *
 * - 交回默认展开（是要读的）；途中来话默认收起；另一个会话的来话左条琥珀。
 * - 「打开窗口 ›」：点了把那个 agent 交给宿主（`ccm:reveal-run` 事件，带运行 id）。
 * - 后台通知：一条一张，那一行是后端写好的 `row`；相邻的哪几条并一组、哪几条交回过不露、组头那一句，全照会话事实
 *   `notices` 那一格摆（[`applyNotices`]），界面不数、不判。
 */
import type { Speaker } from "../generated/Speaker";
import type { NoticeRun } from "../generated/NoticeRun";
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

/** 后台任务通知：一条一张，那一行就是后端写好的 `row`。跟谁并成一组、哪几条不露，等会话事实那一格（[`applyNotices`]）。 */
export function buildNoticeLine(sp: Of<"taskNotification">): HTMLElement {
  const d = document.createElement("details");
  d.className = "card card-notice";
  const s = document.createElement("summary");
  s.className = "notice-head";
  const list = document.createElement("div");
  list.className = "notice-list";
  const r = document.createElement("div");
  r.className = "notice-row";
  r.dataset.failed = String(sp.end === "failed");
  r.textContent = sp.row ?? "";
  list.appendChild(r);
  s.textContent = r.textContent;
  d.append(s, list);
  d.dataset.single = "true";
  d.dataset.failed = r.dataset.failed;
  return d;
}

/**
 * 照会话事实的通知组摆（幂等，每到一份事实重摆一遍）：每组首条那一张当这一组 —— 别的成员那几行收进它的展开里（它们自己那一张收起）；
 * `hidden` 里的那几行收起，整组都不露 ⇒ 整张收起；组头就是后端写好的 `head`，没有 ⇒ 露着的那一行；`failed` 照挂。
 */
export function applyNotices(root: HTMLElement, runs: readonly NoticeRun[]): void {
  const cards = new Map<string, HTMLDetailsElement>();
  const rows = new Map<string, HTMLElement>();
  for (const d of root.querySelectorAll<HTMLDetailsElement>("details.card-notice")) {
    if (d.dataset.id !== undefined) cards.set(d.dataset.id, d);
  }
  for (const r of root.querySelectorAll<HTMLElement>(".notice-row[data-member]")) rows.set(r.dataset.member ?? "", r);
  const rowOf = (id: string): HTMLElement | undefined => {
    const tagged = rows.get(id);
    if (tagged) return tagged;
    const own = cards.get(id)?.querySelector<HTMLElement>(".notice-row");
    if (own) own.dataset.member = id;
    return own ?? undefined;
  };
  for (const run of runs) {
    const lead = cards.get(run.id);
    const list = lead?.querySelector(".notice-list");
    const head = lead?.querySelector<HTMLElement>(".notice-head");
    if (!lead || !list || !head) continue;
    for (const id of run.members) {
      const row = rowOf(id);
      if (!row) continue;
      const home = row.closest<HTMLElement>("details.card-notice");
      if (home !== lead) {
        list.appendChild(row);
        if (home) home.hidden = true;
      }
      row.hidden = run.hidden.includes(id);
    }
    const shown = [...list.querySelectorAll<HTMLElement>(".notice-row")].filter((r) => !r.hidden);
    lead.hidden = shown.length === 0;
    head.textContent = run.head ?? shown[0]?.textContent ?? "";
    if (run.head === null) lead.dataset.single = "true";
    else delete lead.dataset.single;
    lead.dataset.failed = String(run.failed);
  }
}
