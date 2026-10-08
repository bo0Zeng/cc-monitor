/**
 * 通用组件总览（`kit.html?scene=…`）：用产品真组件、合成数据，照规范组件页的排法把各态并排画出来。
 *
 * - `kit-overview`：页内组件（按钮 · 输入 · 开关 · 分栏 · chip · 折叠 · 徽标 · 状态点 · 转圈 · 计量 · 错误条 · 空态）—— 只画产品真在用的组件。
 * - `kit-dialog`：「会打断什么」那一问 ＋ 危险确认 ＋ 填值框报错（各开一次，截最后那个）。
 * - `kit-float`：弹出菜单（子菜单展开 · 危险项 · 不可选）· 悬停提示 · toast 四种 ＋ 撤销 · 抽屉。
 * - `kit-detail`：［复制详情］各态（默认 · 已复制 · 复制失败就地展开 · 窄 · 没有详情）＋ 错误条 · 行内 · 表单对话框里的那一颗。
 * - `kit-detail-toast`：带详情的出错 toast（动作第二行）· 合流 ×N · 后端 ERROR 那条 · 没有详情的 toast 版式不变。
 */
import { button, setBusy, setDisabled, buttonRow } from "../../src/frontend/ui/kit/button";
import { field } from "../../src/frontend/ui/kit/field";
import { toggleSwitch, checkbox } from "../../src/frontend/ui/kit/switch";
import { tabs, segmented } from "../../src/frontend/ui/kit/tabs";
import { chip, setChipOpen } from "../../src/frontend/ui/kit/chip";
import { fold } from "../../src/frontend/ui/kit/fold";
import { select } from "../../src/frontend/ui/kit/select";
import { accountAvatarEl } from "../../src/frontend/ui/account-color";
import { banner } from "../../src/frontend/ui/kit/banner";
import { emptyState } from "../../src/frontend/ui/kit/empty";
import { countBadge, tag, kbd } from "../../src/frontend/ui/kit/badge";
import { copyText } from "../../src/frontend/ui/copy-table";
import { spinner } from "../../src/frontend/ui/kit/progress";
import { meter } from "../../src/frontend/ui/kit/meter";
import { statusDot, type DotState } from "../../src/frontend/ui/kit/status-dot";
import { icon } from "../../src/frontend/ui/kit/icon";
import { confirmDialog, askText } from "../../src/frontend/ui/kit/dialog";
import { confirmInterrupts } from "../../src/frontend/ui/kit/interrupts";
import { openMenu } from "../../src/frontend/ui/kit/menu";
import { attachTooltip } from "../../src/frontend/ui/kit/tooltip";
import { toast, undoToast } from "../../src/frontend/ui/kit/toast";
import { openDrawer } from "../../src/frontend/ui/kit/drawer";
import { copyDetailButton, sayWithDetail } from "../../src/frontend/ui/kit/detail";
import { formDialog } from "../../src/frontend/ui/kit/dialog";

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
    section("按钮", "C1", "g-cols6", [
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

  const acctOpts = [
    { value: "work", label: "work", note: "默认 · 5h 63%", lead: () => accountAvatarEl("work", { size: 16 }) },
    { value: "personal", label: "personal", note: "5h 12%", lead: () => accountAvatarEl("personal", { size: 16 }) },
    { value: "api", label: "api", enabled: false, why: "需登录", lead: () => accountAvatarEl("api", { size: 16 }) },
  ];
  const selDis = select({ label: "账号", options: acctOpts });
  selDis.setDisabled(true);
  document.body.append(
    section("下拉", "C3", "g-cols3", [
      ["收着（身份块 · 字 · 灰字）", select({ label: "账号", options: acctOpts }).el],
      ["没有身份块", select({ label: "agent", options: [{ value: "claude", label: "claude" }] }).el],
      ["禁用", selDis.el],
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
    section("chip · 徽标 · 键帽", "C6 · C17 · C18", "g-cols3", [
      ["状态栏", h("div", "g-bar")],
      ["徽标 · 标记 · 键帽", row(countBadge(3), countBadge(2, "warn"), countBadge(120), tag("远端"), tag("只读"), kbd("Ctrl+K"))],
    ]),
  );
  const bar = document.querySelector<HTMLElement>(".g-bar")!;
  bar.append(chip({ text: "需要你 2", tone: "warn", onClick: () => {} }), openChip, chip({ text: "恢复失败 ×3", tone: "error", onClick: () => {} }), chip({ text: "work 5h 63%" }));

  const foldBody = h("div", "g-fake", "Bash · ls -la · 0.4s");
  document.body.append(
    section("折叠块", "C9", "g-cols3", [
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
  document.body.append(
    section("状态点 · 图标 · 转圈 · 计量条", "V10 · V9 · C14 · C22", "g-cols4", [
      ["状态点八态", dotList],
      ["图标（常规 16 · 紧凑 14 · 空态 32）", row(icon("settings"), icon("history"), icon("grid"), icon("folder"), icon("command", "compact"), icon("search", "compact"), icon("empty", "empty"))],
      ["转圈", stage(row(spinner(), h("span", "g-fake", "连接 devbox…")))],
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
      ["警告条", banner("warn", "重启 cc-monitor 后生效", [button({ label: copyText("restartNow.bar.action"), size: "compact" })])],
      ["空态", stage(emptyState({ text: "无会话", hint: "终端里 ccm 启动后自动出现" }))],
    ]),
  );

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


/** 合成的一份复制详情（出错那一端写的那几行）。 */
const DETAIL = [
  "时刻：2026-10-08 14:32:07 +08:00",
  "机器：Linux x86_64 · 后端 p9k-flicker",
  "本机：cc-monitor 4.1.5 (p9k-flicker) · Linux x86_64",
  "命令：kill",
  "码：kill_failed",
  "原话：can't find window: orders-3:2",
].join("\n");

/** 剪贴板换成写得进 / 写不进的那一份（各态要真点出来）。 */
function clipboard(ok: boolean): void {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: () => (ok ? Promise.resolve() : Promise.reject(new Error("denied"))) },
  });
}

async function details(): Promise<void> {
  document.body.append(h("h1", "g-h1", "复制详情 · 各态"), h("p", "g-sub", "kit/detail.ts · 产品真组件 · 合成详情"));
  const idle = copyDetailButton("结束 orders-3 失败 · tmux 报错", DETAIL)!;
  const copied = copyDetailButton("结束 orders-3 失败 · tmux 报错", DETAIL)!;
  const failHost = h("div");
  failHost.style.width = "360px";
  const failed = copyDetailButton("结束 orders-3 失败 · tmux 报错", DETAIL)!;
  failHost.appendChild(failed);
  const narrow = copyDetailButton("结束 orders-3 失败 · tmux 报错", DETAIL, { iconOnly: true })!;
  const none = h("div", undefined, "端口限 1–65535（本地校验，没有详情 ⇒ 不出按钮）");
  none.style.color = "var(--error)";
  document.body.append(
    section("按钮", "条带 §5.1", "g-cols4", [
      ["默认", idle],
      ["已复制 · 1.5 s 回默认", copied],
      ["复制不了 · 就地展开全选", failHost],
      ["窄 · 只剩图标", row(narrow, copyDetailButton("x", "") ?? none)],
    ]),
  );
  const bannerHost = h("div");
  bannerHost.style.width = "100%";
  bannerHost.append(
    banner("error", "读取 orders-3 画面失败 · 无运行中的 tmux", [button({ label: "刷新", size: "compact" })], DETAIL),
  );
  const narrowBanner = h("div");
  narrowBanner.style.width = "340px";
  narrowBanner.append(banner("error", "读取 orders-3 画面失败 · 无运行中的 tmux", [button({ label: "刷新", size: "compact" })], DETAIL));
  const inline = h("div");
  inline.style.color = "var(--error)";
  sayWithDetail(inline, "读取 cc-bus 失败 · 后端报错", DETAIL);
  document.body.append(
    section("各面", "条带 §5.3", "g-cols2", [
      ["区块顶错误条 · 修法 · 复制详情", bannerHost],
      ["错误条 · 窄（按钮折到下一行）", narrowBanner],
      ["行内 · 红字句子后面直接跟", inline],
      ["没有详情的错误条 · 版式不变", banner("warn", "devbox 离线", [button({ label: "重连", size: "compact" })])],
    ]),
  );
  clipboard(true);
  copied.querySelector("button")!.click();
  await new Promise((r) => setTimeout(r, 20));
  clipboard(false);
  failed.querySelector("button")!.click();
  await new Promise((r) => setTimeout(r, 50));
}

async function detailDialog(): Promise<void> {
  document.body.append(h("h1", "g-h1", "复制详情 · 表单对话框里提交没成"), stage(fakeRows(8)));
  const body = h("div");
  body.append(field({ label: "名称", value: "devbox" }).root, field({ label: "地址", value: "devbox.lan" }).root);
  const handle = formDialog({
    title: "添加机器",
    action: "添加",
    body,
    submit: async () => ({ said: "添加 devbox 失败 · 磁盘满", detail: DETAIL }),
  });
  await new Promise((r) => setTimeout(r, 30));
  handle.submit();
  await new Promise((r) => setTimeout(r, 50));
}

async function detailToasts(): Promise<void> {
  document.body.append(h("h1", "g-h1", "复制详情 · toast"), stage(fakeRows(12)));
  clipboard(true);
  toast("结束 orders 失败 · tmux 报错", "devbox", { detail: DETAIL, action: { label: "重试", run: () => {} } });
  toast("读取 cc-bus 无应答", "devbox", { detail: DETAIL });
  toast("读取 cc-bus 无应答", "devbox", { detail: DETAIL });
  undoToast("已删除 devbox", () => {}, () => {});
  await new Promise((r) => setTimeout(r, 300));
}

const scene = new URLSearchParams(location.search).get("scene") ?? "kit-overview";
const run: Record<string, () => void | Promise<void>> = {
  "kit-overview": overview,
  "kit-dialog": dialogs,
  "kit-dialog-text": dialogText,
  "kit-dialog-danger": dialogDanger,
  "kit-float": floats,
  "kit-detail": details,
  "kit-detail-dialog": detailDialog,
  "kit-detail-toast": detailToasts,
};
window.__shots = { state: "booting", error: null, unhandled: [], layout: [] } satisfies ShotsHandle;
Promise.resolve(run[scene]?.())
  .then(() => (window.__shots!.state = "done"))
  .catch((e: unknown) => {
    window.__shots!.state = "failed";
    window.__shots!.error = String(e);
  });
