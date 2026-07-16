/**
 * issue #5: 快捷键系统的 **Action 清单 = 单一真相源**。
 *
 * 这里改一条新增/改默认/标可用性，全套（dispatcher / 编辑器 UI / 持久化 schema）
 * 自动收敛 —— 各处只引 ACTIONS 不要自己写动作 id 字面量。
 *
 * ## 设计
 *
 * - `id`：稳定字符串 key，进 config.json `keybindings.<id>` 持久化字段
 * - `default`：默认 chord（规范化串，详 `registry.ts::normalizeChord`）；`null` = 默认未绑
 * - `available`：false 表示功能未上线（editor.ts 灰显并标"未上线"），用户既不能
 *   触发它（因为代码没 bind）也不能改它的绑定。预留位置让用户提前知道路线图
 * - `category`：UI 表格分组用，纯展示
 *
 * ## chord 字符串规范
 *
 * 见 `registry.ts::normalizeChord`：modifier 固定顺序 `Ctrl+Shift+Alt+Meta+<code>`，
 * 用 `KeyboardEvent.code`（不是 `key`）避免布局差异 —— 法语键盘 `Ctrl+Comma` 永远
 * 对得上 `e.code === "Comma"`，而 `e.key === ","` 在那个布局压根触发不了。
 */

export type Category = "Tab" | "Term" | "App" | "Beh" | "Panel";

export interface Action {
  /** 稳定 id，进 config 的 key */
  readonly id: string;
  /** UI 表格显示的名字 */
  readonly label: string;
  /** 表格分组列 */
  readonly category: Category;
  /**
   * 默认 chord（normalize 过的串，如 `"Ctrl+Tab"` / `"Ctrl+Shift+KeyW"` / `"Escape"`）。
   * `null` = 默认无绑定（用户可在编辑器里给它绑）。
   */
  readonly default: string | null;
  /**
   * 功能是否已上线。false 时编辑器灰显 + 显示 `comingSoon` 文案，主程序也不会
   * `bind()` 它（即使 config 里有覆盖也不会触发）。
   */
  readonly available: boolean;
  /** available=false 时显示给用户的说明 */
  readonly comingSoon?: string;
}

/**
 * 全部 action 清单。**顺序 = 编辑器 UI 表格里的显示顺序**（同 category 内手工排）。
 *
 * 加新 action 时：
 *  1. 这里加一条
 *  2. main.ts 里 `dispatcher.bind("<id>", callback)`
 *  3. （如果是预留）`available: false` + 文案
 */
// 默认快捷键全部为**单键**（无 Ctrl/Shift）—— cc-monitor 是只读监视窗口，主视图不接受
// 文本输入，单键导航更顺手。**前提**：dispatcher 在可编辑文本元素聚焦时不触发快捷键
// （registry.ts::isEditableTarget），否则单键会在历史搜索 / 设置输入 / 重命名框里误触发。
// 用户仍可在「设置 → 快捷键」编辑器里改成任意组合键。
export const ACTIONS: ReadonlyArray<Action> = [
  // ===== Tab =====
  { id: "tab.next", label: "切到下一个 Tab", category: "Tab", default: "BracketRight", available: true },
  { id: "tab.prev", label: "切到上一个 Tab", category: "Tab", default: "BracketLeft", available: true },
  { id: "tab.jump-1", label: "跳到第 1 个 Tab", category: "Tab", default: "Digit1", available: true },
  { id: "tab.jump-2", label: "跳到第 2 个 Tab", category: "Tab", default: "Digit2", available: true },
  { id: "tab.jump-3", label: "跳到第 3 个 Tab", category: "Tab", default: "Digit3", available: true },
  { id: "tab.jump-4", label: "跳到第 4 个 Tab", category: "Tab", default: "Digit4", available: true },
  { id: "tab.jump-5", label: "跳到第 5 个 Tab", category: "Tab", default: "Digit5", available: true },
  { id: "tab.jump-6", label: "跳到第 6 个 Tab", category: "Tab", default: "Digit6", available: true },
  { id: "tab.jump-7", label: "跳到第 7 个 Tab", category: "Tab", default: "Digit7", available: true },
  { id: "tab.jump-8", label: "跳到第 8 个 Tab", category: "Tab", default: "Digit8", available: true },
  { id: "tab.jump-9", label: "跳到第 9 个 Tab", category: "Tab", default: "Digit9", available: true },
  { id: "tab.close-archived", label: "关闭已归档 Tab", category: "Tab", default: "KeyW", available: true },
  { id: "tab.open-cwd", label: "打开当前 Tab 的工作目录", category: "Tab", default: "KeyE", available: true },
  {
    id: "tab.pop-out",
    label: "在新窗口打开当前 Tab",
    category: "Tab",
    default: "KeyN",
    available: true,
  },

  // ===== Terminal =====
  { id: "terminal.bring-front", label: "把对应终端窗口拉到前台", category: "Term", default: "Backquote", available: true },

  // ===== App =====
  { id: "app.open-settings", label: "打开设置面板", category: "App", default: "Comma", available: true },
  { id: "app.toggle-history", label: "打开 / 关闭历史浏览器", category: "App", default: "KeyH", available: true },
  { id: "app.toggle-panorama", label: "打开 / 关闭代码全景", category: "App", default: "KeyG", available: true },
  {
    id: "app.search-history",
    label: "历史浏览器全文搜索",
    category: "App",
    default: null,
    available: false,
    comingSoon: "已上线：H 打开历史后切「全文」模式（暂无独立快捷键）",
  },
  { id: "app.minimize", label: "最小化主窗口", category: "App", default: "KeyM", available: true },
  { id: "app.toggle-fullscreen", label: "切换真全屏（borderless 覆盖任务栏）", category: "App", default: "F11", available: true },
  {
    id: "overlay.close",
    label: "关闭弹层 / 历史 / 设置",
    category: "App",
    default: "Escape",
    available: true,
  },

  // ===== Behavior toggles =====
  { id: "behavior.toggle-auto-follow", label: "切换「自动跟随用户输入」", category: "Beh", default: null, available: true },
  { id: "behavior.toggle-bring-monitor", label: "切换「自动拉前 monitor」", category: "Beh", default: null, available: true },

  // ===== Panel =====
  { id: "panel.toggle-tasks", label: "Task 面板开 / 关", category: "Panel", default: "KeyT", available: true },
] as const;

/** 全部已注册 action 的 id 联合类型；调用方 `bind(id, ...)` 时 TS 检查拼写 */
export type ActionId = (typeof ACTIONS)[number]["id"];

/** id → Action 反查 */
export function findAction(id: string): Action | undefined {
  return ACTIONS.find((a) => a.id === id);
}

/** 按 category 分桶，编辑器表格用 */
export function groupByCategory(): Map<Category, Action[]> {
  const map = new Map<Category, Action[]>();
  for (const a of ACTIONS) {
    const arr = map.get(a.category) ?? [];
    arr.push(a);
    map.set(a.category, arr);
  }
  return map;
}

/** Category → 表头标签 */
export const CATEGORY_LABEL: Record<Category, string> = {
  Tab: "标签页",
  Term: "终端",
  App: "应用",
  Beh: "行为",
  Panel: "面板",
};
