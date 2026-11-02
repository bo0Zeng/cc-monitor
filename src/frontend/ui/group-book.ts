/**
 * **组的账**：组是后端的数据（`history/groups.rs`），是跨机器的集合——没有哪一台是它的主人，跟着组员走：
 * 每台只存自己有组员的那一片（名字带「几时起的」抄进每一片），组员与固定是会话注解（住会话那一台）。
 *
 * - 读：只问本机后端 `groups-read {machines}`，它问到各台、合一次（名字 · 组员 · 组头计数 · 哪台上次所知全在后端），这里只记要问哪几台、交出合好的那一份。
 * - 写：建组只向本机要一个 id（不写盘，回整份记录）· 进出组问组员那一台的 `groups-write move`（带那一组的记录，进组时抄进那一片）·
 *   改名 · 解散交本机 `groups-write`、带上这一组组员所在的那几台（本机发给各片，回没送到的那几台）· 固定问会话那一台的 `history-annotate`。
 *   写完不改这里的那一份：那台推 `changed {groups}`，再问一次（[`GroupBook.poke`]）。
 * - 什么时候问：起来 · 那台接上 · 机器表加了台 · 那台推 `changed {groups}`（连着来的合成一问）。读失败 ⇒ 留着上一份。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL, type Machine } from "./ipc/origin";
import type { TypedCommands } from "./generated/TypedCommands";
import type { GroupsRead } from "./generated/GroupsRead";
import type { GroupsWritten } from "./generated/GroupsWritten";
import type { HereGroup } from "./generated/HereGroup";

/** 本机后端问到各台再合：等这么久（同「需手动」那一问）。 */
const READ_BUDGET_MS = 20_000;
/** 写一份小文件（会话那台的注解 · 那一片的组）· 本机发给各片：等这么久（同注解那几问）。 */
const WRITE_BUDGET_MS = 10_000;
/** 推送连着来时合成一次重问。 */
const POKE_MS = 300;

type GroupCmd = "groups-read" | "groups-write" | "history-annotate";

/** 问一台一条组那几条命令（生产 ＝ `callTyped`；判据换成替身）。 */
export type GroupCall = <K extends GroupCmd>(origin: Machine, cmd: K, req: TypedCommands[K]["req"]) => Promise<TypedCommands[K]["reply"]>;

const callGroups: GroupCall = (async (origin: Machine, cmd: GroupCmd, req: unknown) => {
  // 每条命令名照字面写（交通道的那一处认得出说的是哪条）。
  switch (cmd) {
    case "groups-read":
      return callTyped(origin, "groups-read", req as TypedCommands["groups-read"]["req"], budgetWithin(READ_BUDGET_MS));
    case "groups-write":
      return callTyped(origin, "groups-write", req as TypedCommands["groups-write"]["req"], budgetWithin(WRITE_BUDGET_MS));
    case "history-annotate":
      return callTyped(origin, "history-annotate", req as TypedCommands["history-annotate"]["req"], budgetWithin(WRITE_BUDGET_MS));
  }
}) as GroupCall;

export class GroupBook {
  /** 要问的那几台（问过的都记着，照先来的先后）。 */
  private readonly origins: Machine[] = [];
  private readonly listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private seq = 0;
  private last: GroupsRead | null = null;

  constructor(
    private readonly call: GroupCall = callGroups,
    private readonly pokeMs: number = POKE_MS,
  ) {}

  /** 合好的最近一份（还没读到过 ⇒ `null`）。 */
  read(): GroupsRead | null {
    return this.last;
  }

  /** 读回来一份新的 ⇒ 叫一声。回退订。 */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** 这几台记进要问的、问一次。 */
  refreshAll(origins: readonly Machine[]): Promise<void> {
    for (const o of origins) if (!this.origins.includes(o)) this.origins.push(o);
    return this.ask();
  }

  /** 那台从机器表里删了：不再问它，剩下的再问一次。 */
  drop(origin: Machine): Promise<void> {
    const i = this.origins.indexOf(origin);
    if (i < 0) return Promise.resolve();
    this.origins.splice(i, 1);
    return this.ask();
  }

  /** 那台的组那一片变了（推 `changed {groups}` · 这里刚写过）：记进要问的，连着来的合成一问。 */
  poke(origin: Machine): void {
    if (!this.origins.includes(origin)) this.origins.push(origin);
    if (this.timer !== null) return;
    // 调度：合批 —— 那台连着推来的几帧 `changed {groups}` 合成一问
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.ask();
    }, this.pokeMs);
  }

  /** 起一个新组：本机起 id 与时刻、不写盘；回的那一份随第一位组员的 [`move`] 抄进它那一台。 */
  async make(name: string): Promise<HereGroup> {
    const r = await this.call(LOCAL, "groups-write", { op: "make", name });
    if (r.is !== "made") throw new Error(`groups-write make replied ${r.is}`);
    return { id: r.id, name: r.name, made: r.made, named: r.named };
  }

  /** 进出组（问组员那一台）：出 `leave` 那一组、进 `join` 那一组（带整份记录）。 */
  async move(origin: Machine, sid: string, join: HereGroup | null, leave: string | null): Promise<void> {
    await this.call(origin, "groups-write", { op: "move", sid, ...(join ? { join } : {}), ...(leave !== null ? { leave } : {}) });
    this.poke(origin);
  }

  /** 改名：本机发给 `machines` 那几片；有没送到的 ⇒ 抛出，说出哪几台。 */
  async rename(id: string, name: string, machines: readonly Machine[]): Promise<void> {
    this.sent(await this.call(LOCAL, "groups-write", { op: "rename", id, name, machines: [...machines] }), machines);
  }

  /** 解散：本机发给 `machines` 那几片（各片摘掉那一组，会话一条不动）；有没送到的 ⇒ 抛出，说出哪几台。 */
  async dissolve(id: string, machines: readonly Machine[]): Promise<void> {
    this.sent(await this.call(LOCAL, "groups-write", { op: "dissolve", id, machines: [...machines] }), machines);
  }

  /** 固定 · 取消固定（问会话那一台的注解）。 */
  async pin(origin: Machine, sid: string, pinned: boolean): Promise<void> {
    await this.call(origin, "history-annotate", { sid, patch: { pinned } });
    this.poke(origin);
  }

  private sent(r: GroupsWritten, machines: readonly Machine[]): void {
    for (const o of machines) this.poke(o);
    if (r.is !== "sent") throw new Error(`groups-write replied ${r.is}`);
    if (r.missed.length > 0) throw new Error(`not delivered to: ${r.missed.map((m) => `${m.origin} (${m.why})`).join(", ")}`);
  }

  private async ask(): Promise<void> {
    const n = ++this.seq;
    let got: GroupsRead;
    try {
      got = await this.call(LOCAL, "groups-read", { machines: [...this.origins] });
    } catch (e) {
      console.warn("[组] 本机没答 groups-read（留着上一份）：", e);
      return;
    }
    if (n !== this.seq) return;
    this.last = got;
    for (const fn of this.listeners) fn();
  }
}
