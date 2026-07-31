/**
 * S4a（settings-ia）：「**当前在看哪台机器**」的单一事实来源。
 *
 * # 要治的病
 *
 * 主计划 §5-4 逐字记录：`accounts` / `mcp` / `cc-bus` / `cc-bus-hooks`
 * **各自维护 `this.origin`，`events.ts` 无任何 origin 广播** ⇒ 四份不同步。
 * 用户在「账号」里切到 aya，转头看「MCP」还停在上一台 —— 而这两块讲的是**同一台机器**。
 *
 * 四份的形状还各不相同（两个 `<select>`、一排按钮、一个直接读 DOM 值），
 * 所以它们连「怎么算切换了」都对不齐。
 *
 * # 语义
 *
 * `null` = **本机**。字符串 = 某台远端的 origin（`label || host`，与
 * `remote-config.ts::hostKey` 同口径）。
 *
 * # 刻意不做的两件事
 *
 * - **不持久化**。「我现在在看哪台」是会话内的导航状态，不是配置。存下来会让下次打开
 *   设置停在一台可能已经被删掉的机器上。
 * - **同值不通知**。四个订阅者收到通知就会 reload，而每次 reload 是一次 ssh 往返。
 *   同值重复 `set` 也广播的话，四块之间会互相激起一串无意义的往返 —— 那就是变相轮询，
 *   撞主计划 §1-2 的红线。
 */

type Listener = (origin: string | null) => void;

let current: string | null = null;
const listeners = new Set<Listener>();

/** 当前机器。`null` = 本机。 */
export function getCurrentMachine(): string | null {
  return current;
}

/**
 * 切到某台机器。**值没变就什么都不做**（见文件头注：同值广播 = 变相轮询）。
 */
export function setCurrentMachine(origin: string | null): void {
  const next = origin === "" ? null : origin;
  if (next === current) return;
  current = next;
  for (const fn of [...listeners]) {
    try {
      fn(next);
    } catch (e) {
      // 一个订阅者抛异常不能让其余的收不到通知 —— 同 `panel.ts::safeBlock` 的隔离思路。
      // 这里的后果比一块白屏更隐蔽：其余三块会**停在上一台机器**上，界面看不出异常，
      // 用户以为自己在看 aya，其实在看 nano。
      console.warn("[machine-context] 订阅者抛异常：", e);
    }
  }
}

/** 订阅切换。返回退订函数。 */
export function subscribeMachine(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** 仅供测试：把 store 还原成初始状态（本机 + 无订阅者）。 */
export function __resetMachineContextForTests(): void {
  current = null;
  listeners.clear();
}
