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
  tasksPanelCollapsed: "cc-monitor.tasks-panel.collapsed",
  /** issue #23：agents 面板折叠状态（与 tasks 面板同形态的全局单例） */
  agentsPanelCollapsed: "cc-monitor.agents-panel.collapsed",
  /** v2.1.0 issue #7：每分组的折叠状态。动态生成 key。 */
  settingsCollapsed: (groupId: string) => `cc-monitor.settings.collapsed.${groupId}`,
  /** issue #12：fork 树展开状态（按 sessionId 入集合）。 */
  historyExpandedForks: "cc-monitor.history.expanded-forks",
  /** v2.3.0：tool result 渲染模式偏好（per tool name）。 */
  toolRender: (toolName: string) => `cc-monitor.tool-render.${toolName}`,
  /** v1.7：cc 集成 PowerShell profile 选择 + 自定义路径。
   *  下划线命名保留避免改 key 时丢用户已存数据（迁移成本大于收益）。 */
  profilePreset: "cc-monitor.profile_preset",
  profilePath: "cc-monitor.profile_path",
  /** Batch5-F19：上次所在 tab 的 sid——启动 active 选择 + replay 优先级。 */
  lastActiveSid: "cc-monitor.last-active-sid",
  /** Batch14-F48：SFTP 面板目录书签(按 origin)。 */
  sftpBookmarks: (origin: string) => `cc-monitor.sftp.bookmarks.${origin}`,
  /** F86(#45)：历史来源筛选——被隐藏的来源 key 列表（origin ?? ""），跨重启保持。 */
  historyHiddenOrigins: "cc-monitor.history.hidden-origins",
  /** F86(#45)：历史来源大区折叠偏好覆盖表（key=origin ?? ""，缺键=走默认：本地展开/远端折叠）。 */
  historyOriginOpen: "cc-monitor.history.origin-open",
  /** F76b(#46)：历史远端「来源列表」快照持久化（`{projects,loadedAt}`）——跨启动**首开**也暖、不再
   *  只本地。hydrate 时把 loadedAt 归 0 → 持久快照只作首帧暖绘、首开必刷新一次(不影响新鲜度)。 */
  historyRemoteSources: "cc-monitor.history.remote-sources",
  /** F84b-fix(batch18)：命令栏可发现 chip 是否已被用户见过——首运行给一次性微高亮，之后不再。 */
  cmdkHintSeen: "cc-monitor.cmdk-hint.seen",
  /** S3(settings-ia)：机器列表行上那几个状态格子的账本（origin → facet → 结论+时刻）。
   *  **纯 UI 缓存，不是权威数据**；丢了只是列表回到「未测过」，不影响任何行为。 */
  machineStatus: "cc-monitor.settings.machine-status",
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
