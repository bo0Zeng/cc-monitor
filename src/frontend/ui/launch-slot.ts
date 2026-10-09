/**
 * **起新会话之后的占位标签页**（主窗口里起的那几条：命令面板 · 历史 / 标签页「在此目录新建」· 消息流「从这里分叉」）。
 *
 * 那台回了「起好了 / 开窗」⇒ 标签页栏末尾长出一行「正在启动」（转圈、机器 ＋ 项目名），主区换成它那一页；
 * 那台报出这个会话 ⇒ 这一行摘掉、换成真的标签页（还看着它 ⇒ 切过去）。20 s 没报到 ⇒ 红点「未报到」，那一页是一张报错卡：
 * 在 tmux 里起的 ⇒ 那个 tmux 会话在不在 ＋ 画面末几行 ＋［终端画面］［在终端里打开］（远端）［结束 tmux 会话］·［关闭标签页］；
 * 开窗起的 ⇒ 只有一句 ＋［关闭标签页］。到点不放弃：之后又报到了照样换成真的。
 *
 * 是不是那一个由 `launch-arrival.ts` 认（同一份比对）；tmux 会话在不在、画面是什么问那台（`terminal-reads.ts`）。这里只排版。
 */
import { copyText } from "./copy-table";
import { machineName } from "./control-said";
import { isRemoteOrigin, type Origin } from "./ipc/origin";
import { button } from "./kit/button";
import { tag } from "./kit/badge";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { statusDot } from "./kit/status-dot";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { toast, failToast } from "./kit/toast";
import { lastWords, watchUntilArrived, type SlotSpec } from "./launch-arrival";
import { listTerminals, previewText } from "./terminal-reads";
import { openPanePreview } from "./views/pane-preview";
import { runRemoteAttach } from "./remote-launch-run";
import { killSession } from "./tmux-control";
import s from "./launch-slot.module.css";

/** 多久没报到算「未报到」（到点只换样子，不放弃等）。 */
export const SLOT_MISS_MS = 20_000;

export type { SlotSpec };

/** 没报到时那个 tmux 会话的样子：在（画面是这几行）· 不在 · 读不到（那一句）。 */
export type SlotScreen = { kind: "there"; terminal: string; words: string } | { kind: "gone" } | { kind: "unread"; why: string };

/** 问那台的两件事与动手的三件（缺省走通道；测试换成假的）。 */
export interface SlotActs {
  screen(origin: Origin, tmuxName: string): Promise<SlotScreen>;
  preview(origin: Origin, tmuxName: string, terminal: string): void;
  attach(origin: Origin, agent: string, tmuxName: string): void;
  kill(origin: Origin, tmuxName: string): Promise<void>;
  confirm: ConfirmFn;
}

export interface SlotHost {
  /** 栏里放这几行的容器（标签页栏视图把它摆在列表末尾）。 */
  tail: HTMLElement;
  /** 主区（那一页盖在上面）。 */
  root: HTMLElement;
  /** 报到了：这个会话的标签页在了没有 · 切过去。 */
  hasTab(sid: string): boolean;
  switchTo(sid: string): void;
  /** 那一页显没显（会话头跟着让开）。 */
  shown(on: boolean): void;
}

interface Slot {
  id: number;
  spec: SlotSpec;
  missed: boolean;
  screen: SlotScreen | null;
  row: HTMLButtonElement;
  stop: () => void;
  timer: ReturnType<typeof setTimeout>;
}

const projOf = (cwd: string): string => cwd.split("/").filter(Boolean).pop() ?? cwd;

export class LaunchSlots {
  private readonly slots = new Map<number, Slot>();
  private next = 1;
  /** 正看着的那一个（`null` ＝ 主区是真的标签页）。 */
  private showing: number | null = null;
  readonly panel: HTMLElement;

  constructor(
    private readonly host: SlotHost,
    private readonly acts: SlotActs,
  ) {
    this.panel = document.createElement("div");
    this.panel.className = s.slotPanel;
    this.panel.hidden = true;
    host.root.appendChild(this.panel);
    host.tail.className = s.slotTail;
  }

  /** 那台回了「起好了 / 开窗」：长出一行、主区换成它。回它的编号。 */
  add(spec: SlotSpec): number {
    const id = this.next++;
    const row = document.createElement("button");
    row.type = "button";
    row.className = "tab";
    row.dataset.slot = String(id);
    row.addEventListener("click", (e) => {
      e.stopPropagation();
      this.show(id);
    });
    const slot: Slot = {
      id,
      spec,
      missed: false,
      screen: null,
      row,
      stop: watchUntilArrived(spec.origin, spec.match, (sid) => this.arrived(id, sid)),
      // 一次性：到点只把样子换成「未报到」（不放弃等、不重试、不轮询）。
      // 调度：一次性 —— 占位标签页 20s 的点：到点换成「未报到」、问一次；报到了 / 关掉时清
      timer: setTimeout(() => void this.miss(id), SLOT_MISS_MS),
    };
    this.slots.set(id, slot);
    this.host.tail.appendChild(row);
    this.paintRow(slot);
    this.show(id);
    return id;
  }

  /** 切到真的标签页之前：那一页收起（标签页栏 · 快捷键 · 命令面板切换都经这里）。 */
  hide(): void {
    if (this.showing === null) return;
    this.showing = null;
    this.panel.hidden = true;
    this.panel.replaceChildren();
    for (const sl of this.slots.values()) sl.row.classList.remove("active");
    this.host.shown(false);
  }

  /** 关掉这一行（不等了）。 */
  close(id: number): void {
    const sl = this.slots.get(id);
    if (!sl) return;
    clearTimeout(sl.timer);
    sl.stop();
    sl.row.remove();
    this.slots.delete(id);
    if (this.showing === id) this.hide();
  }

  /** 正看着某一个占位标签页。 */
  isShowing(): boolean {
    return this.showing !== null;
  }

  /** 只给判据用：此刻有几行 · 正看着哪一个。 */
  debug(): { ids: number[]; showing: number | null } {
    return { ids: [...this.slots.keys()], showing: this.showing };
  }

  private show(id: number): void {
    const sl = this.slots.get(id);
    if (!sl) return;
    this.showing = id;
    for (const o of this.slots.values()) o.row.classList.toggle("active", o.id === id);
    this.paintPanel(sl);
    this.panel.hidden = false;
    this.host.shown(true);
  }

  private arrived(id: number, sid: string): void {
    const sl = this.slots.get(id);
    if (!sl) return;
    const wasShowing = this.showing === id;
    sl.spec.onArrive?.(sid);
    clearTimeout(sl.timer);
    sl.row.remove();
    this.slots.delete(id);
    if (!wasShowing) return;
    this.hide();
    // 远端那一路先报到、后建标签页 ⇒ 等它在了再切（同一拍之内）。
    const go = (): void => {
      if (this.host.hasTab(sid)) this.host.switchTo(sid);
    };
    if (this.host.hasTab(sid)) go();
    else queueMicrotask(go);
  }

  private async miss(id: number): Promise<void> {
    const sl = this.slots.get(id);
    if (!sl) return;
    sl.missed = true;
    if (sl.spec.tmuxName !== null) {
      try {
        sl.screen = await this.acts.screen(sl.spec.origin, sl.spec.tmuxName);
      } catch (e) {
        sl.screen = { kind: "unread", why: e instanceof Error ? e.message : String(e) };
      }
    }
    if (!this.slots.has(id)) return;
    this.paintRow(sl);
    if (this.showing === id) this.paintPanel(sl);
  }

  private paintRow(sl: Slot): void {
    const { origin, cwd } = sl.spec;
    const dot = sl.missed ? statusDot("failed", copyText("launch.slot.missedTitle"), "compact") : spinner();
    dot.classList.add("tab-dot");
    const title = document.createElement("span");
    title.className = "tab-title";
    const proj = document.createElement("span");
    proj.className = "tab-proj";
    proj.textContent = projOf(cwd);
    const label = document.createElement("span");
    label.className = "tab-label";
    label.textContent = sl.missed ? copyText("launch.slot.missedTitle") : copyText("launch.placeholder.title");
    title.append(proj, label);
    const parts: HTMLElement[] = [dot];
    if (isRemoteOrigin(origin)) {
      const m = tag(machineName(origin));
      m.classList.add("tab-machine");
      parts.push(m);
    }
    parts.push(title);
    sl.row.replaceChildren(...parts);
    sl.row.dataset.state = sl.missed ? "missed" : "starting";
  }

  private paintPanel(sl: Slot): void {
    const { origin, cwd, tmuxName } = sl.spec;
    const machine = machineName(origin);
    const secs = String(Math.round(SLOT_MISS_MS / 1000));

    const head = document.createElement("div");
    head.className = s.slotHead;
    const dot = sl.missed ? statusDot("failed", copyText("launch.slot.missedTitle"), "compact") : spinner();
    const title = document.createElement("span");
    title.className = s.slotTitle;
    title.textContent = sl.missed ? copyText("launch.slot.missedTitle") : copyText("launch.placeholder.title");
    const where = document.createElement("span");
    where.className = s.slotWhere;
    where.textContent = `${machine}${copyText("kit.text.sep")}${cwd}`;
    head.append(dot, title, where);
    if (sl.missed) {
      const st = document.createElement("span");
      st.className = s.slotState;
      st.textContent = copyText("launch.slot.missedState", { secs });
      head.appendChild(st);
    }
    if (!sl.missed) {
      this.panel.replaceChildren(head);
      return;
    }

    const card = document.createElement("section");
    card.className = s.slotCard;
    card.setAttribute("role", "alert");
    const body = document.createElement("div");
    body.className = s.slotBody;
    const line1 = document.createElement("div");
    line1.textContent = copyText("launch.slot.missedCard", { secs });
    body.appendChild(line1);

    const acts: HTMLElement[] = [];
    const sc = sl.screen;
    if (tmuxName !== null && sc !== null) {
      const line2 = document.createElement("div");
      line2.className = s.slotSub;
      line2.textContent =
        sc.kind === "gone"
          ? copyText("launch.slot.tmuxGone", { machine, name: tmuxName })
          : sc.kind === "unread"
            ? copyText("launch.slot.noScreen", { machine, name: tmuxName, why: sc.why })
            : sc.words === ""
              ? copyText("launch.slot.tmuxEmpty", { machine, name: tmuxName })
              : copyText("launch.slot.tmuxThere", { machine, name: tmuxName });
      body.appendChild(line2);
      if (sc.kind === "there" && sc.words !== "") {
        const out = document.createElement("pre");
        out.className = s.slotOut;
        out.textContent = sc.words;
        body.appendChild(out);
      }
      if (sc.kind === "there") {
        const terminal = sc.terminal;
        const shot = button({ label: copyText("launch.slot.screen"), size: "compact", icon: "search", onClick: () => this.acts.preview(origin, tmuxName, terminal) });
        shot.dataset.act = "screen";
        acts.push(shot);
        if (isRemoteOrigin(origin)) {
          const att = button({ label: copyText("sessionHead.act.openTerm"), size: "compact", onClick: () => this.acts.attach(origin, sl.spec.agent, tmuxName) });
          att.dataset.act = "attach";
          acts.push(att);
        }
        const kill = button({ label: copyText("sessionState.killIdle.action"), size: "compact", onClick: () => void this.killIn(sl.id) });
        kill.dataset.act = "kill";
        acts.push(kill);
      }
    }
    const sp = document.createElement("span");
    sp.className = s.slotSp;
    const close = button({ label: copyText("launch.slot.close"), kind: "ghost", size: "compact", onClick: () => this.close(sl.id) });
    close.dataset.act = "close";
    const row = document.createElement("div");
    row.className = s.slotActs;
    row.append(...acts, sp, close);
    body.appendChild(row);
    card.append(icon("error"), body);
    this.panel.replaceChildren(head, card);
  }

  private async killIn(id: number): Promise<void> {
    const sl = this.slots.get(id);
    const name = sl?.spec.tmuxName;
    if (!sl || name === null || name === undefined) return;
    const machine = machineName(sl.spec.origin);
    const ok = await this.acts.confirm({
      title: copyText("sessionState.killIdle.title", { name }),
      action: copyText("sessionState.killIdle.action"),
      danger: true,
      rows: [{ label: copyText("kit.interrupts.cut"), items: [copyText("launch.slot.killCuts", { machine, name })] }],
    });
    if (!ok) return;
    try {
      await this.acts.kill(sl.spec.origin, name);
    } catch (e) {
      failToast(copyText("tabSessionActions.kill.failed", { title: name }), e);
      return;
    }
    toast(copyText("sessionState.kill.done", { title: name }), "", { level: "success" });
    this.close(id);
  }
}

/** 缺省的那几件：走通道。 */
export const CHANNEL_ACTS: SlotActs = {
  async screen(origin, tmuxName) {
    const row = (await listTerminals(origin, tmuxName)).find((r) => r.tmuxName === tmuxName);
    if (row === undefined) return { kind: "gone" };
    return { kind: "there", terminal: row.terminal, words: lastWords(await previewText(origin, { terminal: row.terminal }, tmuxName)) };
  },
  preview: (origin, tmuxName, terminal) => void openPanePreview(origin, tmuxName, { terminal }),
  attach: (origin, agent, tmuxName) => void runRemoteAttach(origin, agent, tmuxName),
  kill: async (origin, tmuxName) => void (await killSession(origin, tmuxName)),
  confirm: confirmDialog,
};
