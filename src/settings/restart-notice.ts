/**
 * S7（settings-ia）：「**有改动要重启才生效**」的单一去处。
 *
 * # 要治的病：同一句话散在三处，且都是**假的常驻**
 *
 * 「⚠ 需重启 monitor 才生效」此前逐字出现在至少三个地方
 * （`remote-section` 两处、`diagnostics-section` 一处）。它们有两个共同毛病：
 *
 * 1. **重复**：同一件事说三遍，用户每处都要读一遍才知道说的是同一件事。
 * 2. **恒显示 ≠ 状态**：那几句是**静态说明**（「这类设置改了要重启」），
 *    不管你有没有真的改过都摆在那儿。于是它退化成背景噪音 ——
 *    等到**真的**改了、真的需要重启时，它和平时长得一模一样，没人会注意。
 *
 * ⇒ 拆成两半：**静态说明进 ⓘ**（读一次就够）；**「现在确实有改动待生效」这个状态
 * 收敛到一条底部常驻条**，只在真有改动时出现，并列出改了什么。
 *
 * # 为什么这条不能收进图标（§12，与用户原话的取舍）
 *
 * 用户 2026-07-31 的原话是「警告信息折叠起来变成一个图标，鼠标点击再显示」。
 * 对**静态长说明**这条完全成立，本轮就是这么做的。
 *
 * 但对**状态性**警告，`INVARIANTS §12` 立着一条相反的规矩：
 * 「用户看到了但没注意到关键信息」**已真实发生过一次**，所以状态性/安全性警告
 * 不得只活在 hover / 点击之后。同类判例：`cc-bus-hooks-section.ts` 逐字写着
 * 「必须上屏，否则那条后端判据等于白做」；机器卡片那条「指纹未经验证」是
 * **安全**警告（可能中间人），更不能藏。
 *
 * ⇒ **按 §12 判**：收进 ⓘ 的只有静态说明；「有改动待重启」「指纹未验证」这类
 * 状态性/安全性的一律常驻。**这一条是对用户原话的偏离，已在交付时明说。**
 */

type Listener = (reasons: string[]) => void;

/** 用 Set 而不是数组：同一个原因反复标记（每改一次远端配置都会标）只算一条。 */
const reasons = new Set<string>();
const listeners = new Set<Listener>();

/**
 * 记一笔「这项改动要重启才生效」。
 *
 * `reason` 是**给人读的短句**（如「远端机器配置」），会原样列在条上 ——
 * 只说「有改动」而不说改了什么，用户没法判断要不要现在就重启。
 */
export function markRestartNeeded(reason: string): void {
  const r = reason.trim();
  if (!r || reasons.has(r)) return; // 同值不通知：避免每敲一个字符就重渲染一次
  reasons.add(r);
  notify();
}

/** 当前待生效的改动（按加入顺序）。空 = 没有，条不该出现。 */
export function restartReasons(): string[] {
  return [...reasons];
}

export function subscribeRestart(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** 仅供测试：清空。**生产里刻意没有「清除」入口** —— 见下。 */
export function __resetRestartNoticeForTests(): void {
  reasons.clear();
  listeners.clear();
}

function notify(): void {
  const snapshot = [...reasons];
  for (const fn of [...listeners]) {
    try {
      fn(snapshot);
    } catch (e) {
      // 一个订阅者抛异常不能让其余的收不到 —— 同 machine-context 的隔离思路。
      console.warn("[restart-notice] 订阅者抛异常：", e);
    }
  }
}

/**
 * 底部常驻条。**只在有待生效改动时出现**，空时整块不渲染。
 *
 * 刻意**不给「知道了」按钮**：那会让用户把一个仍然为真的状态划掉
 * ——「改动还没生效」这件事不会因为他点了一下就不成立。要让它消失只有一个办法：
 * 真的重启。（这也是为什么生产里没有清除入口。）
 */
export function createRestartBar(): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "settings-restart-bar";
  bar.setAttribute("role", "status");
  const render = (list: string[]): void => {
    if (list.length === 0) {
      bar.hidden = true;
      bar.textContent = "";
      return;
    }
    bar.hidden = false;
    bar.textContent = `有改动需重启 monitor 才生效：${list.join(" · ")}`;
  };
  render(restartReasons());
  subscribeRestart(render);
  return bar;
}
