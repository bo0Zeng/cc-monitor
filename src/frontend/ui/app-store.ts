/**
 * 〔「一个 store，一个 router」〕**唯一的 pub-sub 原语 ＋ 窗口级那几格应用状态。**
 *
 * 设计原文：「`origin` · 当前机器 · 账号快照 · tab 集合 —— 收进一处，订阅制。**只有一份 pub-sub。**」
 * ⇒ 「谁变了、谁要知道」只经 [`Slice`]：`TabStore` 的 tab 集合 / 当前 tab 那两格、设置窗的当前机器、
 * 账号快照（每台一份）· 每个会话的号都是它的实例；别处不再各养一份 listeners。
 *
 * 语义两条（与原先 `machine-context.ts` 同）：**同值不通知**（每次通知都可能是一次往返，同值广播 = 变相轮询）；
 * 一个订阅者抛异常不挡其余订阅者。零 DOM、零 IPC。
 */
import type { AccountsState } from "./accounts";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { QuotaRead } from "./acct-words";
import type { RotationViewReply } from "./generated/RotationViewReply";
import type { SessionAccountReply } from "./generated/SessionAccountReply";

/** 一格状态：取值 · 置值（同值不通知）· 订阅（回退订函数；订阅那一刻不回放当前值）。 */
export class Slice<T> {
  private readonly listeners = new Set<(v: T) => void>();

  constructor(
    private value: T,
    private readonly same: (a: T, b: T) => boolean = Object.is,
  ) {}

  get(): T {
    return this.value;
  }

  set(next: T): void {
    if (this.same(this.value, next)) return;
    this.value = next;
    for (const fn of [...this.listeners]) {
      try {
        fn(next);
      } catch (e) {
        // 一个订阅者坏了，其余的照样要收到（否则它们停在旧值上、界面看不出异常）。
        console.warn("[app-store] 订阅者抛异常：", e);
      }
    }
  }

  subscribe(fn: (v: T) => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** 只给判据用：值还原、订阅者清空。 */
  __resetForTests(v: T): void {
    this.value = v;
    this.listeners.clear();
  }
}

/** 一个会话的号：会话那台写好的那一份（`session-account`；状态栏按钮 · 号徽章照它画）。 */
export interface SessionAccountEntry {
  origin: Origin;
  read: SessionAccountReply;
}

/** 一个会话在它那台的轮换（`rotation-view` 那一份；中转没见过这个会话 ⇒ `read: null`）。 */
export interface SessionRotationEntry {
  origin: Origin;
  read: RotationViewReply | null;
}

/** `quota-read` 整问失败：出错那一层写好的那一句（`said`）与复制详情（`detail`，没有 ⇒ 空串）。 */
export interface QuotaFailed {
  state: "failed";
  said: string;
  detail: string;
}

/** 那台的额度账还没问回来。 */
export interface QuotaPending {
  state: "pending";
}

/**
 * 一台的额度账此刻是哪一形（一个事实一处）：读到的账（那台写好的成品，`state` 照它自己的三态）· 整问失败那一句 · 还没问回来。
 * 存在 `appStore.quota` 里的只有前两形；没有那一格 ＝ 还没问回来（{@link quotaOf} 补成第三形）。
 */
export type QuotaSlot = QuotaRead | QuotaFailed | QuotaPending;

/** 窗口级的几格（每个窗口一份 —— 各窗口本来就是各自的 JS 环境）。 */
export const appStore = {
  /** 设置窗「当前在看哪台机器」（本机 = `LOCAL_ORIGIN`）。 */
  machine: new Slice<Origin>(LOCAL_ORIGIN),
  /** 每台机器最近一次问回来的账号清单（`account-reads.ts` 每问一次换一份）：只给同步画的那几处（头像号色 · 号牌字 · chip）读，要用清单的地方现问。 */
  accounts: new Slice<ReadonlyMap<Origin, AccountsState>>(new Map()),
  /** 每个会话的号（按 sid；还没问回来 ⇒ 不在里面）。 */
  sessionAccount: new Slice<ReadonlyMap<string, SessionAccountEntry>>(new Map()),
  /** 每台的额度账（{@link QuotaSlot}：读到的账 · 整问失败那一句；还没问回来 ⇒ 不在里面）。只有 `acct-center.ts` 写；读经 {@link quotaOf} / {@link quotaReadOf}。 */
  quota: new Slice<ReadonlyMap<Origin, QuotaRead | QuotaFailed>>(new Map()),
  /** 每个会话的轮换（按 sid；标签页上的账号格照它画）。 */
  sessionRotation: new Slice<ReadonlyMap<string, SessionRotationEntry>>(new Map()),
};

/** 往一张按键的表里换一格（整张换一份新的，订阅者按自己那一格取）。 */
export function putIn<K, V>(slice: Slice<ReadonlyMap<K, V>>, key: K, value: V): void {
  const next = new Map(slice.get());
  next.set(key, value);
  slice.set(next);
}

/** 那台的额度账此刻是哪一形（没有那一格 ⇒ 还没问回来）。 */
export function quotaOf(origin: Origin): QuotaSlot {
  return appStore.quota.get().get(origin) ?? { state: "pending" };
}

/** 那台读到的账；整问失败 · 还没问回来 ⇒ `null`（只要号的用量那几处用它）。 */
export function quotaReadOf(origin: Origin): QuotaRead | null {
  const q = quotaOf(origin);
  return q.state === "failed" || q.state === "pending" ? null : q;
}
