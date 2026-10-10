/**
 * 通用组件各态：每件每一态画出来的形状（角色 · data-* · aria-* · 字），与该态的行为。
 * 样子（颜色 · 尺寸）由 CSS Modules 按这些态上，截图那一页（`tests/shots` 的组件总览）看。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { button, setBusy, setDisabled, buttonRow } from "../../../../src/frontend/ui/kit/button";
import { field } from "../../../../src/frontend/ui/kit/field";
import { toggleSwitch, checkbox } from "../../../../src/frontend/ui/kit/switch";
import { tabs, segmented } from "../../../../src/frontend/ui/kit/tabs";
import { chip, setChipOpen } from "../../../../src/frontend/ui/kit/chip";
import { fold } from "../../../../src/frontend/ui/kit/fold";
import { banner } from "../../../../src/frontend/ui/kit/banner";
import { emptyState } from "../../../../src/frontend/ui/kit/empty";
import { countBadge, tag, kbd } from "../../../../src/frontend/ui/kit/badge";
import { skeletonRows } from "../../../../src/frontend/ui/kit/skeleton";
import { spinner } from "../../../../src/frontend/ui/kit/progress";
import { meter } from "../../../../src/frontend/ui/kit/meter";
import { statusDot, setDot } from "../../../../src/frontend/ui/kit/status-dot";
import { icon } from "../../../../src/frontend/ui/kit/icon";

beforeEach(() => document.body.replaceChildren());

describe("按钮", () => {
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

  it("按钮行：按给的先后摆（取消在左、确认在右）", () => {
    const row = buttonRow(button({ label: "取消" }), button({ label: "结束", kind: "danger" }));
    expect([...row.children].map((c) => c.textContent)).toEqual(["取消", "结束"]);
  });
});

describe("输入框", () => {
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

describe("开关 · 复选框", () => {
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

describe("分栏 · 分段按钮", () => {
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

describe("chip", () => {
  it("可点的是按钮、只读的是 span；有事才上色；开着浮层 aria-expanded", () => {
    const c = chip({ text: "需手动 2", tone: "warn", onClick: () => {} });
    expect([c.tagName, c.dataset.intent]).toEqual(["BUTTON", "warn"]);
    setChipOpen(c, true);
    expect(c.getAttribute("aria-expanded")).toBe("true");
    expect(chip({ text: "5h 63%" }).tagName).toBe("SPAN");
  });

});

describe("折叠块", () => {
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

describe("转圈 · 计量条 · 徽标 · 键帽 · 骨架 · 状态点", () => {
  it("转圈：纯装饰，读屏器不念", () => {
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

  it("计数 0 不画、99 以上写 99+；需手动的计数琥珀；标记与键帽", () => {
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

describe("错误条 · 空态", () => {
  it("错误条 role=alert、警告条 role=status；不带 ×；动作在右", () => {
    const e = banner("error", "读取账号失败 · 内容无法解析", [button({ label: "复制详情" })]);
    expect([e.getAttribute("role"), e.dataset.intent, e.lastElementChild?.textContent]).toEqual(["alert", "error", "复制详情"]);
    expect(banner("warn", "重启 cc-monitor 后生效").getAttribute("role")).toBe("status");
  });

  it("空态：图标 ＋ 一句 ＋ 怎么让它有 ＋ 至多一颗按钮", () => {
    const e = emptyState({ text: "无会话", hint: "终端里 ccm 启动后自动出现" });
    expect(e.textContent).toBe("无会话终端里 ccm 启动后自动出现");
  });
});
