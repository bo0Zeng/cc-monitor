/**
 * 「有改动要重启才生效」的单一去处：静态说明进 ⓘ（读一次就够）；「现在确实有改动待生效」这个状态收敛到一条底部常驻条，
 * 只在真有改动时出现、列出改了什么。状态性 / 安全性警告不得只活在悬停 / 点击之后（`INVARIANTS §12`），所以它常驻、不收进图标。
 */

/**
 * 这条状态属于这一次启动：记在设置窗网页的 `sessionStorage` 里（设置窗关了只是藏起来，关窗再开 / 网页重载条都还在；
 * monitor 真重启 ＝ 新的设置窗网页，条就消了）。不放 `localStorage`：否则重启之后条还挂着。
 */
import { LS_KEYS } from "../local-storage";
import { copyText } from "../copy-table";
import { restartNowButton } from "./restart-now";

type Listener = (reasons: string[]) => void;

/** 用 Set 而不是数组：同一个原因反复标记（每改一次远端配置都会标）只算一条。 */
const reasons = new Set<string>(loadPersisted());
const listeners = new Set<Listener>();

function loadPersisted(): string[] {
  let raw: string | null = null;
  try {
    raw = sessionStorage.getItem(LS_KEYS.restartReasons);
  } catch (e) {
    console.warn("[restart-notice] 读不到会话存储：", e);
  }
  if (!raw) return [];
  try {
    const p = JSON.parse(raw) as unknown;
    return Array.isArray(p) ? p.filter((r): r is string => typeof r === "string") : [];
  } catch {
    return []; // 坏数据当没有——这条只是提示，不值得为它报错
  }
}

function persist(): void {
  try {
    sessionStorage.setItem(LS_KEYS.restartReasons, JSON.stringify([...reasons]));
  } catch (e) {
    console.warn("[restart-notice] 写不进会话存储：", e);
  }
}

/** 记一笔「这项改动要重启才生效」。`reason` 是给人读的短句（如「远端机器配置」），原样列在条上。 */
export function markRestartNeeded(reason: string): void {
  const r = reason.trim();
  if (!r || reasons.has(r)) return; // 同值不通知：避免每敲一个字符就重渲染一次
  reasons.add(r);
  persist(); // 关窗再开不能丢
  notify();
}

/**
 * 那一项拨回了这次运行起来时的值 ⇒ 它不再「待重启才生效」，这一笔划掉（行内那一句与这条一起消）。
 * 只给「改回原值」这一种情形用：改动还没生效时不许拿它把条划掉。
 */
export function clearRestartNeeded(reason: string): void {
  if (!reasons.delete(reason.trim())) return;
  persist();
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

/** 仅供测试：清空（生产里只有「拨回原值」那一种划掉，见 [`clearRestartNeeded`]）。 */
export function __resetRestartNoticeForTests(): void {
  reasons.clear();
  listeners.clear();
  persist();
}

/** 仅供测试：把存着的那份重新读进内存（模拟「设置窗网页重新载入」）。 */
export function __rehydrateRestartNoticeForTests(): void {
  reasons.clear();
  for (const r of loadPersisted()) reasons.add(r);
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
 * 底部常驻条：只在有待生效改动时出现。不给「知道了」按钮（改动还没生效不会因为点了一下就不成立）；
 * 消失只有两个办法：真的重启，或把那一项拨回这次运行起来时的值。
 */
export function createRestartBar(): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "settings-restart-bar";
  bar.setAttribute("role", "status");
  const text = document.createElement("span");
  bar.append(text, restartNowButton());
  const render = (list: string[]): void => {
    bar.hidden = list.length === 0;
    text.textContent = list.length === 0 ? "" : copyText("restartNotice.render.pending", { n: list.length, list: list.join(copyText("restartNotice.render.listSep")) });
  };
  render(restartReasons());
  subscribeRestart(render);
  return bar;
}
