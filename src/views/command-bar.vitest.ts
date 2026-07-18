// F84（#57）：命令栏 —— 纯 filterCommands + 视图（jsdom：open/过滤/方向键/回车执行/背景关/Esc）。
import { describe, it, expect, vi } from "vitest";

const { pushOverlay, popOverlay } = vi.hoisted(() => ({
  pushOverlay: vi.fn(),
  popOverlay: vi.fn(),
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay, popOverlay },
}));

import { filterCommands, CommandBarView, type Command } from "./command-bar";

const cmd = (id: string, title: string, keywords?: string): Command => ({
  id,
  title,
  keywords,
  run: vi.fn(),
});

describe("F84 filterCommands", () => {
  const cmds = [
    cmd("a", "打开历史浏览器", "history 历史"),
    cmd("b", "打开代码全景", "panorama 全景"),
    cmd("c", "切到下一个 Tab", "next tab"),
    cmd("d", "最小化窗口", "minimize"),
  ];

  it("空 query → 原序返回全部（新数组）", () => {
    const r = filterCommands(cmds, "");
    expect(r.map((c) => c.id)).toEqual(["a", "b", "c", "d"]);
    expect(r).not.toBe(cmds);
  });
  it("子串命中标题", () => {
    expect(filterCommands(cmds, "打开").map((c) => c.id)).toEqual(["a", "b"]);
  });
  it("大小写不敏感 + keywords 命中", () => {
    expect(filterCommands(cmds, "HISTORY").map((c) => c.id)).toEqual(["a"]);
    expect(filterCommands(cmds, "minimize").map((c) => c.id)).toEqual(["d"]);
  });
  it("标题前缀命中排在 keywords 命中之前", () => {
    const list = [
      cmd("kw", "关闭面板", "tab 相关"), // 仅 keywords 含 "tab"
      cmd("pre", "Tab 切换", "切换"), // 标题前缀含 "tab"
    ];
    expect(filterCommands(list, "tab").map((c) => c.id)).toEqual(["pre", "kw"]);
  });
  it("无命中 → 空", () => {
    expect(filterCommands(cmds, "zzz")).toEqual([]);
  });
});

describe("F84 CommandBarView", () => {
  const mkView = (cmds: Command[]) => new CommandBarView(() => cmds);

  it("open：挂 DOM + pushOverlay + 聚焦输入 + 渲染全表", () => {
    document.body.replaceChildren();
    pushOverlay.mockClear();
    const view = mkView([cmd("a", "打开历史"), cmd("b", "打开全景")]);
    view.open();
    expect(view.isVisible()).toBe(true);
    expect(pushOverlay).toHaveBeenCalledWith(view);
    expect(document.querySelectorAll(".command-bar-item").length).toBe(2);
    expect(document.activeElement).toBe(document.querySelector(".command-bar-input"));
    // 首项默认选中
    expect(document.querySelector(".command-bar-item")?.classList.contains("selected")).toBe(true);
  });

  it("输入过滤缩小列表；无匹配显空态", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "打开历史", "history"), cmd("b", "最小化", "minimize")]);
    view.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    input.value = "历史";
    input.dispatchEvent(new Event("input"));
    expect([...document.querySelectorAll(".command-bar-item")].map((e) => e.textContent)).toEqual([
      "打开历史",
    ]);
    input.value = "zzz";
    input.dispatchEvent(new Event("input"));
    expect(document.querySelector(".command-bar-empty")?.textContent).toContain("无匹配");
  });

  it("ArrowDown/Up 移动选中（环绕）", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A"), cmd("b", "B"), cmd("c", "C")]);
    view.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    const selectedText = () =>
      document.querySelector(".command-bar-item.selected")?.textContent;
    expect(selectedText()).toBe("A");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    expect(selectedText()).toBe("B");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp" }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp" })); // 环绕到末项
    expect(selectedText()).toBe("C");
  });

  it("Enter 执行选中命令的 run 且 close（先 close 再 run）", () => {
    document.body.replaceChildren();
    popOverlay.mockClear();
    const a = cmd("a", "A");
    const b = cmd("b", "B");
    const view = mkView([a, b]);
    let visibleAtRun: boolean | null = null;
    (b.run as ReturnType<typeof vi.fn>).mockImplementation(() => {
      visibleAtRun = view.isVisible(); // run 执行时命令栏应已关（先 close 再 run）
    });
    view.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" })); // 选 B
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(b.run).toHaveBeenCalledTimes(1);
    expect(a.run).not.toHaveBeenCalled();
    expect(view.isVisible()).toBe(false);
    expect(popOverlay).toHaveBeenCalledWith(view);
    expect(visibleAtRun).toBe(false);
  });

  it("点命令项执行并 close", () => {
    document.body.replaceChildren();
    const a = cmd("a", "A");
    const view = mkView([a]);
    view.open();
    const item = document.querySelector<HTMLElement>(".command-bar-item")!;
    item.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(a.run).toHaveBeenCalledTimes(1);
    expect(view.isVisible()).toBe(false);
  });

  it("点背景（root，非 box）关闭", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A")]);
    view.open();
    const root = document.querySelector<HTMLElement>(".command-bar")!;
    root.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(view.isVisible()).toBe(false);
  });

  it("空列表 Enter → no-op（不崩）", () => {
    document.body.replaceChildren();
    const view = mkView([]);
    view.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    expect(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }))).not.toThrow();
    expect(view.isVisible()).toBe(true); // 无命令可执行，不关
  });

  it("过滤缩表后 selected 重置（不越界、不跑 stale 命令）", () => {
    document.body.replaceChildren();
    const a = cmd("a", "打开历史", "history");
    const b = cmd("b", "打开全景", "panorama");
    const c = cmd("c", "打开用量", "usage");
    const view = mkView([a, b, c]);
    view.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    // 选中移到末项（selected=2）
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    // 过滤到只剩 1 项 → selected 必须重置，否则 filtered[2] 越界
    input.value = "用量";
    input.dispatchEvent(new Event("input"));
    expect(document.querySelector(".command-bar-item.selected")?.textContent).toBe("打开用量");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(c.run).toHaveBeenCalledTimes(1);
    expect(a.run).not.toHaveBeenCalled();
    expect(b.run).not.toHaveBeenCalled();
  });

  it("开着时 Ctrl+K 关闭（输入框本地兜——dispatcher 被可编辑守卫拦）", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A")]);
    view.open();
    expect(view.isVisible()).toBe(true);
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { ctrlKey: true, code: "KeyK", key: "k" }));
    expect(view.isVisible()).toBe(false);
  });

  it("handleEsc / toggle", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A")]);
    view.toggle();
    expect(view.isVisible()).toBe(true);
    view.handleEsc();
    expect(view.isVisible()).toBe(false);
    view.toggle();
    expect(view.isVisible()).toBe(true);
  });
});
