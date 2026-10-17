/**
 * **起新会话之后的占位标签页**（主窗口里起的那几条：命令面板 · 历史 / 标签页「在此目录新建」· 消息流「从这里分叉」）。
 *
 * 那台回了「起好了 / 开窗」⇒ 标签页栏末尾长出一行「正在启动」（转圈、机器 ＋ 项目名），主区换成它那一页；
 * 问那台等它报到（`session-arrive`，期限 [`SLOT_MISS_MS`]）：报到了 ⇒ 这一行摘掉、换成真的标签页（还看着它 ⇒ 切过去）；
 * 没见到 ⇒ 红点「未报到」，那一页是一张报错卡：那台写的那一句（起在 tmux 里的带画面末几行）＋ 那个终端还在 ⇒
 *［终端画面］（开抽屉的终端页看那个终端）［在终端里打开］（远端）［结束 tmux 会话］·［关闭标签页］。
 *
 * 认不认得出 · 那一句 · 那一屏都是那台的；这里只照回包排版。
 */
import { copyText } from "./copy-table";
import { machineName } from "./control-said";
import { isLocalOrigin, isRemoteOrigin, type Origin } from "./ipc/origin";
import { button } from "./kit/button";
import { tag } from "./kit/badge";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { statusDot } from "./kit/status-dot";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { toast, failToast } from "./kit/toast";
import { askArrive, openSession, type ArriveSaid, type SlotSpec } from "./session-open";
import type { ArriveAsk } from "./generated/ArriveAsk";
import { killSession } from "./tmux-control";
import s from "./launch-slot.module.css";

/** 等多久没报到算「未报到」（那台照它等，到了答那一句）。 */
export const SLOT_MISS_MS = 20_000;

export type { SlotSpec };

/** 问那台的一件事与动手的两件（缺省走通道；测试换成假的）。 */
export interface SlotActs {
  arrive(origin: Origin, ask: ArriveAsk, within: number): Promise<ArriveSaid>;
  attach(origin: Origin, tmuxName: string): void;
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
  /** ［终端画面］：开底部抽屉的终端页（它看的是 [`LaunchSlots.watched`]）。 */
  viewTerminal(): void;
}

/** 没报到时那台说的：那一句（第一行标题）· 起在的那个终端还在 ⇒ 它的句柄。 */
interface Missed {
  said: string;
  terminal: string | null;
}

interface Slot {
  id: number;
  spec: SlotSpec;
  missed: Missed | null;
  row: HTMLButtonElement;
}

const projOf = (cwd: string): string => cwd.split("/").filter(Boolean).pop() ?? cwd;

export class LaunchSlots {
  private readonly slots = new Map<number, Slot>();
  private next = 1;
  /** 正看着的那一个（`null` ＝ 主区是真的标签页）。 */
  private showing: number | null = null;
  /** 正看着的那一个报到了、它的标签页还没建（会话流那一帧晚于回话）⇒ 建了就切过去。 */
  private follow: string | null = null;
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
    const slot: Slot = { id, spec, missed: null, row };
    this.slots.set(id, slot);
    this.host.tail.appendChild(row);
    this.paintRow(slot);
    this.show(id);
    void this.acts.arrive(spec.origin, spec.arrival, SLOT_MISS_MS).then((end) => {
      if (end.kind === "arrived") this.arrived(id, end.sid);
      else this.miss(id, end.kind === "missed" ? end : { said: end.said, terminal: null });
    });
    return id;
  }

  /** 一个会话的标签页建好了：正等着切过去的就是它 ⇒ 切过去。 */
  tabAppeared(sid: string): void {
    if (this.follow !== sid) return;
    this.follow = null;
    this.host.switchTo(sid);
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
    sl.row.remove();
    this.slots.delete(id);
    if (this.showing === id) this.hide();
  }

  /** 正看着某一个占位标签页。 */
  isShowing(): boolean {
    return this.showing !== null;
  }

  /** 正看着的那个占位起在的那个终端（没报到、问到了它还在才有）：抽屉的终端页看它。 */
  watched(): { origin: Origin; terminal: string; name: string; title: string } | null {
    const sl = this.showing === null ? undefined : this.slots.get(this.showing);
    const terminal = sl?.missed?.terminal;
    if (!sl || terminal === null || terminal === undefined || sl.spec.tmuxName === null) return null;
    return { origin: sl.spec.origin, terminal, name: sl.spec.tmuxName, title: projOf(sl.spec.cwd) };
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
    sl.row.remove();
    this.slots.delete(id);
    if (!wasShowing) return;
    this.hide();
    if (this.host.hasTab(sid)) this.host.switchTo(sid);
    else this.follow = sid;
  }

  private miss(id: number, m: Missed): void {
    const sl = this.slots.get(id);
    if (!sl) return;
    sl.missed = m;
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
    const m = sl.missed;
    if (m === null) {
      this.panel.replaceChildren(head);
      return;
    }

    const card = document.createElement("section");
    card.className = s.slotCard;
    card.setAttribute("role", "alert");
    const body = document.createElement("div");
    body.className = s.slotBody;
    // 那台那一句：第一行标题 · 第二行说那个终端怎样 · 其余是画面末几行。
    const [line, sub, ...words] = m.said.split("\n");
    const line1 = document.createElement("div");
    line1.textContent = line;
    body.appendChild(line1);
    if (sub !== undefined) {
      const line2 = document.createElement("div");
      line2.className = s.slotSub;
      line2.textContent = sub;
      body.appendChild(line2);
    }
    if (words.length > 0) {
      const out = document.createElement("pre");
      out.className = s.slotOut;
      out.textContent = words.join("\n");
      body.appendChild(out);
    }

    const acts: HTMLElement[] = [];
    if (tmuxName !== null && m.terminal !== null) {
      const shot = button({ label: copyText("launch.slot.screen"), size: "compact", icon: "search", onClick: () => this.host.viewTerminal() });
      shot.dataset.act = "screen";
      acts.push(shot);
      if (isRemoteOrigin(origin)) {
        const att = button({ label: copyText("sessionHead.act.openTerm"), size: "compact", onClick: () => this.acts.attach(origin, tmuxName) });
        att.dataset.act = "attach";
        acts.push(att);
      }
      const kill = button({ label: copyText("sessionState.killIdle.action"), size: "compact", onClick: () => void this.killIn(sl.id) });
      kill.dataset.act = "kill";
      acts.push(kill);
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
  arrive: askArrive,
  attach: (origin, tmuxName) => void openSession(origin, { what: { kind: "look", session: tmuxName }, local: isLocalOrigin(origin) }),
  kill: async (origin, tmuxName) => void (await killSession(origin, tmuxName)),
  confirm: confirmDialog,
};
