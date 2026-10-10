/**
 * localStorage 统一接入层：
 *
 * 1. **集中 LS_KEYS** —— 所有 key 字面量收口到一份对象。新加 key 必须先在这里
 *    注册，否则模块内 grep 不到无法定位。
 * 2. **safeGet / safeSet 包 try/catch** —— 私密模式 / disk quota / WebView2 沙盒
 *    异常时不抛，只 console.warn。
 * 3. **safeGetJson / safeSetJson** —— JSON 序列化对象时复用。
 * 4. **enumeratePrefix** —— data-section 列出所有 `cc-monitor.*` key 时用。
 *
 * INVARIANT § 14：所有 key 必须 `cc-monitor.` 前缀。
 */

const LS_PREFIX = "cc-monitor." as const;

/** 集中常量。新加 key 必须在这里登记。 */
export const LS_KEYS = {
  /** 主窗口底部抽屉：开着哪一页、多高（`{"page":"tasks"|"agents"|"terminal"|null,"height":240}`）。纯界面偏好，丢了回到收着、缺省高。 */
  bottomDrawer: "cc-monitor.bottom-drawer",
  /** fork 树展开状态（按 sessionId 入集合）。 */
  historyExpandedForks: "cc-monitor.history.expanded-forks",
  /** tool result 渲染模式偏好（按工具名）。 */
  toolRender: (toolName: string) => `cc-monitor.tool-render.${toolName}`,
  /** 上次所在 tab 的 sid —— 启动时选它、重放先发它。 */
  lastActiveSid: "cc-monitor.last-active-sid",
  /** 历史页的界面偏好（看法 · 筛选：机器 · 时间 · 排序 · 显示已隐藏 · 搜内容时）。纯界面偏好，丢了回到默认。 */
  historyPrefs: "cc-monitor.history.prefs",
  /** 计划页的界面偏好（选中哪一片 · 不做了藏不藏 · 折着的格 · 手加的工作区目录）。 */
  planPrefs: "cc-monitor.plan.prefs",
  /** 命令栏 chip 是否已被用户见过 —— 首次运行给一次性微高亮，之后不再。 */
  cmdkHintSeen: "cc-monitor.cmdk-hint.seen",
  /** 「有改动需重启」的原因集。存在设置窗网页的**会话存储**里（这一次启动的状态，重启即清），不在 localStorage。 */
  restartReasons: "cc-monitor.settings.restart-reasons",
  /** 竖直 tab 栏拖出来的宽度（px）。读写者只有 `tab-bar-width.ts`。 */
  tabBarWidth: "cc-monitor.tab-bar-w",
  /** 「过程默认展开」（会话头「⋯」· Ctrl+O）。每扇窗一份；`"1"` 展开，其余收起。 */
  processExpanded: "cc-monitor.stream.process-expanded",
  /** 「谁说的」稿 A：「显示系统注入」（会话头「⋯」）。每扇窗一份；`"1"` 显示，其余不露。 */
  showInjected: "cc-monitor.stream.show-injected",
  /** 主窗口缩放（`Ctrl+=` / `Ctrl+-` / `Ctrl+0`）：倍数串，如 `"1.25"`。丢了回到 1。 */
  mainZoom: "cc-monitor.main.zoom",
  /** 标签页栏手动收起（命令面板「收起标签页栏」）：`"1"` 收起，其余照窗宽。 */
  tabBarFolded: "cc-monitor.tab-bar.folded",
} as const;

export function safeGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch (e) {
    console.warn(`[local-storage] get ${key} failed:`, e);
    return null;
  }
}

export function safeSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch (e) {
    console.warn(`[local-storage] set ${key} failed:`, e);
  }
}

export function safeGetJson<T>(key: string): T | null {
  const raw = safeGet(key);
  if (raw === null) return null;
  try {
    return JSON.parse(raw) as T;
  } catch (e) {
    console.warn(`[local-storage] parse ${key} failed:`, e);
    return null;
  }
}

export function safeSetJson<T>(key: string, value: T): void {
  try {
    safeSet(key, JSON.stringify(value));
  } catch (e) {
    console.warn(`[local-storage] stringify ${key} failed:`, e);
  }
}

/** 枚举所有 `cc-monitor.` 前缀的 key（data-section 用）。 */
export function enumeratePrefix(prefix: string = LS_PREFIX): Array<{ key: string; value: string }> {
  const out: Array<{ key: string; value: string }> = [];
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (!k || !k.startsWith(prefix)) continue;
      out.push({ key: k, value: localStorage.getItem(k) ?? "" });
    }
  } catch (e) {
    console.warn(`[local-storage] enumeratePrefix ${prefix} failed:`, e);
  }
  out.sort((a, b) => a.key.localeCompare(b.key));
  return out;
}
