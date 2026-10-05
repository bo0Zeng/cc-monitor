/**
 * 通用组件总览（`kit.html?scene=…`）：用产品真组件、合成数据，照规范组件页的排法把各态并排画出来。
 *
 * - `kit-overview`：页内组件（按钮 · 输入 · 开关 · 分栏 · chip · 卡片 · 列表行 · 折叠 · 徽标 · 状态点 · 进度 · 计量 · 错误条 · 空态 · 区块七态）。
 * - `kit-dialog`：「会打断什么」那一问 ＋ 危险确认 ＋ 填值框报错（各开一次，截最后那个）。
 * - `kit-float`：弹出菜单（子菜单展开 · 危险项 · 不可选）· 悬停提示 · toast 四种 ＋ 撤销 · 抽屉。
 */
import { button, setBusy, setDisabled, toggleButton, buttonRow } from "../../src/frontend/ui/kit/button";
import { field } from "../../src/frontend/ui/kit/field";
import { toggleSwitch, checkbox } from "../../src/frontend/ui/kit/switch";
import { tabs, segmented } from "../../src/frontend/ui/kit/tabs";
import { chip, connectionPill, setChipOpen } from "../../src/frontend/ui/kit/chip";
import { card } from "../../src/frontend/ui/kit/card";
import { listRow, setRowState } from "../../src/frontend/ui/kit/list-row";
import { fold } from "../../src/frontend/ui/kit/fold";
import { banner } from "../../src/frontend/ui/kit/banner";
import { emptyState, noMatch } from "../../src/frontend/ui/kit/empty";
import { countBadge, tag, kbd } from "../../src/frontend/ui/kit/badge";
import { skeletonRows } from "../../src/frontend/ui/kit/skeleton";
import { progressBar, spinner } from "../../src/frontend/ui/kit/progress";
import { meter } from "../../src/frontend/ui/kit/meter";
import { statusDot, type DotState } from "../../src/frontend/ui/kit/status-dot";
import { dataBlock, type BlockState } from "../../src/frontend/ui/kit/block";
import { icon } from "../../src/frontend/ui/kit/icon";
import { confirmDialog, askText } from "../../src/frontend/ui/kit/dialog";
import { confirmInterrupts } from "../../src/frontend/ui/kit/interrupts";
import { openMenu } from "../../src/frontend/ui/kit/menu";
import { attachTooltip } from "../../src/frontend/ui/kit/tooltip";
import { toast, undoToast } from "../../src/frontend/ui/kit/toast";
import { openDrawer } from "../../src/frontend/ui/kit/drawer";

import type { ShotsHandle } from "./fake/types";

const h = <K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
};

function section(title: string, rule: string, cols: string, cells: [string, HTMLElement][]): HTMLElement {
  const s = h("section", "g-sec");
  const t = h("h2", undefined, title);
  t.appendChild(h("span", undefined, rule));
  const grid = h("div", `g-grid ${cols}`);
  for (const [cap, el] of cells) {
    const c = h("div", "g-cell");
    c.append(h("div", "g-cap", cap), el);
    grid.appendChild(c);
  }
  s.append(t, grid);
  return s;
}

const row = (...els: (Element | null)[]): HTMLElement => {
  const r = h("div", "g-row");
  for (const e of els) if (e) r.appendChild(e);
  return r;
};
const stage = (...els: HTMLElement[]): HTMLElement => {
  const s = h("div", "g-stage");
  s.append(...els);
  return s;
};
const fakeRows = (n = 3): HTMLElement => {
  const d = h("div", "g-fake");
  for (let i = 0; i < n; i++) d.appendChild(h("i"));
  return d;
};

function overview(): void {
  document.body.append(h("h1", "g-h1", "通用组件 · 各态"), h("p", "g-sub", "产品真组件 · 真令牌 · 合成数据（src/frontend/ui/kit/）"));

  const busy = button({ label: "保存", kind: "primary" });
  const dis = button({ label: "卸载", hint: "从这台卸载" });
  setDisabled(dis, "devbox 离线");
  const focus = button({ label: "重试" });
  focus.dataset.focusDemo = "1";
  document.body.append(
    section("按钮", "C1 · C21", "g-cols6", [
      ["主", button({ label: "保存", kind: "primary" })],
      ["次", button({ label: "取消" })],
      ["危险（只在确认框）", button({ label: "结束", kind: "danger" })],
      ["危险文字（入口）", button({ label: "删除这台机器", kind: "danger-text" })],
      ["幽灵", button({ label: "详情", kind: "ghost" })],
      ["图标", row(button({ label: "刷新", kind: "icon", icon: "info", hint: "刷新" }), button({ label: "关闭", kind: "icon", icon: "close", size: "compact", hint: "关闭" }))],
      ["紧凑", row(button({ label: "重新连接", size: "compact" }), button({ label: "撤销", kind: "ghost", size: "compact" }))],
      ["禁用（悬停说为什么）", dis],
      ["进行中", busy],
      ["键盘焦点", focus],
      ["切换按钮 开 / 关", row(toggleButton({ label: "隐藏文件", icon: "folder", pressed: true, onToggle: () => {} }), toggleButton({ label: "双栏", icon: "grid", pressed: false, onToggle: () => {} }))],
      ["按钮行", buttonRow(button({ label: "取消" }), button({ label: "结束会话", kind: "danger" }))],
    ]),
  );
  setBusy(busy, "正在保存");

  const fErr = field({ label: "端口", value: "70000", help: "1–65535" });
  fErr.setError("端口越界 · 1–65535");
  const fVal = field({ label: "主机", value: "devbox.lan" });
  fVal.setValidating(true);
  const fRo = field({ label: "数据目录", value: "~/.cc-monitor" });
  fRo.setReadonly(true);
  const fDis = field({ label: "用户", value: "user" });
  fDis.setDisabled("devbox 离线");
  document.body.append(
    section("输入框", "C2", "g-cols6", [
      ["常态（说明在下）", field({ label: "名称", placeholder: "devbox", help: "列表里显示的名字" }).root],
      ["带前缀", field({ label: "路径", prefix: "~/", value: "notes.txt" }).root],
      ["出错（替换说明）", fErr.root],
      ["校验中", fVal.root],
      ["只读", fRo.root],
      ["禁用", fDis.root],
    ]),
  );

  const swBusy = toggleSwitch({ label: "自动跟随", on: false, onChange: () => new Promise<boolean>(() => {}) });
  document.body.append(
    section("开关 · 复选 · 分栏 · 分段", "C4 · C5 · C23", "g-cols4", [
      ["开关 开 / 关", stage(toggleSwitch({ label: "自动切到前台", on: true, onChange: () => true }).root, toggleSwitch({ label: "显示分身会话", on: false, onChange: () => true }).root)],
      ["等后端答复", stage(swBusy.root)],
      ["复选框（勾一批）", stage(checkbox("orders", true, () => {}), checkbox("build", false, () => {}))],
      ["分段按钮", segmented({ label: "范围", items: [{ key: "d", label: "默认" }, { key: "s", label: "本会话" }], current: "s", onChange: () => {} })],
      ["页内分栏", tabs({ label: "机器", items: [{ key: "c", label: "连接" }, { key: "a", label: "账号" }, { key: "t", label: "终端" }], current: "a", onChange: () => {} })],
    ]),
  );
  swBusy.input.click();

  const openChip = chip({ text: "任务 3/12", icon: "check", onClick: () => {} });
  setChipOpen(openChip, true);
  document.body.append(
    section("chip · 连接药丸 · 徽标 · 键帽", "C6 · C17 · C18", "g-cols3", [
      ["状态栏", h("div", "g-bar")],
      ["连接药丸：离线 · 重连中 · 已连接（连着时不画）", row(connectionPill("devbox", "down", () => {}), connectionPill("gpu-01", "connecting", () => {}), connectionPill("devbox", "restored", () => {}))],
      ["徽标 · 标记 · 键帽", row(countBadge(3), countBadge(2, "warn"), countBadge(120), tag("远端"), tag("只读"), kbd("Ctrl+K"))],
    ]),
  );
  const bar = document.querySelector<HTMLElement>(".g-bar")!;
  bar.append(chip({ text: "需要你 2", tone: "warn", onClick: () => {} }), openChip, chip({ text: "恢复失败 ×3", tone: "error", onClick: () => {} }), chip({ text: "work 5h 63%" }));

  const rowSel = listRow({ name: "orders-service", icon: "folder", meta: "12:04" });
  setRowState(rowSel, { selected: true });
  const rowCur = listRow({ name: "build-pipeline", icon: "folder", meta: "11:58", actions: [button({ label: "复制", kind: "icon", icon: "check", size: "compact", hint: "复制路径" })] });
  setRowState(rowCur, { current: true });
  const rowLong = listRow({ name: `very-long-name-of-a-report-${"x".repeat(60)}-final.tar.gz`, icon: "check", meta: "3.2 MB" });
  const foldBody = h("div", "g-fake", "Bash · ls -la · 0.4s");
  document.body.append(
    section("卡片 · 列表行 · 折叠块", "C7 · C8 · C9", "g-cols3", [
      ["卡片 · 需要你", stage(card({ title: "计划待批", needsYou: true, lead: statusDot("needs-you", "需要你"), actions: [button({ label: "详情", kind: "ghost", size: "compact" })], body: "3 步 · 改 2 个文件" }), card({ title: "子 agent", body: "Explore · 完成 · 2m" }))],
      ["列表行：常态 · 选中 · 当前（行尾动作悬停才出）· 中间省略", stage(listRow({ name: "notes.md", icon: "check", meta: "4 KB" }), rowSel, rowCur, rowLong)],
      ["折叠块 收 / 开", stage(fold({ title: "过程", summary: "工具 ×3 · 21:14", open: false, body: h("div", "g-fake", "…") }), fold({ title: "思考", summary: "214 字", open: true, body: foldBody }))],
    ]),
  );

  const dots: [DotState, string][] = [
    ["running", "运行中"],
    ["needs-you", "需要你"],
    ["idle", "空闲"],
    ["exited", "Claude 已退出"],
    ["ended", "已结束"],
    ["unknown", "说不清"],
    ["gone", "记录已不在"],
    ["failed", "出错"],
  ];
  const dotList = h("div");
  for (const [st, label] of dots) dotList.appendChild(row(statusDot(st, label), h("span", "g-fake", label)));
  const prog = progressBar(0.32, "3.2 / 10 MB");
  document.body.append(
    section("状态点 · 图标 · 进度 · 计量条", "V10 · V9 · C14 · C22", "g-cols4", [
      ["状态点八态", dotList],
      ["图标（常规 16 · 紧凑 14 · 空态 32）", row(icon("settings"), icon("history"), icon("grid"), icon("folder"), icon("keyboard", "compact"), icon("search", "compact"), icon("empty", "empty"))],
      ["进度条 · 转圈", stage(prog.root, row(spinner(), h("span", "g-fake", "连接 devbox…")))],
      [
        "计量条：常态 · 到阈值 · 被拒 · 数旧 · 无采样",
        stage(
          meter({ label: "5h", ratio: 0.42, state: "normal", value: "42%", reset: "↻18:30" }),
          meter({ label: "7d", ratio: 0.86, state: "near", value: "86%", reset: "↻周一" }),
          meter({ label: "5h", ratio: 1, state: "refused", value: "104%", reset: "↻18:30" }),
          meter({ label: "7d", ratio: 0.5, state: "stale", value: "50%", reset: "↻周一" }),
          meter({ label: "5h", ratio: 0, state: "none", value: "—" }),
        ),
      ],
    ]),
  );

  document.body.append(
    section("错误条 · 警告条 · 空态", "C15 · C16", "g-cols4", [
      ["错误条", banner("error", "读取账号失败 · 内容无法解析", [button({ label: "复制详情", size: "compact" })])],
      ["警告条", banner("warn", "重启 cc-monitor 后生效", [button({ label: "现在重启", size: "compact" })])],
      ["空态", stage(emptyState({ text: "无会话", hint: "终端里 ccm 启动后自动出现" }))],
      ["筛选无结果", stage(noMatch("ordrs", () => {}))],
    ]),
  );

  const states: [string, BlockState | "loading-skel"][] = [
    ["加载（>300ms 骨架）", "loading-skel"],
    ["空", { kind: "empty", empty: { text: "无任务" } }],
    ["出错", { kind: "error", text: "读取失败 · devbox 离线", retry: () => {} }],
    ["过期", { kind: "stale", text: "devbox 离线 · 采样 3m 前", retry: () => {}, content: fakeRows(2) }],
    ["部分", { kind: "partial", content: fakeRows(2), missing: ["gpu-01 无应答", "win-laptop 读取中"] }],
    ["很多", { kind: "many", content: fakeRows(2), note: "前 500 · 搜索可找全部" }],
    ["禁用", { kind: "disabled", why: "devbox 离线", content: fakeRows(2) }],
    ["刷新中（内容不动）", { kind: "ready", content: fakeRows(3) }],
  ];
  const cells: [string, HTMLElement][] = states.map(([cap, st]) => {
    const b = dataBlock();
    if (st === "loading-skel") {
      b.root.dataset.state = "loading";
      b.root.firstElementChild!.appendChild(skeletonRows(3));
    } else b.show(st);
    if (cap.startsWith("刷新")) b.refreshing(true);
    return [cap, stage(b.root)];
  });
  document.body.append(section("读数据的区块：七态", "I5", "g-cols4", cells));
  focus.focus();
}

async function dialogs(): Promise<void> {
  document.body.append(h("h1", "g-h1", "对话框 · 「会打断什么」"), stage(fakeRows(8)));
  void confirmInterrupts({
    title: "重启切换 → work",
    action: "重启切换",
    keep: ["会话记录 · 终端 orders-cc"],
    ask: async () => ({ families: [{ family: "turn", names: [] }, { family: "agent", names: ["Explore", "Plan"] }, { family: "task", names: ["写判据"] }] }),
  });
  await new Promise((r) => setTimeout(r, 50));
}

async function dialogText(): Promise<void> {
  document.body.append(h("h1", "g-h1", "对话框 · 填值（错误在框里）"), stage(fakeRows(8)));
  void askText({ title: "新建集合", action: "新建", label: "集合名", validate: (v) => (v.trim() === "" ? "名字为空" : null) });
  await new Promise((r) => setTimeout(r, 30));
  document.querySelector<HTMLButtonElement>('[aria-modal="true"] button:last-child')?.click();
}

async function dialogDanger(): Promise<void> {
  document.body.append(h("h1", "g-h1", "对话框 · 撤不回的一批"), stage(fakeRows(8)));
  void confirmDialog({
    title: "结束 12 个会话",
    action: "结束 12 个",
    danger: true,
    rows: [
      { label: "中断", items: ["当前轮次 ×3"] },
      { label: "保留", items: ["会话记录 · 可恢复"] },
    ],
    list: Array.from({ length: 12 }, (_, i) => `· session-${i + 1}（devbox）`),
  });
  await new Promise((r) => setTimeout(r, 30));
}

async function floats(): Promise<void> {
  document.body.append(h("h1", "g-h1", "浮层：菜单 · 悬停提示 · toast · 抽屉"), stage(fakeRows(12)));
  const tipHost = button({ label: "刷新", kind: "icon", icon: "info" });
  tipHost.style.position = "fixed";
  tipHost.style.left = "40px";
  tipHost.style.top = "420px";
  document.body.appendChild(tipHost);
  attachTooltip(tipHost, "刷新 · 重读这台的会话\nCtrl+R", { immediate: true });
  tipHost.dispatchEvent(new MouseEvent("mouseenter"));
  openMenu({ x: 40, y: 80 }, [
    { label: "在新窗口打开", icon: "grid", detail: "Ctrl+Enter" },
    { label: "恢复", submenu: [{ label: "work", checked: true, detail: "a@x" }, { label: "home", checked: false, detail: "b@y" }] },
    { label: "预览", enabled: false, title: "devbox 要更新" },
    { label: "", divider: true },
    { label: "结束会话", danger: true },
  ]);
  const wrap = document.querySelector<HTMLElement>('body > [role="menu"] [role="none"]');
  wrap?.dispatchEvent(new MouseEvent("mouseenter"));
  toast("已复制", "", { level: "success" });
  toast("远端管道拥塞", "box1 丢行", { level: "info" });
  toast("远端管道拥塞", "box3 丢行", { level: "info" });
  undoToast("已删除 devbox", () => {}, () => {});
  toast("恢复失败 · orders", "devbox 离线", { level: "error", action: { label: "重试", run: () => {} } });
  const body = h("div");
  body.append(field({ label: "名称", value: "work" }).root, field({ label: "默认模型", value: "opus" }).root);
  openDrawer({ title: "账号 work", body, width: 400 });
  await new Promise((r) => setTimeout(r, 300));
}

const scene = new URLSearchParams(location.search).get("scene") ?? "kit-overview";
const run: Record<string, () => void | Promise<void>> = {
  "kit-overview": overview,
  "kit-dialog": dialogs,
  "kit-dialog-text": dialogText,
  "kit-dialog-danger": dialogDanger,
  "kit-float": floats,
};
window.__shots = { state: "booting", error: null, unhandled: [] } satisfies ShotsHandle;
Promise.resolve(run[scene]?.())
  .then(() => (window.__shots!.state = "done"))
  .catch((e: unknown) => {
    window.__shots!.state = "failed";
    window.__shots!.error = String(e);
  });
