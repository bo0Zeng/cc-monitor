/**
 * S4a（settings-ia）：「**当前在看哪台机器**」的单一事实来源。
 *
 * # 要治的病
 *
 * 记录：`accounts` / `mcp` / `cc-bus` / `cc-bus-hooks`
 * **各自维护 `this.origin`，`events.ts` 无任何 origin 广播** ⇒ 四份不同步。
 * 用户在「账号」里切到 aya，转头看「MCP」还停在上一台 —— 而这两块讲的是**同一台机器**。
 *
 * 四份的形状还各不相同（两个 `<select>`、一排按钮、一个直接读 DOM 值），
 * 所以它们连「怎么算切换了」都对不齐。
 *
 * # 语义
 *
 * 值是一个 origin：本机 = `LOCAL_ORIGIN`（`"<local>"`）；其余 = 某台远端的 origin（`label || host`，与
 * `remote-config.ts::hostKey` 同口径）。
 *
 * 上一版是「`null` = 本机」，于是四个订阅者各自写一遍
 * `origin === null ? LOCAL_ORIGIN : origin` 把它换成后端认的那个串（`cc-bus-section.ts` 那句注释逐字：
 * 「两套表示各有各的理由，**换算只准在这一处发生**」）。现在只有一套表示，换算没有了。
 * 空白名**不是任何一台机器** ⇒ `set` 收到它什么都不做（上一版把它归一成「本机」——
 * 那正是步 2 要治的「没说被当成本机」，与 Rust `Origin::route` 拒空白名同一条）。
 *
 * # 刻意不做的两件事
 *
 * - **不持久化**。「我现在在看哪台」是会话内的导航状态，不是配置。存下来会让下次打开
 *   设置停在一台可能已经被删掉的机器上。
 * - **同值不通知**。四个订阅者收到通知就会 reload，而每次 reload 是一次 ssh 往返。
 *   同值重复 `set` 也广播的话，四块之间会互相激起一串无意义的往返 —— 那就是变相轮询，
 *   撞红线。
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
