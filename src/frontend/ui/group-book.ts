/**
 * **组的账**：组是后端的数据（`history/groups.rs`）——组本身住家那一台、组员与固定是会话注解（住会话那一台）。
 *
 * - 读：只问本机后端 `groups-read {machines}`，它问到各台、并成一份（名字 · 组员 · 组头计数 · 哪台上次所知全在后端），这里只记要问哪几台、交出合好的那一份。
 * - 写：建组问本机（本机就是新组的家）· 改名 · 解散问家那一台（`groups-read` 交了 `home`）· 进出组 · 固定问会话那一台的 `history-annotate`。
 *   写完不改这里的那一份：那台推 `changed {groups}`，再问一次（[`GroupBook.poke`]）。
 * - 什么时候问：起来 · 那台接上 · 机器表加了台 · 那台推 `changed {groups}`（连着来的合成一问）。读失败 ⇒ 留着上一份。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { TypedCommands } from "./generated/TypedCommands";
import type { GroupsRead } from "./generated/GroupsRead";
import type { Patch } from "./generated/Patch";

/** 本机后端问到各台再合：等这么久（同「需手动」那一问）。 */
const READ_BUDGET_MS = 20_000;
/** 写一份小文件（家那台的组 · 会话那台的注解）：等这么久（同注解那几问）。 */
const WRITE_BUDGET_MS = 10_000;
/** 推送连着来时合成一次重问。 */
const POKE_MS = 300;

type GroupCmd = "groups-read" | "groups-write" | "history-annotate";

/** 问一台一条组那几条命令（生产 ＝ `callTyped`；判据换成替身）。 */
export type GroupCall = <K extends GroupCmd>(origin: Origin, cmd: K, req: TypedCommands[K]["req"]) => Promise<TypedCommands[K]["reply"]>;

const callGroups: GroupCall = (origin, cmd, req) =>
  callTyped(origin, cmd, req, budgetWithin(cmd === "groups-read" ? READ_BUDGET_MS : WRITE_BUDGET_MS));

export class GroupBook {
  /** 要问的那几台（问过的都记着，照先来的先后）。 */
  private readonly origins: Origin[] = [];
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
  refreshAll(origins: readonly Origin[]): Promise<void> {
    for (const o of origins) if (!this.origins.includes(o)) this.origins.push(o);
    return this.ask();
  }

  /** 那台从机器表里删了：不再问它，剩下的再问一次。 */
  drop(origin: Origin): Promise<void> {
    const i = this.origins.indexOf(origin);
    if (i < 0) return Promise.resolve();
    this.origins.splice(i, 1);
    return this.ask();
  }

  /** 那台的组那一半变了（推 `changed {groups}` · 这里刚写过）：记进要问的，连着来的合成一问。 */
  poke(origin: Origin): void {
    if (!this.origins.includes(origin)) this.origins.push(origin);
    if (this.timer !== null) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.ask();
    }, this.pokeMs);
  }

  /** 建一组（本机是家），回新组的 id。 */
  async make(name: string): Promise<string> {
    const r = await this.call(LOCAL_ORIGIN, "groups-write", { op: "make", name });
    if (r.is !== "made") throw new Error(`groups-write make replied ${r.is}`);
    this.poke(LOCAL_ORIGIN);
    return r.id;
  }

  /** 改名（问家那一台）。 */
  async rename(home: Origin, id: string, name: string): Promise<void> {
    await this.call(home, "groups-write", { op: "rename", id, name });
    this.poke(home);
  }

  /** 解散（问家那一台）：只去掉组，会话一条不动。 */
  async dissolve(home: Origin, id: string): Promise<void> {
    await this.call(home, "groups-write", { op: "dissolve", id });
    this.poke(home);
  }

  /** 进出组 · 固定（问会话那一台的注解）：`patch` 只带 `join` · `leave` · `pinned` 那几格。 */
  async member(origin: Origin, sid: string, patch: Pick<Patch, "join" | "leave" | "pinned">): Promise<void> {
    await this.call(origin, "history-annotate", { sid, patch });
    this.poke(origin);
  }

  private async ask(): Promise<void> {
    const n = ++this.seq;
    let got: GroupsRead;
    try {
      got = await this.call(LOCAL_ORIGIN, "groups-read", { machines: [...this.origins] });
    } catch (e) {
      console.warn("[组] 本机没答 groups-read（留着上一份）：", e);
      return;
    }
    if (n !== this.seq) return;
    this.last = got;
    for (const fn of this.listeners) fn();
  }
}
