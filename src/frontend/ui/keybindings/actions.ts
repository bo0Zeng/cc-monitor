/**
 * issue #5: 快捷键系统的 **Action 清单 = 单一源头**。
 *
 * 这里改一条新增/改默认/标可用性，全套（dispatcher / 编辑器 UI / 持久化 schema）
 * 自动收敛 —— 各处只引 ACTIONS 不要自己写动作 id 字面量。
 *
 * ## 设计
 *
 * - `id`：稳定字符串 key，进 config.json `keybindings.<id>` 持久化字段
 * - `default`：默认 chord（规范化串，详 `registry.ts::normalizeChord`）；`null` = 默认未绑
 * - `available`：false 表示功能未上线（editor.ts 灰显那一行），用户既不能
 *   触发它（因为代码没 bind）也不能改它的绑定。今天清单里没有未上线的：
 *   用不上的预留位不留（最后一条 `app.search-history`〔散文墓碑〕删了），真要做时再加
 * - `category`：UI 表格分组用，纯展示
 *
 * ## chord 字符串规范
 *
 * 见 `registry.ts::normalizeChord`：modifier 固定顺序 `Ctrl+Shift+Alt+Meta+<code>`，
 * 用 `KeyboardEvent.code`（不是 `key`）避免布局差异 —— 法语键盘 `Ctrl+Comma` 永远
 * 对得上 `e.code === "Comma"`，而 `e.key === ","` 在那个布局压根触发不了。
 */

import { copyText } from "../copy-table";

export type Category = "Tab" | "Term" | "App" | "Beh" | "Panel" | "Acct";

/**
 * 何时生效（作用范围）。带修饰键的那一档（Ctrl / Alt / ⌘）由 dispatcher 现判，这里写的是默认键的那一档：
 * - `any`：随时（输入框里也算；模态开着时只放行 Esc）；
 * - `idle`：没有输入焦点时；
 * - `main`：焦点在主区（消息流 · 会话头 · 没焦点）；
 * - `nav`：焦点在主区或标签页栏；
 * - `bare`：单键规则 —— 焦点在主区或标签页栏，且没有浮层 / 对话框 / 输入焦点。用户改成带修饰键的键 ⇒ 按 `any` 算。
 */
export type Scope = "any" | "idle" | "main" | "nav" | "bare";

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
   * 功能是否已上线。false 时编辑器灰显那一行，主程序也不会
   * `bind()` 它（即使 config 里有覆盖也不会触发）。
   */
  readonly available: boolean;
  /** 何时生效（见 [`Scope`]）。 */
  readonly scope: Scope;
  /** 另一个固定的键（不进配置、改不了），例如菜单键之于 `Shift+F10`。 */
  readonly also?: string;
}

/**
 * 全部 action 清单。**顺序 = 编辑器 UI 表格里的显示顺序**（同 category 内手工排）。
 *
 * 加新 action 时：
 *  1. 这里加一条
 *  2. main.ts 里 `dispatcher.bind("<id>", callback)`
 *  3. （如果是预留）`available: false`〔预留位不留 —— 真做时再加〕
 */
// 主视图不接受文本输入 ⇒ 导航多用单键；单键只在主区 / 标签页栏、没有浮层与输入焦点时生效（`Scope`）。
export const ACTIONS: ReadonlyArray<Action> = [
  // ===== Tab =====
  { id: "tab.next", label: copyText("keybindingActions.tab.next"), category: "Tab", default: "BracketRight", available: true, scope: "bare" },
  { id: "tab.prev", label: copyText("keybindingActions.tab.prev"), category: "Tab", default: "BracketLeft", available: true, scope: "bare" },
  { id: "tab.jump-1", label: copyText("keybindingActions.tab.jump1"), category: "Tab", default: "Digit1", available: true, scope: "bare" },
  { id: "tab.jump-2", label: copyText("keybindingActions.tab.jump2"), category: "Tab", default: "Digit2", available: true, scope: "bare" },
  { id: "tab.jump-3", label: copyText("keybindingActions.tab.jump3"), category: "Tab", default: "Digit3", available: true, scope: "bare" },
  { id: "tab.jump-4", label: copyText("keybindingActions.tab.jump4"), category: "Tab", default: "Digit4", available: true, scope: "bare" },
  { id: "tab.jump-5", label: copyText("keybindingActions.tab.jump5"), category: "Tab", default: "Digit5", available: true, scope: "bare" },
  { id: "tab.jump-6", label: copyText("keybindingActions.tab.jump6"), category: "Tab", default: "Digit6", available: true, scope: "bare" },
  { id: "tab.jump-7", label: copyText("keybindingActions.tab.jump7"), category: "Tab", default: "Digit7", available: true, scope: "bare" },
  { id: "tab.jump-8", label: copyText("keybindingActions.tab.jump8"), category: "Tab", default: "Digit8", available: true, scope: "bare" },
  { id: "tab.jump-9", label: copyText("keybindingActions.tab.jump9"), category: "Tab", default: "Digit9", available: true, scope: "bare" },
  { id: "tab.close-archived", label: copyText("sessionState.closeEnded.shortcut"), category: "Tab", default: "KeyW", available: true, scope: "bare" },
  { id: "tab.open-cwd", label: copyText("keybindingActions.tab.openCwd"), category: "Tab", default: "KeyE", available: true, scope: "bare" },
  { id: "tab.pop-out", label: copyText("keybindingActions.tab.openInWindow"), category: "Tab", default: "KeyN", available: true, scope: "bare" },
  // 在当前标签页上开右键菜单（会话头「更多」是同一份菜单）。菜单键是固定的另一个键。
  { id: "tab.context-menu", label: copyText("keybindingActions.tab.menu"), category: "Tab", default: "Shift+F10", available: true, scope: "nav", also: "ContextMenu" },
  // 跳到下一个需要你的会话（等得最久的在前）。带 Ctrl：任何时候都要能按到。
  { id: "needs.next", label: copyText("tabBar.needs.hint"), category: "Tab", default: "Ctrl+KeyJ", available: true, scope: "any" },
  // 会话内查找（大纲在同一块面板里）：只搜当前 tab 这一份会话；跨全部会话的全文搜索在历史页里。
  { id: "session.find", label: copyText("keybindingActions.session.find"), category: "Tab", default: "Ctrl+KeyF", available: true, scope: "any" },
  // 过程默认展开 / 折起：完成的轮把过程折成一行，这一键全部摊开 / 全部收回。
  { id: "session.toggle-process", label: copyText("keybindingActions.session.toggleProcess"), category: "Tab", default: "Ctrl+KeyO", available: true, scope: "any" },
  // 上 / 下一轮（轮次刻度的键盘那一半）· 回到底部。
  { id: "session.prev-turn", label: copyText("keybindingActions.session.prevTurn"), category: "Tab", default: "Alt+ArrowUp", available: true, scope: "main" },
  { id: "session.next-turn", label: copyText("keybindingActions.session.nextTurn"), category: "Tab", default: "Alt+ArrowDown", available: true, scope: "main" },
  { id: "session.to-bottom", label: copyText("keybindingActions.session.toBottom"), category: "Tab", default: "End", available: true, scope: "main" },

  // ===== Terminal =====
  { id: "terminal.bring-front", label: copyText("keybindingActions.terminal.front"), category: "Term", default: "Backquote", available: true, scope: "bare" },

  // ===== App =====
  { id: "app.open-settings", label: copyText("keybindingActions.app.settings"), category: "App", default: "Comma", available: true, scope: "bare" },
  { id: "app.toggle-history", label: copyText("keybindingActions.app.history"), category: "App", default: "KeyH", available: true, scope: "bare" },
  { id: "app.open-command-bar", label: copyText("keybindingActions.app.commandBar"), category: "App", default: "Ctrl+KeyK", available: true, scope: "any" },
  { id: "app.keys", label: copyText("keybindingActions.app.keys"), category: "App", default: "Shift+Slash", available: true, scope: "bare" },
  { id: "app.minimize", label: copyText("keybindingActions.app.minimize"), category: "App", default: "Ctrl+KeyM", available: true, scope: "any" },
  { id: "app.toggle-fullscreen", label: copyText("keybindingActions.app.fullscreen"), category: "App", default: "F11", available: true, scope: "any" },
  { id: "app.zoom-in", label: copyText("keybindingActions.app.zoomIn"), category: "App", default: "Ctrl+Equal", available: true, scope: "any" },
  { id: "app.zoom-out", label: copyText("keybindingActions.app.zoomOut"), category: "App", default: "Ctrl+Minus", available: true, scope: "any" },
  { id: "app.zoom-reset", label: copyText("keybindingActions.app.zoomReset"), category: "App", default: "Ctrl+Digit0", available: true, scope: "any" },
  { id: "app.toggle-tab-bar", label: copyText("keybindingActions.app.toggleTabBar"), category: "App", default: null, available: true, scope: "any" },
  { id: "app.open-cc-bus", label: copyText("keybindingActions.app.ccBus"), category: "App", default: null, available: true, scope: "any" },
  { id: "overlay.close", label: copyText("keybindingActions.app.closeOverlay"), category: "App", default: "Escape", available: true, scope: "any" },
  // 撤销刚才那一步（最近一条还开着的撤销提示）。输入框里的 Ctrl+Z 归输入框。
  { id: "app.undo", label: copyText("keybindingActions.app.undo"), category: "App", default: "Ctrl+KeyZ", available: true, scope: "idle" },

  // ===== 账号 =====
  // 默认不绑：键位表已经很满，这不是高频操作。想要的人自己在设置里绑。
  { id: "account.switch-default", label: copyText("keybindingActions.account.menu"), category: "Acct", default: null, available: true, scope: "any" },

  // ===== Behavior toggles =====
  { id: "behavior.toggle-auto-follow", label: copyText("keybindingActions.behavior.autoFollow"), category: "Beh", default: null, available: true, scope: "any" },
  { id: "behavior.toggle-bring-monitor", label: copyText("keybindingActions.behavior.autoFront"), category: "Beh", default: null, available: true, scope: "any" },

  // ===== Panel =====
  { id: "panel.toggle-tasks", label: copyText("keybindingActions.panel.tasks"), category: "Panel", default: "KeyT", available: true, scope: "bare" },
  { id: "panel.toggle-agents", label: copyText("keybindingActions.panel.agents"), category: "Panel", default: "KeyA", available: true, scope: "bare" },
  { id: "panel.toggle-terminal", label: copyText("keybindingActions.panel.terminal"), category: "Panel", default: "Ctrl+Backquote", available: true, scope: "any" },
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
  Tab: copyText("keybindingActions.category.tab"),
  Term: copyText("keybindingActions.category.terminal"),
  App: copyText("keybindingActions.category.app"),
  Beh: copyText("keybindingActions.category.behavior"),
  Panel: copyText("keybindingActions.category.panel"),
  Acct: copyText("keybindingActions.category.account"),
};

/**
 * 编辑器里各分组的**显示顺序**。放这里而不是 editor.ts 里手抄一份数组——漏加一个 Category 会让
 * 整组 action 在编辑器里**静默消失**（用户既看不到、也没法绑），而 TS 不会因为数组少一个成员报错。
 * `CATEGORY_LABEL` 是 `Record<Category,…>`（漏加会编译失败），这份用 vitest 锁死覆盖完整。
 */
export const CATEGORY_ORDER: ReadonlyArray<Category> = [
  "Tab",
  "Term",
  "App",
  "Acct",
  "Beh",
  "Panel",
];
