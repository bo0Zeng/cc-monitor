/**
 * 「当前在看哪台机器」的单一事实来源：设置里按机器的那几块都订这一处（各自存一份的话，在「账号」切到 devbox，
 * 转头看别处还停在上一台）。值是一个 origin：本机 ＝ `LOCAL_ORIGIN`，其余是那台远端的 origin（与 `remote-config.ts::hostKey` 同口径）。
 * 空白名不是任何一台机器 ⇒ `set` 收到它什么都不做（不许「没说」被当成本机，与 Rust `Origin::route` 同一条）。
 * 不持久化（会话内导航状态；存下来会停在一台可能已删的机器上）；同值不通知（每次通知都是一次 ssh 往返，同值广播就是变相轮询）。
 */

import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
// 值与订阅住 `app-store.ts` 那一格（唯一的 pub-sub）；本文件只剩「空白名不是任何一台」这道门。
import { appStore } from "../app-store";

type Listener = (origin: Origin) => void;

/** 当前机器（本机 = `LOCAL_ORIGIN`）。 */
export function getCurrentMachine(): Origin {
  return appStore.machine.get();
}

/**
 * 切到某台机器。**值没变就什么都不做**（见文件头注：同值广播 = 变相轮询；`Slice.set` 自己挡）。
 */
export function setCurrentMachine(origin: Origin): void {
  if (origin.trim() === "") return; // 空白名不是任何一台机器（见头注）
  appStore.machine.set(origin);
}

/** 订阅切换。返回退订函数。一个订阅者抛异常不挡其余的（`Slice` 里隔离）。 */
export function subscribeMachine(fn: Listener): () => void {
  return appStore.machine.subscribe(fn);
}

/** 仅供测试：把 store 还原成初始状态（本机 + 无订阅者）。 */
export function __resetMachineContextForTests(): void {
  appStore.machine.__resetForTests(LOCAL_ORIGIN);
}
