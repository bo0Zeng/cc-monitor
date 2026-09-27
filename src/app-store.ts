/**
 * 〔GAP1 · `设计/01 §1.5`「一个 store，一个 router」〕**唯一的 pub-sub 原语 ＋ 窗口级那几格应用状态。**
 *
 * 设计原文：「`origin` · 当前机器 · 账号快照 · tab 集合 —— 收进一处，订阅制。**只有一份 pub-sub。**」
 * ⇒ 「谁变了、谁要知道」只经 [`Slice`]：`TabStore` 的 tab 集合 / 当前 tab 那两格、设置窗的当前机器、
 * 账号快照（每台一份 · 徽章那份）都是它的实例；别处不再各养一份 listeners。
 *
 * 语义两条（与原先 `machine-context.ts` 同）：**同值不通知**（每次通知都可能是一次往返，同值广播 = 变相轮询）；
 * 一个订阅者抛异常不挡其余订阅者。零 DOM、零 IPC。
 */
import type { AccountsState, SessionAccount } from "./accounts";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";

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

/** tab 栏徽章那份账号快照（主窗口的会话账号刷新器算好、整份换）。 */
export interface SessionAccountsSnapshot {
  rows: SessionAccount[];
  emailByName: Map<string, string>;
  lastByS: Map<string, string>;
  readyOrigins: Set<string>;
  currentByOrigin: Map<string, string>;
}

/** 窗口级的几格（每个窗口一份 —— 各窗口本来就是各自的 JS 环境）。 */
export const appStore = {
  /** 设置窗「当前在看哪台机器」（本机 = `LOCAL_ORIGIN`）。 */
  machine: new Slice<Origin>(LOCAL_ORIGIN),
  /** 每台机器最近一次取回来的账号清单（`account-reads.ts` 每取回一次换一份；`null` = 那一问没结果）。 */
  accounts: new Slice<ReadonlyMap<Origin, AccountsState | null>>(new Map()),
  /** tab 栏账号徽章那份快照（`null` = 还没算过）。 */
  sessionAccounts: new Slice<SessionAccountsSnapshot | null>(null),
};

/** 把某台的账号清单写进 `appStore.accounts`（整张表换一份新的，订阅者按自己那台取）。 */
export function putAccounts(origin: Origin, state: AccountsState | null): void {
  const cur = appStore.accounts.get();
  if (cur.has(origin) && cur.get(origin) === state) return; // 缓存命中回的同一份 ⇒ 不算变
  const next = new Map(cur);
  next.set(origin, state);
  appStore.accounts.set(next);
}
