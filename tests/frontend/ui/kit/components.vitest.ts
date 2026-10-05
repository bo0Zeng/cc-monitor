/**
 * 通用组件各态（规范 C1–C23 · V10 · I5）：每件每一态画出来的形状（角色 · data-* · aria-* · 字），与该态的行为。
 * 样子（颜色 · 尺寸）由 CSS Modules 按这些态上，截图那一页（`tests/shots` 的组件总览）看。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { button, setBusy, setDisabled, toggleButton, buttonRow } from "../../../../src/frontend/ui/kit/button";
import { field } from "../../../../src/frontend/ui/kit/field";
import { toggleSwitch, checkbox } from "../../../../src/frontend/ui/kit/switch";
import { tabs, segmented } from "../../../../src/frontend/ui/kit/tabs";
import { chip, connectionPill, setChipOpen } from "../../../../src/frontend/ui/kit/chip";
import { card } from "../../../../src/frontend/ui/kit/card";
import { listRow, setRowState, middleEllipsis } from "../../../../src/frontend/ui/kit/list-row";
import { fold } from "../../../../src/frontend/ui/kit/fold";
import { banner } from "../../../../src/frontend/ui/kit/banner";
import { emptyState, noMatch } from "../../../../src/frontend/ui/kit/empty";
import { countBadge, tag, kbd } from "../../../../src/frontend/ui/kit/badge";
import { skeletonRows } from "../../../../src/frontend/ui/kit/skeleton";
import { progressBar, spinner } from "../../../../src/frontend/ui/kit/progress";
import { meter } from "../../../../src/frontend/ui/kit/meter";
import { statusDot, setDot } from "../../../../src/frontend/ui/kit/status-dot";
import { dataBlock, BLOCK_SKELETON_AFTER_MS, BLOCK_SAY_DOING_AFTER_MS } from "../../../../src/frontend/ui/kit/block";
import { icon } from "../../../../src/frontend/ui/kit/icon";
import { copyText } from "../../../../src/frontend/ui/copy-table";

beforeEach(() => document.body.replaceChildren());

describe("C1 按钮 · C21 切换按钮", () => {
  it("层级落在 data-kind；图标按钮的字进读屏名、不上屏", () => {
    for (const kind of ["primary", "secondary", "danger", "danger-text", "ghost"] as const) {
      const b = button({ label: "结束", kind });
      expect([b.dataset.kind, b.textContent, b.type]).toEqual([kind, "结束", "button"]);
    }
    const ib = button({ label: "关闭", kind: "icon", icon: "close", hint: "关闭" });
    expect([ib.textContent, ib.getAttribute("aria-label"), ib.title, ib.querySelector("svg")?.dataset.icon]).toEqual(["", "关闭", "关闭", "close"]);
    expect(button({ label: "x", size: "compact" }).dataset.size).toBe("compact");
  });

  it("禁用：aria-disabled ＋ 悬停说为什么；点了不做；解禁还原原提示", () => {
    const run = vi.fn();
    const b = button({ label: "卸载", hint: "从这台卸载", onClick: run });
    setDisabled(b, "devbox 离线");
    expect([b.getAttribute("aria-disabled"), b.title]).toEqual(["true", "devbox 离线"]);
    b.click();
    expect(run).not.toHaveBeenCalled();
    setDisabled(b, null);
    expect([b.hasAttribute("aria-disabled"), b.title]).toEqual([false, "从这台卸载"]);
    b.click();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("进行中：字换成「正在…」＋ 转圈，不可再点；还原后字回来", () => {
    const run = vi.fn();
    const b = button({ label: "保存", onClick: run });
    setBusy(b, "正在保存");
    expect([b.dataset.busy, b.textContent, b.querySelectorAll('[data-role="spin"]').length]).toEqual(["true", "正在保存", 1]);
    b.click();
    expect(run).not.toHaveBeenCalled();
    setBusy(b, null);
    expect([b.dataset.busy, b.textContent]).toEqual([undefined, "保存"]);
  });

  it("切换按钮：aria-pressed 跟着翻，回调拿到翻后的值", () => {
    const seen: boolean[] = [];
    const t = toggleButton({ label: "显示隐藏文件", pressed: false, onToggle: (p) => seen.push(p) });
    t.click();
    t.click();
    expect([seen, t.getAttribute("aria-pressed")]).toEqual([[true, false], "false"]);
  });

  it("按钮行：按给的先后摆（取消在左、确认在右）", () => {
    const row = buttonRow(button({ label: "取消" }), button({ label: "结束", kind: "danger" }));
    expect([...row.children].map((c) => c.textContent)).toEqual(["取消", "结束"]);
  });
});

describe("C2 输入框", () => {
  it("标签连着框；说明在下；出错替换说明的位置、带图标、aria-invalid；消掉回到说明", () => {
    const f = field({ label: "端口", value: "22", help: "1–65535" });
    document.body.appendChild(f.root);
    const note = f.root.lastElementChild as HTMLElement;
    expect(f.root.querySelector("label")!.htmlFor).toBe(f.input.id);
    expect(note.textContent).toBe("1–65535");
    f.setError("端口越界");
    expect([f.root.dataset.error, f.input.getAttribute("aria-invalid"), note.textContent, note.querySelectorAll("svg").length]).toEqual([
      "true",
      "true",
      "端口越界",
      1,
    ]);
    expect(f.root.children.length, "错误句另起了一行（跳位）").toBe(3);
    f.setError(null);
    expect([f.root.dataset.error, note.textContent]).toEqual([undefined, "1–65535"]);
  });

  it("校验中转圈 · 只读 · 禁用（说为什么）· 多行 · 前缀", () => {
    const f = field({ label: "路径", prefix: "~/", multiline: true });
    f.setValidating(true);
    expect(f.root.querySelectorAll("span[aria-hidden]").length).toBe(1);
    f.setValidating(false);
    f.setReadonly(true);
    f.setDisabled("devbox 离线");
    expect([f.input.tagName, f.root.dataset.readonly, (f.input as HTMLTextAreaElement).readOnly, f.input.disabled]).toEqual([
      "TEXTAREA",
      "true",
      true,
      true,
    ]);
    expect(f.root.textContent).toContain("~/");
  });
});

describe("C4 开关 · 复选框", () => {
  it("立刻生效：拨了就翻；回 false ⇒ 退回原位", () => {
    const s = toggleSwitch({ label: "自动跟随", on: false, onChange: (on) => on !== true });
    s.input.click();
    expect(s.input.getAttribute("aria-checked"), "后端拒了没退回").toBe("false");
    const t = toggleSwitch({ label: "x", on: false, onChange: () => true });
    t.input.click();
    expect(t.input.getAttribute("aria-checked")).toBe("true");
  });

  it("等后端答复：拇指先动、旁边转圈、不能再拨；拒了退回、转圈收掉", async () => {
    let settle!: (v: boolean) => void;
    const s = toggleSwitch({ label: "x", on: true, onChange: () => new Promise<boolean>((r) => (settle = r)) });
    s.input.click();
    expect([s.input.getAttribute("aria-checked"), s.input.dataset.busy, s.root.querySelectorAll("span[aria-hidden]").length]).toEqual([
      "false",
      "true",
      1,
    ]);
    s.input.click(); // 等着时再拨不算
    settle(false);
    await Promise.resolve();
    await Promise.resolve();
    expect([s.input.getAttribute("aria-checked"), s.input.dataset.busy, s.root.querySelectorAll("span[aria-hidden]").length]).toEqual([
      "true",
      undefined,
      0,
    ]);
  });

  it("复选框：勾了报勾没勾", () => {
    const seen: boolean[] = [];
    const c = checkbox("orders", false, (v) => seen.push(v));
    document.body.appendChild(c);
    c.querySelector("input")!.click();
    expect(seen).toEqual([true]);
  });
});

describe("C5 分栏 · C23 分段按钮", () => {
  it("当前那一个 aria-selected / aria-checked、只有它进 Tab 顺序；←→ Home End 换并报出", () => {
    const seen: string[] = [];
    const t = tabs({ label: "机器", items: [{ key: "a", label: "连接" }, { key: "b", label: "账号" }, { key: "c", label: "终端" }], current: "a", onChange: (k) => seen.push(k) });
    document.body.appendChild(t);
    const items = [...t.children] as HTMLButtonElement[];
    expect(items.map((b) => [b.getAttribute("aria-selected"), b.tabIndex])).toEqual([["true", 0], ["false", -1], ["false", -1]]);
    const key = (k: string): boolean => t.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
    key("ArrowRight");
    key("End");
    key("ArrowRight");
    key("Home");
    expect(seen).toEqual(["b", "c", "a"]);
    expect(t.getAttribute("role")).toBe("tablist");
    const s = segmented({ label: "范围", items: [{ key: "d", label: "默认" }, { key: "s", label: "本会话" }], current: "s", onChange: () => {} });
    expect([s.getAttribute("role"), (s.children[1] as HTMLElement).getAttribute("aria-checked")]).toEqual(["radiogroup", "true"]);
  });
});

describe("C6 chip · 连接药丸", () => {
  it("可点的是按钮、只读的是 span；有事才上色；开着浮层 aria-expanded", () => {
    const c = chip({ text: "需要你 2", tone: "warn", onClick: () => {} });
    expect([c.tagName, c.dataset.intent]).toEqual(["BUTTON", "warn"]);
    setChipOpen(c, true);
    expect(c.getAttribute("aria-expanded")).toBe("true");
    expect(chip({ text: "5h 63%" }).tagName).toBe("SPAN");
  });

  it("连着时不画；断了警示可点即重连；重连中转圈；连回来成功色", () => {
    const re = vi.fn();
    expect(connectionPill("devbox", "up", re)).toBeNull();
    const down = connectionPill("devbox", "down", re)!;
    expect([down.textContent, down.dataset.intent]).toEqual([copyText("kit.pill.offline", { machine: "devbox" }), "warn"]);
    down.click();
    expect(re).toHaveBeenCalledTimes(1);
    expect(connectionPill("devbox", "connecting", re)!.querySelectorAll("span[aria-hidden]").length).toBe(1);
    expect(connectionPill("devbox", "restored", re)!.dataset.intent).toBe("success");
  });
});

describe("C7 卡片 · C8 列表行 · C9 折叠块", () => {
  it("卡片：需要你 ⇒ data-needs-you（左条由 CSS 画），标题 ＋ 右侧动作", () => {
    const c = card({ title: "计划待批", needsYou: true, actions: [button({ label: "详情", kind: "ghost" })], body: "3 步" });
    expect([c.dataset.needsYou, c.textContent]).toEqual(["true", "计划待批详情3 步"]);
  });

  it("列表行：选中与当前分开；长名字中间省略、全名在悬停；Enter / 双击打开", () => {
    const open = vi.fn();
    const r = listRow({ name: `${"a".repeat(80)}.tar.gz`, meta: "12 MB", onOpen: open, actions: [button({ label: "复制", kind: "ghost" })] });
    setRowState(r, { selected: true });
    setRowState(r, { current: true });
    expect([r.getAttribute("aria-selected"), r.getAttribute("aria-current")]).toEqual(["true", "true"]);
    setRowState(r, { current: false });
    expect(r.hasAttribute("aria-current")).toBe(false);
    const name = r.querySelector<HTMLElement>("[title]")!;
    expect(name.title.endsWith(".tar.gz") && name.textContent!.endsWith(".tar.gz") && name.textContent!.includes("…")).toBe(true);
    r.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    r.dispatchEvent(new MouseEvent("dblclick"));
    expect(open).toHaveBeenCalledTimes(2);
    expect(middleEllipsis("short.txt", 60)).toBe("short.txt");
  });

  it("折叠块：aria-expanded 与正文 hidden 同进退；→ 开 ← 收；回调只在真变了时调", () => {
    const seen: boolean[] = [];
    const f = fold({ title: "工具 ×3", summary: "21:14", open: false, body: document.createElement("div"), onToggle: (o) => seen.push(o) });
    const head = f.querySelector("button")!;
    const body = f.lastElementChild as HTMLElement;
    expect([head.getAttribute("aria-expanded"), body.hidden]).toEqual(["false", true]);
    head.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    head.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    head.click();
    expect([seen, head.getAttribute("aria-expanded"), body.hidden]).toEqual([[true, false], "false", true]);
  });
});

describe("C14 进度 · C22 计量条 · C17 徽标 · C18 键帽 · C19 骨架 · V10 状态点", () => {
  it("进度条：比例夹在 0–1、读数原样、aria-valuenow 跟着变", () => {
    const p = progressBar(0.32, "3.2 / 10 MB");
    p.set(1.5, "10 / 10 MB");
    expect([p.root.getAttribute("aria-valuenow"), p.root.textContent]).toEqual(["100", "10 / 10 MB"]);
    expect(spinner().getAttribute("aria-hidden")).toBe("true");
  });

  it("计量条五态落在 data-state；无采样不画填充", () => {
    for (const st of ["normal", "near", "refused", "stale", "none"] as const) {
      const m = meter({ label: "5h", ratio: 0.63, state: st, value: "63%", reset: "↻18:30" });
      expect(m.dataset.state).toBe(st);
      const fill = m.querySelector<HTMLElement>("span > span")!;
      expect(fill.style.transform === "", st).toBe(st === "none");
    }
  });

  it("计数 0 不画、99 以上写 99+；需要你的计数琥珀；标记与键帽", () => {
    expect(countBadge(0)).toBeNull();
    expect(countBadge(120)!.textContent).toBe("99+");
    expect(countBadge(2, "warn")!.dataset.intent).toBe("warn");
    expect([tag("远端").textContent, kbd("Ctrl+K").tagName]).toEqual(["远端", "KBD"]);
  });

  it("骨架：3–5 行、读屏器不念", () => {
    expect([skeletonRows(1).children.length, skeletonRows(9).children.length, skeletonRows().getAttribute("aria-hidden")]).toEqual([3, 5, "true"]);
  });

  it("状态点：八态各一形，悬停与读屏名写人话", () => {
    const d = statusDot("running", "运行中");
    expect([d.dataset.state, d.title, d.getAttribute("aria-label"), d.getAttribute("role")]).toEqual(["running", "运行中", "运行中", "img"]);
    setDot(d, "unknown", "说不清");
    expect([d.dataset.state, d.title]).toEqual(["unknown", "说不清"]);
    expect(statusDot("failed", "失败", "compact").dataset.size).toBe("compact");
  });

  it("图标：纯装饰不念、三档尺寸", () => {
    const i = icon("info", "empty");
    expect([i.getAttribute("aria-hidden"), i.dataset.size, i.querySelectorAll("path").length]).toEqual(["true", "empty", 1]);
  });
});

describe("C15 错误条 · C16 空态", () => {
  it("错误条 role=alert、警告条 role=status；不带 ×；动作在右", () => {
    const e = banner("error", "读取账号失败 · 内容无法解析", [button({ label: "复制详情" })]);
    expect([e.getAttribute("role"), e.dataset.intent, e.lastElementChild?.textContent]).toEqual(["alert", "error", "复制详情"]);
    expect(banner("warn", "重启 cc-monitor 后生效").getAttribute("role")).toBe("status");
  });

  it("空态：图标 ＋ 一句 ＋ 怎么让它有 ＋ 至多一颗按钮；筛选空另一句 ＋［清除过滤］", () => {
    const e = emptyState({ text: "无会话", hint: "终端里 ccm 启动后自动出现" });
    expect(e.textContent).toBe("无会话终端里 ccm 启动后自动出现");
    const clear = vi.fn();
    const n = noMatch("ordrs", clear);
    expect(n.textContent).toContain(copyText("kit.empty.noMatch", { query: "ordrs" }));
    n.querySelector("button")!.click();
    expect(clear).toHaveBeenCalled();
  });
});

describe("I5 读数据的区块：七态", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("加载：300ms 内什么都不画 ⇒ 骨架 ⇒ 10s 后写正在做什么；数据到了原位替换", () => {
    const b = dataBlock();
    b.show({ kind: "loading", doing: "连接 devbox" });
    expect([b.root.dataset.state, b.root.textContent]).toEqual(["loading", ""]);
    vi.advanceTimersByTime(BLOCK_SKELETON_AFTER_MS);
    expect(b.root.querySelectorAll('[aria-hidden="true"]').length).toBe(1);
    vi.advanceTimersByTime(BLOCK_SAY_DOING_AFTER_MS);
    expect(b.root.textContent).toContain("连接 devbox");
    const content = document.createElement("div");
    content.textContent = "行";
    b.show({ kind: "ready", content });
    vi.advanceTimersByTime(BLOCK_SAY_DOING_AFTER_MS * 2);
    expect(b.root.textContent, "换了态，上一态的计时还在画").toBe("行");
  });

  it("出错 ≠ 空：出错是错误条 ＋［重试］；空是空态", () => {
    const b = dataBlock();
    const retry = vi.fn();
    b.show({ kind: "error", text: "读取失败 · devbox 离线", retry });
    expect(b.root.querySelector('[role="alert"]')?.textContent).toContain("读取失败");
    b.root.querySelector("button")!.click();
    expect(retry).toHaveBeenCalled();
    b.show({ kind: "empty", empty: { text: "无会话" } });
    expect([b.root.querySelector('[role="alert"]'), b.root.textContent]).toEqual([null, "无会话"]);
  });

  it("过期：旧数据照常在、顶上警告条；部分：没答的那台单独一行；很多：底下一行；禁用：说为什么", () => {
    const b = dataBlock();
    const rows = (): HTMLElement => Object.assign(document.createElement("div"), { textContent: "旧行" });
    b.show({ kind: "stale", text: "devbox 离线 · 采样 3m 前", retry: () => {}, content: rows() });
    expect([b.root.querySelector('[role="status"]')?.textContent?.includes("采样 3m 前"), b.root.textContent?.includes("旧行")]).toEqual([true, true]);
    b.show({ kind: "partial", content: rows(), missing: ["gpu-01 无应答", "win-laptop 读取中"] });
    expect(b.root.textContent).toBe("旧行gpu-01 无应答win-laptop 读取中");
    b.show({ kind: "many", content: rows(), note: "前 500 · 搜索可找全部" });
    expect(b.root.textContent).toBe("旧行前 500 · 搜索可找全部");
    b.show({ kind: "disabled", why: "devbox 离线" });
    expect([b.root.dataset.state, b.root.title]).toEqual(["disabled", "devbox 离线"]);
  });

  it("刷新：内容不动，右上角转圈；收掉就没了", () => {
    const b = dataBlock();
    b.show({ kind: "ready", content: Object.assign(document.createElement("div"), { textContent: "行" }) });
    b.refreshing(true);
    b.refreshing(true);
    expect([b.root.textContent, b.root.querySelectorAll('span[aria-hidden="true"]').length]).toEqual(["行", 1]);
    b.refreshing(false);
    expect(b.root.querySelectorAll('span[aria-hidden="true"]').length).toBe(0);
  });
});
