/**
 * localStorage 统一接入层（P2.1）。
 *
 * 之前各 panel / view 自写 `try { localStorage.getItem ... } catch + console.warn`，
 * 加 key 散落（命名风格不一：profile_preset 用 _，其他用 . 或 -）。本模块：
 *
 * 1. **集中 LS_KEYS** —— 所有 key 字面量收口到一份对象。新加 key 必须先在这里
 *    注册，否则模块内 grep 不到无法定位。
 * 2. **safeGet / safeSet 包 try/catch** —— 私密模式 / disk quota / WebView2 沙盒
 *    异常时不抛，只 console.warn。
 * 3. **safeGetJson / safeSetJson** —— JSON 序列化对象时复用。
 * 4. **enumeratePrefix** —— data-section 列出所有 `cc-monitor.*` key 时用。
 *
 * INVARIANT § 14：所有 key 必须 `cc-monitor.` 前缀。
 *
 * 注：`profile_preset` / `profile_path` 仍保留下划线命名 —— 改 key 会丢用户已存
 * 的偏好（cc 集成下次打开会回到 PS 5.1 默认）。未来若做版本迁移再统一。
 */

const LS_PREFIX = "cc-monitor." as const;

/** 集中常量。新加 key 必须在这里登记。 */
export const LS_KEYS = {
  /** 主窗口底部抽屉：开着哪一页、多高（`{"page":"tasks"|"agents"|"terminal"|null,"height":240}`）。纯界面偏好，丢了回到收着、缺省高。 */
  bottomDrawer: "cc-monitor.bottom-drawer",
  // 🔴 `tabArchiveCollapsed` 删掉 —— 归档抽屉整个不存在了。
  //    用户逐字「没有归档这个东西，不要归档，就是灰 tab」。盘上遗留的那个键无人再读
  //    （条 80：不为盘上已有状态留兼容 ⇒ 不写清理，让它自然作废）。
  /** v2.1.0 issue #7：每分组的折叠状态。动态生成 key。 */
  settingsCollapsed: (groupId: string) => `cc-monitor.settings.collapsed.${groupId}`,
  /** issue #12：fork 树展开状态（按 sessionId 入集合）。 */
  historyExpandedForks: "cc-monitor.history.expanded-forks",
  /** v2.3.0：tool result 渲染模式偏好（per tool name）。 */
  toolRender: (toolName: string) => `cc-monitor.tool-render.${toolName}`,
  // v1.7 那两个键（cc 集成的 PowerShell profile 选择 + 自定义路径）删掉 —— 「终端集成」页随 AL1c / AL1d 退役之后
  //    零读写（`AL1d.md §5` 第 6 条）。盘上遗留的值无人再读（条 80：不为盘上已有状态留兼容 ⇒ 不写清理，让它自然作废）。
  /** Batch5-F19：上次所在 tab 的 sid——启动 active 选择 + replay 优先级。 */
  lastActiveSid: "cc-monitor.last-active-sid",
  /** 历史页的界面偏好（看法 · 筛选：机器 · 时间 · 排序 · 显示已隐藏 · 搜内容时）。纯界面偏好，丢了回到默认。 */
  historyPrefs: "cc-monitor.history.prefs",
  /** F84b-fix(batch18)：命令栏可发现 chip 是否已被用户见过——首运行给一次性微高亮，之后不再。 */
  cmdkHintSeen: "cc-monitor.cmdk-hint.seen",
  /** S3(settings-ia)：机器列表行上那几个状态格子的账本（origin → facet → 结论+时刻）。
   *  **纯 UI 缓存，不是权威数据**；丢了只是列表回到「未测过」，不影响任何行为。 */
  machineStatus: "cc-monitor.settings.machine-status",
  /** 「有改动需重启」的原因集。存在设置窗网页的**会话存储**里（这一次启动的状态，重启即清），不在 localStorage。 */
  restartReasons: "cc-monitor.settings.restart-reasons",
  /** Batch11-F33：竖直 tab 栏拖出来的宽度（px）。从 `main.ts` 的直写收进来，读写者只有 `tab-bar-width.ts`。 */
  tabBarWidth: "cc-monitor.tab-bar-w",
  /** 主窗口稿 §5.2.2：「过程默认展开」（会话头「⋯」· Ctrl+O）。每扇窗一份；`"1"` 展开，其余收起。 */
  processExpanded: "cc-monitor.stream.process-expanded",
  /** 「谁说的」稿 A：「显示系统注入」（会话头「⋯」）。每扇窗一份；`"1"` 显示，其余不露。 */
  showInjected: "cc-monitor.stream.show-injected",
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

export function safeRemove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch (e) {
    console.warn(`[local-storage] remove ${key} failed:`, e);
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
