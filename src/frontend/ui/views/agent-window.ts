/**
 * **agent 窗口**：一个子运行自己的独立窗口（`viewer.html?viewer=<sid>&run=<运行>&origin=<机器>` 加载，入口 `entry-viewer.ts`）。
 *
 * 外壳与只读查看窗同一套：细顶栏（路径 `{会话} › {派出它的那几个} › {它}`，每段可点 ｜［回到派出它的地方］）＋ 消息流 ＋ 细状态栏
 * （`只读 · 实时` ｜ `会话 {会话}`）。消息流里从上往下：标题区（类别 · 标签 · 状态标记 · 几个时刻）· 说明（一态一句，在跑且没在等时不出）·
 * 它派出的 agent（一排小片，点了开那个的窗口）· 它的记录（与主会话同一套卡；派活的那段话画成带抬头的框）· 收场后尾巴上一条结束线。
 *
 * 数据：状态 · 时刻 · 原因 · 谁派的 —— 都读后端运行表（会话流里的 `runs`，`followSession` 交来的），界面不判；
 * 记录按运行读（`run-timeline.ts` → `history-run`），运行表里它一变就从上次读到的地方续读。
 * 滚动：在跑的进来停在最底、跟着长；收场的进来停在最上；往上翻了不拽人，底下出「↓ 新内容」。
 * 时长只写到分钟，每分钟按已有的运行表重画一次（[`CLOCK_MS`]，不取数）。
 * 系统标题 `{标签} · {状态} · {会话}`，状态变了跟着改。`Esc` 不关窗。窗口开着 / 关了广播给主窗口（`window-events.ts`）。
 */
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { copyText } from "../copy-table";
import { renderMessage, type JsonlRecord, type RenderContext } from "../cards";
import { markRunCard, markRunWindow } from "../cards/subagent";
import { REVEAL_RUN_EVENT } from "../cards/speaker-bar";
import { speakerNameOf } from "../agent-profile";
import { followSession, type FollowEvent } from "../events";
import { fetchList, type HistoryRow } from "../history-list-reads";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import { button } from "../kit/button";
import { banner } from "../kit/banner";
import { detailOf } from "../kit/detail";
import { icon } from "../kit/icon";
import { tag } from "../kit/badge";
import { MessageStream } from "../stream";
import { RunTimeline } from "../run-timeline";
import { runLabel, runStateIcon, runStateText } from "../runs";
import type { RunInfo } from "../generated/RunInfo";
import { agentWindowTitle, chainOf, endOf, factsOf, kidsOf, whyOf } from "../agent-window-text";
import { openAgentWindow } from "../agent-window-open";
import {
  AGENT_WINDOW_EVENT,
  AGENT_WINDOWS_ASK_EVENT,
  SHOW_RUN_CARD_EVENT,
  SWITCH_TO_SESSION_EVENT,
  type AgentWindowSaid,
  type ShowRunCard,
} from "../window-events";
import sv from "./session-viewer.module.css";
import s from "./agent-window.module.css";

/** 重画「已跑 · 最近 · 等了多久」的节拍：时长只写到分钟。 */
export const CLOCK_MS = 60_000;

export interface AgentWindowParts {
  /** 细顶栏（`#app` 网格的 top 那一格）。 */
  topbar: HTMLElement;
  /** 消息流那一格。 */
  main: HTMLElement;
  /** 细状态栏。 */
  foot: HTMLElement;
}

export class AgentWindow {
  private readonly stream: MessageStream;
  private readonly scrollEl: HTMLElement;
  private readonly head: HTMLElement;
  private readonly why: HTMLElement;
  private readonly kids: HTMLElement;
  private readonly end: HTMLElement;
  /** 记录那一段（结束线跟在它后面）。 */
  private readonly wrap: HTMLElement;
  private readonly pill: HTMLButtonElement;
  private readonly crumbs: HTMLElement;
  private readonly status: HTMLElement;
  private readonly bannerEl: HTMLElement;
  private timeline: RunTimeline | null = null;
  private row: HistoryRow | null = null;
  private runs: RunInfo[] = [];
  private info: RunInfo | null = null;
  private live = false;
  private following = false;
  /** 第一页读到、运行表也到了才定一次滚动（在跑 ⇒ 底，收场 ⇒ 顶）。 */
  private placed = false;
  private loaded = false;
  /** 它派出的那几张卡（父侧工具调用 id ⇒ 卡）。 */
  private readonly runCards = new Map<string, HTMLElement>();
  /** 要滚到的派出卡（卡还没读到时先记着）。 */
  private pendingCard: string | null = null;
  private readonly ctx: RenderContext;
  /** 这个会话里开着窗口的那几个子运行（各窗口开 / 关时广播的）。 */
  private readonly windows = new Set<string>();

  constructor(
    private readonly sid: string,
    private readonly run: string,
    private readonly origin: Origin,
    private readonly parts: AgentWindowParts,
  ) {
    this.crumbs = document.createElement("div");
    this.crumbs.className = "viewer-topbar-title";
    const acts = document.createElement("div");
    acts.className = "viewer-topbar-acts";
    acts.appendChild(
      button({
        label: copyText("agentWindow.back.label"),
        size: "compact",
        hint: copyText("agentWindow.back.hint"),
        onClick: () => void this.back(),
      }),
    );
    parts.topbar.className = "viewer-topbar";
    parts.topbar.replaceChildren(this.crumbs, acts);

    const view = document.createElement("div");
    view.className = s.awView;
    this.bannerEl = document.createElement("div");
    this.scrollEl = document.createElement("div");
    this.scrollEl.className = "stream session-viewer-stream";
    view.append(this.bannerEl, this.scrollEl);
    this.stream = new MessageStream(this.scrollEl);
    this.head = document.createElement("div");
    this.head.className = s.awHead;
    this.why = document.createElement("div");
    this.kids = document.createElement("div");
    this.end = document.createElement("div");
    this.end.className = s.awEnd;
    this.wrap = document.createElement("div");
    this.wrap.className = s.awTimeline;
    this.ctx = {
      parentPath: "",
      speaker: null,
      origin,
      briefFrom: null,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      runCards: this.runCards,
    };
    this.stream.contentElement.append(this.head, this.why, this.kids);

    const pillRow = document.createElement("div");
    pillRow.className = sv.svPillRow;
    this.pill = button({
      label: copyText("sessionViewer.stream.newContent"),
      icon: "arrowDown",
      size: "compact",
      onClick: () => {
        this.stream.scrollToBottom();
        this.showPill(false);
      },
    });
    this.pill.classList.add(sv.svPill);
    this.pill.dataset.role = "new-content";
    this.showPill(false);
    pillRow.appendChild(this.pill);
    view.appendChild(pillRow);
    this.scrollEl.addEventListener("scroll", () => {
      if (this.stream.stuckToBottom) this.showPill(false);
    }, { passive: true });
    parts.main.replaceChildren(view);

    this.status = document.createElement("span");
    const session = document.createElement("span");
    session.dataset.role = "session";
    parts.foot.className = s.awFoot;
    parts.foot.replaceChildren(this.status, session);
    // 消息流里它派出的那几张卡：点卡头 ⇒ 开那个孙 agent 自己的窗口。
    document.addEventListener(REVEAL_RUN_EVENT, (e) => {
      const d = (e as CustomEvent<{ run?: unknown; tool?: unknown }>).detail;
      const r = this.runs.find((x) => (typeof d?.run === "string" ? x.run === d.run : x.tool === d?.tool));
      if (r) void this.open(r);
    });
  }

  /** 读这个会话的那一行、订它的流、读第一页。 */
  async start(): Promise<void> {
    try {
      const list = await fetchList(isLocalOrigin(this.origin) ? undefined : this.origin, { sid: this.sid });
      this.row = list.rows.find((r) => r.sessionId === this.sid) ?? null;
    } catch (e) {
      this.say(copyText("sessionViewer.load.failed", { why: String(e) }), detailOf(e));
      return;
    }
    const row = this.row;
    if (!row) {
      this.say(copyText("viewerWindow.record.gone"));
      return;
    }
    this.live = row.status === "live";
    this.ctx.parentPath = row.jsonlPath;
    this.ctx.speaker = speakerNameOf(row.agent);
    const foot = this.parts.foot.querySelector<HTMLElement>('[data-role="session"]');
    if (foot) foot.textContent = copyText("agentWindow.status.session", { session: this.sessionName() });
    this.timeline = new RunTimeline({
      origin: this.origin,
      parent: row.jsonlPath,
      which: { run: this.run },
      render: (rec: JsonlRecord) => renderMessage(rec, this.ctx),
    });
    this.wrap.appendChild(this.timeline.element);
    this.stream.contentElement.append(this.wrap);
    this.paint();
    await this.announce();
    const sub = await followSession(this.origin, this.sid, (e) => this.onFollow(e));
    this.following = true;
    // 时长都只写到分钟：每分钟按已有的运行表重画一次（不取数）。
    // 调度：钟 —— 每分钟重画标题区（时长只写到分钟），不取数；关窗即清
    const clock = window.setInterval(() => this.paint(), CLOCK_MS);
    window.addEventListener("pagehide", () => {
      sub.stop();
      window.clearInterval(clock);
    });
    this.paint();
    await this.pull();
  }

  private sessionName(): string {
    const r = this.row;
    if (!r) return "";
    return r.untitled ? copyText("history.row.untitled") : r.label;
  }

  /** 顶上那一条；`detail`（出错那一端写的复制详情）非空 ⇒ 带［复制详情］。 */
  private say(text: string, detail = ""): void {
    this.bannerEl.replaceChildren(banner("warn", text, [], detail));
  }

  /** 从上次读到的地方续读；读到了新东西而人不在底部 ⇒ 底下出「↓ 新内容」。 */
  private async pull(): Promise<void> {
    const t = this.timeline;
    if (!t) return;
    const before = t.body.childElementCount;
    await t.refresh();
    this.markKids();
    this.loaded = true;
    if (!this.placed) this.place();
    else if (t.body.childElementCount > before && !this.stream.stuckToBottom) this.showPill(true);
    if (this.pendingCard) this.showCard(this.pendingCard);
  }

  /** 「↓ 新内容」露不露（人不在底部、又读到了新东西）。 */
  private showPill(on: boolean): void {
    this.pill.hidden = !on;
  }

  /** 进来时停在哪：在跑的停在最底（跟着长），收场的停在最上（从派活的那段话读起）。 */
  private place(): void {
    if (!this.loaded || !this.info) return;
    this.placed = true;
    if (this.info.state !== "running") this.scrollEl.scrollTop = 0;
  }

  private onFollow(e: FollowEvent): void {
    if (e.t === "runs") {
      this.runs = e.payload.runs;
      const next = this.runs.find((r) => r.run === this.run) ?? null;
      const moved = next !== null && JSON.stringify(next) !== JSON.stringify(this.info);
      this.info = next;
      this.paint();
      if (!this.placed) this.place();
      if (moved) void this.pull();
      else this.markKids();
    } else if (e.t === "live") {
      this.live = e.live;
      this.paint();
    } else if (e.t === "sight") {
      this.following = e.seen;
      this.paint();
      if (e.seen) void this.pull();
    }
  }

  /** 它派出的那几张卡标上状态与「窗口已开」（运行表里 `tool` 对得上的）。 */
  private markKids(): void {
    for (const r of this.runs) {
      if (r.tool === undefined) continue;
      const card = this.runCards.get(r.tool);
      if (!card) continue;
      markRunCard(card, r.run, r.state, r);
      markRunWindow(card, this.windows.has(r.run));
    }
  }

  /** 照此刻的运行表把标题区 · 说明 · 派出的 · 结束线 · 路径 · 系统标题 · 状态栏重画一遍。 */
  paint(now = Date.now()): void {
    const r: RunInfo = this.info ?? { run: this.run, state: "unknown" };
    const kids = kidsOf(this.runs, this.run);
    const chain = chainOf(this.runs, r);
    const parent = chain.length > 0 ? chain[chain.length - 1] : null;
    this.ctx.briefFrom = parent ? runLabel(parent) : null;
    for (const who of this.stream.contentElement.querySelectorAll<HTMLElement>('[data-role="brief-from"]')) {
      who.textContent = parent ? copyText("agentWindow.brief.fromAgent", { parent: runLabel(parent) }) : copyText("agentWindow.brief.fromMain");
    }

    // 标题区
    const title = document.createElement("div");
    title.className = s.awTitle;
    if (r.kind) title.appendChild(tag(r.kind));
    const label = document.createElement("span");
    label.className = s.awLabel;
    label.textContent = runLabel(r);
    title.appendChild(label);
    const facts = document.createElement("div");
    facts.className = s.awFacts;
    const mark = tag(`${runStateIcon(r.state)} ${runStateText(r.state)}`);
    mark.dataset.state = r.state;
    mark.classList.add(s.awMark);
    facts.appendChild(mark);
    const line = document.createElement("span");
    line.textContent = factsOf(r, kids.length, now).join(copyText("kit.text.sep"));
    facts.appendChild(line);
    this.head.replaceChildren(title, facts);

    // 说明
    const w = whyOf(r, now);
    if (w === null) {
      this.why.replaceChildren();
      this.why.className = "";
    } else {
      this.why.className = s.awWhy;
      this.why.dataset.state = r.state;
      const top = document.createElement("div");
      top.className = s.awWhyLine;
      if (w.lead) {
        const b = document.createElement("strong");
        b.textContent = w.lead;
        top.appendChild(b);
      }
      const t = document.createElement("span");
      t.textContent = w.text;
      top.appendChild(t);
      if (w.note) {
        const n = document.createElement("span");
        n.className = s.awWhyNote;
        n.textContent = w.note;
        top.appendChild(n);
      }
      if (w.refresh) {
        const b = button({ label: copyText("agentWindow.why.refresh"), size: "compact", onClick: () => void this.pull() });
        b.classList.add(s.awWhyAct);
        top.appendChild(b);
      }
      this.why.replaceChildren(top);
      if (w.error) {
        const pre = document.createElement("pre");
        pre.className = s.awWhyError;
        pre.textContent = w.error;
        this.why.appendChild(pre);
      }
    }

    // 它派出的 agent
    if (kids.length === 0) {
      this.kids.replaceChildren();
      this.kids.className = "";
    } else {
      this.kids.className = s.awKids;
      const h = document.createElement("div");
      h.className = s.awKidsHead;
      h.textContent = copyText("agentWindow.kids.title");
      const row = document.createElement("div");
      row.className = s.awKidsRow;
      for (const k of kids) {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = s.awKid;
        chip.dataset.state = k.state;
        chip.dataset.run = k.run;
        chip.title = runLabel(k);
        const i = stateIcon(k);
        const t = document.createElement("span");
        t.className = s.awKidLabel;
        t.textContent = runLabel(k);
        chip.append(i, t);
        chip.addEventListener("click", () => void this.open(k));
        row.appendChild(chip);
      }
      this.kids.replaceChildren(h, row);
    }

    // 结束线
    const e = endOf(r, now);
    if (e === null || !this.info) {
      this.end.remove();
    } else {
      if (this.wrap.isConnected) this.wrap.after(this.end);
      this.end.dataset.state = r.state;
      const head = document.createElement("div");
      head.className = s.awEndHead;
      head.textContent = e.head;
      const sub = document.createElement("div");
      sub.className = s.awEndSub;
      sub.textContent = e.sub;
      this.end.replaceChildren(head, sub);
    }

    // 路径：会话 › 派出它的那几个 › 它
    const parts: Node[] = [];
    const session = document.createElement("button");
    session.type = "button";
    session.className = s.awCrumb;
    session.textContent = this.sessionName();
    session.addEventListener("click", () => void emit(SWITCH_TO_SESSION_EVENT, { sessionId: this.sid }).catch(() => {}));
    parts.push(session);
    for (const up of chain) {
      parts.push(icon("caretRight", "compact"));
      const b = document.createElement("button");
      b.type = "button";
      b.className = s.awCrumb;
      b.append(stateIcon(up), runLabel(up));
      b.addEventListener("click", () => void this.open(up));
      parts.push(b);
    }
    parts.push(icon("caretRight", "compact"));
    const me = document.createElement("span");
    me.className = "viewer-topbar-label";
    me.append(stateIcon(r), runLabel(r));
    parts.push(me);
    this.crumbs.replaceChildren(...parts);

    // 系统标题 · 状态栏
    if (this.row) void getCurrentWindow().setTitle(agentWindowTitle(r, this.sessionName(), this.row.origin ?? null)).catch(() => {});
    this.status.textContent = this.live && this.following ? copyText("agentWindow.status.live") : copyText("agentWindow.status.plain");
  }

  /** 开另一个子运行的窗口（在这扇窗右下错开；已开着 ⇒ 拉到前面）。 */
  private async open(r: RunInfo): Promise<void> {
    await openAgentWindow({ origin: this.origin, sid: this.sid, run: r, session: this.sessionName(), machine: this.row?.origin ?? null });
  }

  /** 回到派出它的地方：父 agent 的窗口（没开就开）或主窗口，滚到派出它的那张卡。 */
  private async back(): Promise<void> {
    const r = this.info;
    const chain = r ? chainOf(this.runs, r) : [];
    const parent = chain.length > 0 ? chain[chain.length - 1] : null;
    if (parent) await this.open(parent);
    if (r?.tool === undefined) {
      if (!parent) await emit(SWITCH_TO_SESSION_EVENT, { sessionId: this.sid }).catch(() => {});
      return;
    }
    const said: ShowRunCard = { sessionId: this.sid, tool: r.tool, in: parent ? parent.run : null };
    await emit(SHOW_RUN_CARD_EVENT, said).catch(() => {});
    if (parent) {
      // 父窗口是刚开的：它报到了再说一次（卡还没读到时它先记着，读到了再滚）。
      const un = await listen<AgentWindowSaid>(AGENT_WINDOW_EVENT, (ev) => {
        if (ev.payload.sessionId === this.sid && ev.payload.run === parent.run && ev.payload.open) {
          un();
          void emit(SHOW_RUN_CARD_EVENT, said).catch(() => {});
        }
      });
    }
  }

  /** 派出卡那张：滚过去、闪一下（还没读到 ⇒ 记着，读到了再滚）。 */
  showCard(tool: string): void {
    const card = this.runCards.get(tool);
    if (!card) {
      this.pendingCard = tool;
      return;
    }
    this.pendingCard = null;
    card.scrollIntoView({ block: "center" });
    card.classList.remove("search-hit-flash");
    void card.offsetWidth;
    card.classList.add("search-hit-flash");
  }

  /** 报到（开了）、接主窗口的「哪些开着」、接「滚到派出卡」；关窗前说一声关了。 */
  private async announce(): Promise<void> {
    const said = (open: boolean): Promise<void> => emit(AGENT_WINDOW_EVENT, { sessionId: this.sid, run: this.run, open } satisfies AgentWindowSaid).catch(() => {});
    await listen(AGENT_WINDOWS_ASK_EVENT, () => void said(true));
    await listen<AgentWindowSaid>(AGENT_WINDOW_EVENT, (ev) => {
      if (ev.payload.sessionId !== this.sid) return;
      if (ev.payload.open) this.windows.add(ev.payload.run);
      else this.windows.delete(ev.payload.run);
      this.markKids();
    });
    await listen<ShowRunCard>(SHOW_RUN_CARD_EVENT, (ev) => {
      if (ev.payload.sessionId !== this.sid || ev.payload.in !== this.run) return;
      const w = getCurrentWindow();
      void w.unminimize().then(() => w.setFocus()).catch(() => {});
      this.showCard(ev.payload.tool);
    });
    await getCurrentWindow().onCloseRequested(async () => {
      await said(false);
    });
    await said(true);
  }
}

/** 状态图标（在跑绿、失败红）。 */
function stateIcon(r: RunInfo): HTMLElement {
  const i = document.createElement("span");
  i.className = s.awStateIcon;
  i.dataset.state = r.state;
  i.textContent = runStateIcon(r.state);
  return i;
}

/** 入口：`viewer.html?viewer=<sid>&run=<运行>&origin=<机器>`。 */
export async function bootstrapAgentWindow(sid: string, run: string, origin: Origin): Promise<void> {
  document.body.classList.add("viewer-mode");
  const app = document.getElementById("app");
  const main = document.getElementById("message-stream");
  const foot = document.getElementById("status-bar");
  if (!app || !main || !foot) {
    console.error("agent window: layout containers missing");
    return;
  }
  const topbar = document.createElement("div");
  topbar.className = "viewer-topbar";
  app.insertBefore(topbar, app.firstChild);
  const w = new AgentWindow(sid, run, origin, { topbar, main, foot });
  await w.start();
}
