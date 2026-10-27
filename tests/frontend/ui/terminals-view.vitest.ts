/**
 * 「终端」视图（`views/terminals-view.ts`）的判据：名单照后端那份摆，实时只改行内字不挪行。读写 · 终端面换成假的。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 收到某行状态变化的那份（后端把它排到别处）⇒ 行的 DOM 顺序不变、行内字跟着变 | 「不挪行」 |
 * | 新来的接在那一组末尾；没了的删掉 | 「接尾」 |
 * | 换筛选 ⇒ 照后端的先后重排、只留那一格的行 | 「换筛选」 |
 * | 找：按后端给的 `find` 那一格筛 | 「找」 |
 * | Ctrl 点两行 ⇒ 底下「已选 2」，结束交的是那两行 | 「多选」 |
 * | 点一行 ⇒ 终端面看那一台的那个终端 | 「看」 |
 * | 空 · 某台没答（那一句 ＋ 重新连接）· 读不出 | 「样子」 |
 */
import { describe, expect, it, vi } from "vitest";

// 新加的字在合并树里才进文案表：表里没有的那几条照键名出（有的照表）。
vi.mock("../../../src/frontend/ui/copy-table", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/copy-table")>();
  return {
    ...real,
    copyText: (key: string, args?: Record<string, string | number>) => {
      try {
        return real.copyText(key as never, args as never);
      } catch {
        return `${key}${args ? JSON.stringify(args) : ""}`;
      }
    },
  };
});

import { TerminalsView, type TerminalsHost } from "../../../src/frontend/ui/views/terminals-view";
import type { TerminalsBoard } from "../../../src/frontend/ui/generated/TerminalsBoard";
import type { BoardRow } from "../../../src/frontend/ui/generated/BoardRow";
import type { BoardGroup } from "../../../src/frontend/ui/generated/BoardGroup";
import type { Watched } from "../../../src/frontend/ui/terminal-page";

function row(key: string, over: Partial<BoardRow> = {}): BoardRow {
  return {
    key,
    terminal: `h-${key}`,
    tmux_name: `ccm-${key}`,
    sid: `sid-${key}`,
    dot: "running",
    dot_name: "运行中",
    title: `title ${key}`,
    mono: false,
    meta: ["work", "~/src"],
    chips: ["后台"],
    at_text: "1m",
    tags: ["session", "background"],
    find: `title ${key}\n~/src`,
    can_end: true,
    ...over,
  };
}

function group(machine: string, rows: BoardRow[], over: Partial<BoardGroup> = {}): BoardGroup {
  return { machine, name: machine, sub: `${rows.length} 个终端`, complete: true, rows, ...over };
}

function board(groups: BoardGroup[]): TerminalsBoard {
  const n = groups.reduce((a, g) => a + g.rows.length, 0);
  return {
    filters: [
      { tag: "all", label: "全部", count: n },
      { tag: "session", label: "有会话的", count: n },
      { tag: "bare", label: "没有会话的", count: 0 },
      { tag: "background", label: "后台", count: n },
    ],
    groups,
  };
}

interface Rig {
  view: TerminalsView;
  host: TerminalsHost & { [K in "read" | "end" | "reconnect" | "watch"]: ReturnType<typeof vi.fn> };
  next(b: TerminalsBoard): void;
}

function rig(first: TerminalsBoard): Rig {
  let current = first;
  const watched: (Watched | null)[] = [];
  const host = {
    machines: () => ["<local>", "gpu"],
    read: vi.fn(async () => current),
    end: vi.fn(async () => {}),
    reconnect: vi.fn(),
    goSession: vi.fn(),
    openWindow: vi.fn(),
    newSession: vi.fn(),
    watch: vi.fn((w: Watched | null) => void watched.push(w)),
    face: () => {
      const el = document.createElement("div");
      el.dataset.face = "";
      return el;
    },
  };
  const view = new TerminalsView(host as unknown as TerminalsHost);
  return { view, host: host as Rig["host"], next: (b) => (current = b) };
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

const keys = (): string[] => [...document.querySelectorAll<HTMLElement>("[data-row]")].map((e) => e.dataset.row ?? "");
const rowEl = (k: string): HTMLElement => document.querySelector<HTMLElement>(`[data-row="${k}"]`)!;

describe("「终端」视图", () => {
  it("★ 不挪行：某行状态变了、后端把它排到前面 ⇒ DOM 顺序不变、行内字变了", async () => {
    const { view, next } = rig(board([group("<local>", [row("a"), row("b"), row("c")])]));
    await view.open();
    await flush();
    expect(keys()).toEqual(["a", "b", "c"]);
    const bEl = rowEl("b");
    next(board([group("<local>", [row("b", { dot: "needs-you", badge: "需手动", at_text: "现在" }), row("a"), row("c")])]));
    await view.refresh();
    await flush();
    expect(keys()).toEqual(["a", "b", "c"]);
    expect(rowEl("b")).toBe(bEl);
    expect(rowEl("b").textContent).toContain("需手动");
    expect(rowEl("b").textContent).toContain("现在");
    view.close();
  });

  it("接尾：新来的接在那一组末尾，没了的删掉", async () => {
    const { view, next } = rig(board([group("<local>", [row("a"), row("b")]), group("gpu", [row("g")])]));
    await view.open();
    await flush();
    next(board([group("<local>", [row("n"), row("b")]), group("gpu", [row("g")])]));
    await view.refresh();
    await flush();
    expect(keys()).toEqual(["b", "n", "g"]);
    view.close();
  });

  it("换筛选：照后端的先后重排、只留那一格的行", async () => {
    const { view, next } = rig(board([group("<local>", [row("a"), row("b", { tags: ["bare"] })])]));
    await view.open();
    await flush();
    next(board([group("<local>", [row("b", { tags: ["bare"] }), row("a")])]));
    await view.refresh();
    expect(keys()).toEqual(["a", "b"]);
    document.querySelector<HTMLElement>('[data-filter="bare"]')!.click();
    await flush();
    expect(keys()).toEqual(["b"]);
    document.querySelector<HTMLElement>('[data-filter="all"]')!.click();
    await flush();
    expect(keys()).toEqual(["b", "a"]);
    view.close();
  });

  it("找：按后端给的 find 那一格筛", async () => {
    const { view } = rig(board([group("<local>", [row("a", { find: "orders\n~/src/orders" }), row("b", { find: "billing" })])]));
    await view.open();
    await flush();
    const box = document.querySelector<HTMLInputElement>("input[data-find]")!;
    box.value = "ORDERS";
    box.dispatchEvent(new Event("input"));
    expect(keys()).toEqual(["a"]);
    view.close();
  });

  it("多选：Ctrl 点两行 ⇒ 已选 2，结束交那两行", async () => {
    const { view, host } = rig(board([group("<local>", [row("a"), row("b"), row("c")])]));
    await view.open();
    await flush();
    rowEl("a").dispatchEvent(new MouseEvent("click", { ctrlKey: true, bubbles: true }));
    rowEl("c").dispatchEvent(new MouseEvent("click", { ctrlKey: true, bubbles: true }));
    const bar = document.querySelector<HTMLElement>("[data-select-bar]")!;
    expect(bar.hidden).toBe(false);
    expect(bar.textContent).toContain("2");
    bar.querySelector<HTMLElement>("[data-end]")!.click();
    await flush();
    expect(host.end).toHaveBeenCalledTimes(1);
    const items = host.end.mock.calls[0][0] as { origin: string; row: BoardRow }[];
    expect(items.map((i) => i.row.key)).toEqual(["a", "c"]);
    expect(items[0].origin).toBe("<local>");
    view.close();
  });

  it("看：点一行 ⇒ 终端面看那一台的那个终端", async () => {
    const { view, host } = rig(board([group("gpu", [row("g")])]));
    await view.open();
    await flush();
    rowEl("g").click();
    const w = host.watch.mock.calls.at(-1)![0] as Watched;
    expect(w).toMatchObject({ kind: "slot", origin: "gpu", terminal: "h-g" });
    expect(rowEl("g").getAttribute("aria-selected")).toBe("true");
    view.close();
  });

  it("样子：空 · 某台没答（那一句 ＋ 重新连接）· 读不出", async () => {
    const empty = rig(board([group("<local>", [])]));
    await empty.view.open();
    await flush();
    expect(document.querySelector("[data-empty]")).not.toBeNull();
    empty.view.close();

    const down = rig(board([group("gpu", [row("g", { dot: "unknown" })], { down: { said: "gpu 未答 · offline" } })]));
    await down.view.open();
    await flush();
    const d = document.querySelector<HTMLElement>("[data-down]")!;
    expect(d.textContent).toContain("gpu 未答 · offline");
    d.querySelector("button")!.click();
    expect(down.host.reconnect).toHaveBeenCalledWith("gpu");
    expect(keys()).toEqual(["g"]);
    down.view.close();

    const bad = rig(board([]));
    bad.host.read.mockRejectedValueOnce(new Error("boom"));
    await bad.view.open();
    await flush();
    expect(document.querySelector("[data-failed]")).not.toBeNull();
    bad.view.close();
  });
});
