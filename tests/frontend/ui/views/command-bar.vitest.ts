// F84（#57）：命令栏 —— 纯 filterCommands + 视图（jsdom：open/过滤/方向键/回车执行/背景关/Esc）。
import { describe, it, expect, vi } from "vitest";

const { pushOverlay, popOverlay } = vi.hoisted(() => ({
  pushOverlay: vi.fn(),
  popOverlay: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay, popOverlay },
}));

import { filterCommands, CommandBarView, type Command } from "../../../../src/frontend/ui/views/command-bar";

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { srcDirOf } from "../../../test-support/repo-root";
import { copyText } from "../../../../src/frontend/ui/copy-table";
/**
 * `P6d-Y1`：写动作那半的**裁定**必须写在头注上，且措辞是「裁定不加」而不是「延后再加」。
 *
 * # 为什么值得立一条判据钉一段散文
 *
 * `U2`〔用@08-11〕逐字：「**不加**。`P6d` 因此只做读动作那一半，`#57` 正文那句
 * 「输主机名直接 ssh/attach」**明确不做，不是漏做** —— **要写进 `P6d` 的诚实边界**。」
 * 而 `P6d` 的件文件**当时根本不存在**（`IDX` 声明了它却从没建过节点）⇒ 那句指令
 * 在账本里悬着，代码这边一个字都没收到。
 *
 * 更糟的是头注原来写「**首刀**排除……**延后**须 danger + 二次确认」——那是**排期**口吻：
 * 下一个人会以为「配好 danger 样式就能加」，而事实是这条路**被裁掉了**。
 * 两者差一个量级（同 `#79` 的「上游还没有 vs 我们还没做」）。
 *
 * ⚠ 本条读的是 `command-bar.ts`，**不是本文件** —— 判据不会被自己的字面量喂绿
 * （本会话第七次防同一种自伤）。
 */
describe("P6d-Y1：写动作是裁定不做，不是待办", () => {
  const src = readFileSync(resolve(srcDirOf(__dirname), "command-bar.ts"), "utf8");

  it("★★ 裁定与它的住址都写在头注上", () => {
    for (const needle of ["U2", "明确不做", "不是漏做", ""]) {
      expect(src, `头注里少了「${needle}」`).toContain(needle);
    }
  });

  it("★ 不许再单靠「首刀/延后」把写动作说成排期", () => {
    // 「延后」这个词本身不禁 —— 禁的是**没有裁定在旁边**的那种用法。
    // 判法：出现「延后」时，同一段里必须也出现「裁」。
    const paras = src.split("\n *\n");
    for (const p of paras) {
      if (p.includes("延后") && !p.includes("裁")) {
        throw new Error(
          `这一段用「延后」描述写动作却没提裁定 —— 那正是 U2 要消灭的读法：\n${p.slice(0, 200)}`,
        );
      }
    }
  });

  it("★ 反向自检：命令栏本体还在（否则上面两条会因为「文件都没了」而假绿）", () => {
    expect(src).toContain("filterCommands");
    expect(src).toContain("CommandBarView");
  });
});

const cmd = (id: string, title: string, keywords?: string, group: Command["group"] = "open"): Command => ({
  id,
  title,
  keywords,
  group,
  run: vi.fn(),
});
const ses = (id: string, title: string, waiting: string | null = null): Command => ({
  id,
  title,
  session: { dot: waiting ? "needs-you" : "running", dotLabel: "", machine: null, project: null, title, waiting },
  run: vi.fn(),
});
const ITEM = "[data-role=command-item]";
const INPUT = "[data-role=command-input]";
const items = () => [...document.querySelectorAll<HTMLElement>(ITEM)];
const selectedText = () => document.querySelector(`${ITEM}[aria-selected=true]`)?.textContent;
const key = (init: KeyboardEventInit) => document.querySelector<HTMLInputElement>(INPUT)!.dispatchEvent(new KeyboardEvent("keydown", init));
const type = (text: string) => {
  const input = document.querySelector<HTMLInputElement>(INPUT)!;
  input.value = text;
  input.dispatchEvent(new Event("input"));
};
const groupTitles = () => [...document.querySelectorAll<HTMLElement>("[role=listbox] > :not([data-role=command-item])")].map((e) => e.textContent);

describe("F84 filterCommands", () => {
  const cmds = [
    cmd("a", "打开历史浏览器", "history 历史"),
    cmd("b", "打开多 agent 监控", "grid 网格"),
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

describe("命令面板", () => {
  const mkView = (cmds: Command[]) => new CommandBarView(() => cmds);

  it("open：模态压栈 ＋ 焦点进框 ＋ 首项选中", () => {
    document.body.replaceChildren();
    pushOverlay.mockClear();
    const view = mkView([cmd("a", "打开历史"), cmd("b", "打开监控")]);
    view.open();
    expect(view.isVisible()).toBe(true);
    expect(pushOverlay.mock.calls[0][0].modal, "开着时快捷键只放行 Esc").toBe(true);
    expect(items().length).toBe(2);
    expect(document.activeElement).toBe(document.querySelector(INPUT));
    expect(selectedText()).toBe("打开历史");
  });

  it("🔴 空输入：按分组列（需手动 · 当前会话 · 打开 · 窗口 · 账号），会话只列在等你的；有输入：会话在前、命令在后", () => {
    document.body.replaceChildren();
    const view = mkView([
      ses("s1", "表格分页", "等批准"),
      ses("s2", "清洗日志"),
      cmd("w", "全屏", "fullscreen", "window"),
      cmd("f", "在会话里找", "find", "current"),
      cmd("h", "历史", "history", "open"),
      cmd("acct", "管理账号…", "account", "account"),
    ]);
    view.open();
    expect(groupTitles()).toEqual([copyText("commandBar.group.needs"), copyText("commandBar.group.current"), copyText("commandBar.group.open"), copyText("commandBar.group.window"), copyText("commandBar.group.account")]);
    expect(items().map((e) => e.textContent)).toEqual([`表格分页${copyText("commandBar.session.waiting", { what: "等批准" })}`, "在会话里找", "历史", "全屏", "管理账号…"]);
    type("日");
    expect(groupTitles()).toEqual([copyText("commandBar.group.sessions")]);
    type("i");
    expect(groupTitles()).toEqual([copyText("commandBar.group.commands")]);
    view.close();
  });

  it("输入过滤缩小列表；无匹配显空态", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "打开历史", "history"), cmd("b", "最小化", "minimize")]);
    view.open();
    type("历史");
    expect(items().map((e) => e.textContent)).toEqual(["打开历史"]);
    type("zzz");
    expect(document.querySelector("[role=listbox]")?.textContent).toBe(copyText("commandBar.renderList.none"));
  });

  it("ArrowDown/Up 移动选中（环绕，跳过灰着的）；组字中的方向键 / Enter 归输入法", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A"), { ...cmd("b", "B"), disabled: "仅 Windows" }, cmd("c", "C")]);
    view.open();
    expect(selectedText()).toBe("A");
    key({ key: "ArrowDown" });
    expect(selectedText(), "灰着的那项不许选中").toBe("C");
    key({ key: "ArrowDown", isComposing: true });
    expect(selectedText()).toBe("C");
    key({ key: "ArrowUp" });
    expect(selectedText()).toBe("A");
    key({ key: "ArrowUp" }); // 环绕到末项
    expect(selectedText()).toBe("C");
    expect(items()[1].getAttribute("aria-disabled")).toBe("true");
    expect(items()[1].textContent).toBe("B仅 Windows");
  });

  it("Enter 执行选中命令的 run 且 close（先 close 再 run）；灰着的点了不做事", () => {
    document.body.replaceChildren();
    popOverlay.mockClear();
    const a = cmd("a", "A");
    const b = cmd("b", "B");
    const off = { ...cmd("x", "X"), disabled: "仅 Windows" };
    const view = mkView([a, b, off]);
    let visibleAtRun: boolean | null = null;
    (b.run as ReturnType<typeof vi.fn>).mockImplementation(() => {
      visibleAtRun = view.isVisible();
    });
    view.open();
    items()[2].dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(off.run).not.toHaveBeenCalled();
    expect(view.isVisible()).toBe(true);
    key({ key: "ArrowDown" }); // 选 B
    key({ key: "Enter", isComposing: true });
    expect(b.run, "组字中的 Enter 归输入法").not.toHaveBeenCalled();
    key({ key: "Enter" });
    expect(b.run).toHaveBeenCalledTimes(1);
    expect(a.run).not.toHaveBeenCalled();
    expect(view.isVisible()).toBe(false);
    expect(popOverlay).toHaveBeenCalled();
    expect(visibleAtRun).toBe(false);
  });

  it("点命令项执行并 close；点遮罩关", () => {
    document.body.replaceChildren();
    const a = cmd("a", "A");
    const view = mkView([a]);
    view.open();
    items()[0].dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(a.run).toHaveBeenCalledTimes(1);
    expect(view.isVisible()).toBe(false);
    view.open();
    const backdrop = document.querySelector<HTMLElement>("[role=dialog]")!.parentElement!;
    backdrop.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(view.isVisible()).toBe(false);
  });

  it("空列表 Enter → no-op（不崩）", () => {
    document.body.replaceChildren();
    const view = mkView([]);
    view.open();
    expect(() => key({ key: "Enter" })).not.toThrow();
    expect(view.isVisible()).toBe(true);
    view.close();
  });

  it("过滤缩表后 selected 重置（不越界、不跑 stale 命令）", () => {
    document.body.replaceChildren();
    const a = cmd("a", "打开历史", "history");
    const b = cmd("b", "打开监控", "grid");
    const c = cmd("c", "打开用量", "usage");
    const view = mkView([a, b, c]);
    view.open();
    key({ key: "ArrowDown" });
    key({ key: "ArrowDown" });
    type("用量");
    expect(selectedText()).toBe("打开用量");
    key({ key: "Enter" });
    expect(c.run).toHaveBeenCalledTimes(1);
    expect(a.run).not.toHaveBeenCalled();
    expect(b.run).not.toHaveBeenCalled();
  });

  it("开着时 Ctrl+K 关闭（模态压栈后快捷键只放行 Esc，这一键由框自己接）；toggle", () => {
    document.body.replaceChildren();
    const view = mkView([cmd("a", "A")]);
    view.open();
    key({ ctrlKey: true, code: "KeyK", key: "k" });
    expect(view.isVisible()).toBe(false);
    view.toggle();
    expect(view.isVisible()).toBe(true);
    view.toggle();
    expect(view.isVisible()).toBe(false);
  });
});
