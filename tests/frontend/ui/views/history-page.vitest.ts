/**
 * 历史页（`src/frontend/ui/views/history.ts`）的判据 —— 设计稿「文件与历史」乙4 ②③ · 乙5 · 乙2 那几条。
 *
 * 1. 开页即给焦点（R5W-H08）；每台问一次 `history-list`（远端带 `origin`），哪台先答先画；行按 `at` 并成一列、按今天 / 昨天 … 分段。
 * 2. 敲字 ⇒ 停 150 ms 带 `query` 再问各台（后端搜全部会话，R2-2-5）；不在界面里过滤。
 * 3. 一台没答 ⇒ 列表顶「{机器} 离线 · 未列出［重新连接］」，点了只再问那一台、带 `fresh`（R2-2-6）；别的台照画。
 * 4. 回车 ⇒ 内容搜索；失败 ⇒ 清掉旧结果、换成错误条（R5W-H06）。
 * 5. 行上的按钮照后端给的 `can` 画：在跑的是「切过去」（不起第二份）；Codex 行恢复按它自己那一家起（E1）。
 * 6. 删除只问一次（R2-2-2）；在跑的不问、直接说「运行中 · 先结束」。
 * 7. 分叉挂在父会话下（默认收起，「1 个分叉 ▸」）；父会话被筛掉 ⇒ 后端带来的 `context` 行淡显（R5W-H05）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
/** 查看器替身：只把宿主拼好的头摆出来（头里的按钮由历史页建，判据点的就是它们）、记下最后一次 `load` 与头下那一条。 */
const viewerStub = vi.hoisted(() => ({ last: null as null | Record<string, unknown>, banner: null as HTMLElement | null }));
vi.mock("../../../../src/frontend/ui/views/session-viewer", () => ({
  viewerPath: (t: string) => Object.assign(document.createElement("span"), { textContent: t }),
  SessionViewer: class {
    element = document.createElement("div");
    load(o: Record<string, unknown>): Promise<void> {
      viewerStub.last = o;
      const h = o.head as { badges?: HTMLElement[]; actions?: HTMLElement; meta?: HTMLElement[] } | undefined;
      const head = document.createElement("div");
      head.dataset.role = "viewer-head";
      head.append(...(h?.badges ?? []), ...(h?.actions ? [h.actions] : []), ...(h?.meta ?? []));
      this.element.replaceChildren(head);
      return Promise.resolve();
    }
    showBanner(el: HTMLElement | null): void {
      viewerStub.banner = el;
    }
    dispose(): void {}
    openFind(): void {}
  },
}));
vi.mock("../../../../src/frontend/ui/tmux-resume", () => ({ startInTmuxThenAttach: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../../src/frontend/ui/launch-menu", () => ({
  enumerateAccountModifiers: vi.fn().mockResolvedValue([
    { kind: "base", label: "账号 0" },
    { kind: "account", name: "work", label: "work" },
    { kind: "account", name: "home", label: "home" },
  ]),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/local-resume", () => ({ resumeLocalSession: vi.fn().mockResolvedValue(true) }));
vi.mock("../../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  getBehavior: () => Promise.resolve({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({ resolveResumeCommand: () => Promise.resolve("") }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../../../../src/frontend/ui/views/history";
import { toast } from "../../../../src/frontend/ui/kit/toast";
import { resumeLocalSession } from "../../../../src/frontend/ui/local-resume";
import { startInTmuxThenAttach } from "../../../../src/frontend/ui/tmux-resume";
import { enumerateAccountModifiers } from "../../../../src/frontend/ui/launch-menu";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { answerAskDialog, answerAskText, noAskDialog } from "../../../test-support/ask-dialog-driver.ts";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const NOW = Date.now();

function row(over: Record<string, unknown>): Record<string, unknown> {
  const sid = String(over.sessionId);
  const at = Number(over.at ?? NOW - 60_000);
  return {
    agent: "claude",
    agentTag: null,
    projectDir: "-w-p",
    projectPath: "/w/p",
    projectName: "p",
    group: "claude:/w/p",
    aiTitle: null,
    firstUserExcerpt: "说过的话",
    title: `标题 ${sid}`,
    label: `标题 ${sid}`,
    untitled: false,
    startedAt: at,
    updatedAt: at,
    at,
    jsonlPath: `/h/.claude/projects/-w-p/${sid}.jsonl`,
    messageCountApprox: 3,
    isBg: false,
    starred: false,
    customTitle: null,
    hidden: false,
    status: "ended",
    can: { resume: "yes", accounts: true, fork: true, delete: "yes" },
    ...over,
  };
}

const list = (rows: Record<string, unknown>[]) => ({ rows, groups: [], total: rows.length, truncated: false, notice: null });

/** 每台的回答（`origin` 缺 = 本机）；`fail` 里的那几台答不上。 */
let world: { local: Record<string, unknown>[]; dev: Record<string, unknown>[]; fail: Set<string>; search: "ok" | "fail" };

function calls(op: string): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "chan_call" && (c[1] as ChanCallArgs).op === op)
    .map((c) => chanArgsJson(c[1] as ChanCallArgs) as Record<string, unknown>);
}

const flush = async (ms = 0): Promise<void> => {
  await new Promise((r) => setTimeout(r, ms));
  await new Promise((r) => setTimeout(r, 0));
};

async function opened(): Promise<HistoryView> {
  const v = new HistoryView();
  await v.open();
  await flush();
  return v;
}

const rows = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="option"][data-key]')];
const titles = (): string[] => rows().map((r) => r.querySelector("span")?.textContent ?? "");
const byText = (sel: string, text: string): HTMLElement | undefined =>
  [...document.querySelectorAll<HTMLElement>(sel)].find((e) => (e.textContent ?? "").includes(text));

beforeEach(() => {
  // jsdom 没有布局：滚到哪一行量不了，这里只要它不抛。
  Element.prototype.scrollIntoView = () => {};
  document.body.replaceChildren();
  vi.mocked(toast).mockClear();
  vi.mocked(resumeLocalSession).mockClear();
  vi.mocked(startInTmuxThenAttach).mockClear();
  vi.mocked(enumerateAccountModifiers).mockClear();
  viewerStub.last = null;
  viewerStub.banner = null;
  world = { local: [], dev: [], fail: new Set(), search: "ok" };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd === "list_remote_mcp_origins") return ["dev"];
    if (cmd !== "chan_call") return undefined;
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    if (c.op === "history-list") {
      const o = (args.origin as string | undefined) ?? "";
      if (world.fail.has(o)) return Promise.reject(refusedReply("unreachable", `${o} 问不到`));
      const q = String(args.query ?? "");
      const mine = (o ? world.dev : world.local).filter((r) => !q || String(r.label).includes(q));
      return chanReply(list(mine.map((r) => (o ? { ...r, origin: o } : r))));
    }
    if (c.op === "history-search") {
      if (world.search === "fail") return Promise.reject(new Error("搜不动"));
      return chanReply({
        lines: [JSON.stringify({ agent: "claude", sessionId: "a", projectPath: "/w/p", projectName: "p", jsonlPath: "/x/a.jsonl", title: "标题 a", updatedAt: NOW, hitCount: 1, hits: [{ uuid: "u1", tsMs: NOW, kind: "user", before: "前", matched: "词", after: "后" }], hitsTruncated: false })],
        unreadable: 0,
        skipped: [],
      });
    }
    if (c.op === "history-search-merge") {
      const ss = args.sessions as { hitCount: number }[];
      return chanReply({ totalHits: ss.reduce((n, s) => n + s.hitCount, 0), sessionCount: ss.length, truncated: false, sessions: ss });
    }
    if (c.op === "history-annotate") return chanReply({ entry: { starred: false, customTitle: null, hidden: true, updatedAt: 1 } });
    if (c.op === "files-delete-session") return chanReply({ path: "/x" });
    if (c.op === "history-forget") return chanReply({ removed: true, sid: "a" });
    return undefined;
  });
});

describe("开页 · 每台一问 · 按时间", () => {
  it("开页即给焦点；本机与 dev 各问一次；行按 at 并成一列、分段；远端行带机器标签", async () => {
    world.local = [row({ sessionId: "a", at: NOW - 1000 }), row({ sessionId: "c", at: NOW - 3 * 86_400_000 - 1 })];
    world.dev = [row({ sessionId: "b", at: NOW - 2000 })];
    const v = await opened();
    expect(document.activeElement?.classList.contains("history-search")).toBe(true);
    expect(calls("history-list").map((a) => a.origin ?? "")).toEqual(["", "dev"]);
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["a", "b", "c"]);
    expect(byText('[role="option"]', "标题 b")?.textContent).toContain("dev");
    expect(document.body.textContent).toContain(copyText("history.section.today"));
    v.close();
  });

  it("敲字 ⇒ 停 150 ms 后带 query 再问各台，界面不自己筛", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "z", label: "别的" })];
    const v = await opened();
    invokeMock.mock.calls.length = 0;
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "标题";
    input.dispatchEvent(new Event("input"));
    await flush(50);
    expect(calls("history-list")).toEqual([]);
    await flush(150);
    expect(calls("history-list").map((a) => [a.origin ?? "", a.query])).toEqual([
      ["", "标题"],
      ["dev", "标题"],
    ]);
    expect(titles()).toEqual(["标题 a"]);
    v.close();
  });
});

describe("一台没答", () => {
  it("dev 答不上 ⇒ 顶上一条［重新连接］，点了只再问 dev、带 fresh；本机照画", async () => {
    world.local = [row({ sessionId: "a" })];
    world.fail.add("dev");
    const v = await opened();
    expect(titles()).toEqual(["标题 a"]);
    const msg = copyText("history.list.machineDown", { machine: "dev" });
    expect(document.body.textContent).toContain(msg);
    invokeMock.mock.calls.length = 0;
    world.fail.clear();
    world.dev = [row({ sessionId: "b", at: NOW })];
    byText("button", copyText("history.list.reconnect"))!.click();
    await flush();
    expect(calls("history-list")).toEqual([{ origin: "dev", sort: "activity", fresh: true }]);
    expect(titles()).toEqual(["标题 b", "标题 a"]);
    expect(document.body.textContent).not.toContain(msg);
    v.close();
  });
});

describe("内容搜索", () => {
  it("回车搜内容；再搜失败 ⇒ 旧结果清掉、换成错误条", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "词";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(document.querySelector("mark")?.textContent).toBe("词");
    world.search = "fail";
    vi.spyOn(console, "warn").mockImplementation(() => {});
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(document.querySelector("mark")).toBeNull();
    expect(document.body.textContent).toContain(copyText("history.search.failed", { why: "" }).trim().split("·")[0].trim());
    v.close();
  });
});

describe("行上的动作照 can 画", () => {
  it("在跑的 ⇒「切过去」，点了切主窗口、不起第二份", async () => {
    world.local = [row({ sessionId: "a", status: "live", can: { resume: "switch", accounts: true, fork: true, delete: "live" } })];
    const v = await opened();
    const went: string[] = [];
    v.switchTo = (sid) => went.push(sid);
    byText("button", copyText("history.row.switch"))!.click();
    await flush();
    expect(went).toEqual(["a"]);
    expect(resumeLocalSession).not.toHaveBeenCalled();
    expect(v.isVisible()).toBe(false);
  });

  it("Codex 行恢复 ⇒ 按它自己那一家起（E1）", async () => {
    world.local = [row({ sessionId: "cx", agent: "codex", agentTag: "Codex", status: "unknown", can: { resume: "yes", accounts: false, fork: false, delete: "unsure" } })];
    const v = await opened();
    expect(byText('[role="option"]', "Codex")).toBeTruthy();
    byText("button", copyText("history.row.resume"))!.click();
    await flush();
    expect(vi.mocked(resumeLocalSession).mock.calls[0][0]).toMatchObject({ agent: "codex", sid: "cx" });
    v.close();
  });
});

describe("删除", () => {
  it("只问一次；在跑的不问、说「运行中 · 先结束」", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "l", status: "live", can: { resume: "switch", accounts: true, fork: true, delete: "live" } })];
    const v = await opened();
    const key = (sid: string) => rows().find((r) => r.dataset.key?.endsWith(sid))!;
    key("l").click();
    key("l").dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await flush();
    expect(noAskDialog()).toBe(true);
    expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toContain(copyText("history.delete.liveHint"));
    key("a").click();
    key("a").dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await flush();
    await answerAskDialog(true);
    await flush();
    expect(noAskDialog()).toBe(true);
    v.close();
  });
});

describe("分叉", () => {
  it("挂在父会话下、默认收起；父会话是 context ⇒ 淡显", async () => {
    world.local = [
      row({ sessionId: "kid", at: NOW - 1000, forkedFromSessionId: "dad", forkedFromMessageUuid: "m" }),
      row({ sessionId: "dad", at: NOW - 5000, context: true }),
    ];
    const v = await opened();
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["dad"]);
    expect(rows()[0].dataset.context).toBe("true");
    byText("button", copyText("history.row.forks", { n: 1 }))!.click();
    await flush();
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["dad", "kid"]);
    expect(rows()[1].dataset.child).toBe("true");
    v.close();
  });
});

describe("按项目", () => {
  it("组按后端给的序列出、收着；点开出那一组的行；「全部展开」全开", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "b", group: "claude:/w/q", projectPath: "/w/q", projectName: "q" })];
    const groups = [
      { key: "claude:/w/p", agent: "claude", projectName: "p", projectPath: "/w/p", projectDir: "-w-p", count: 1, hasLive: false, starred: false, lastActivity: NOW, order: 2, failed: null },
      { key: "claude:/w/q", agent: "claude", projectName: "q", projectPath: "/w/q", projectDir: "-w-q", count: 1, hasLive: false, starred: false, lastActivity: NOW, order: 1, failed: null },
      { key: "dir:-w-x", agent: "claude", projectName: "-w-x", projectPath: "", projectDir: "-w-x", count: 0, hasLive: null, starred: false, lastActivity: 0, order: 0, failed: "读不了" },
    ];
    const base = invokeMock.getMockImplementation() as (cmd: string, a: unknown) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
      const r = await base(cmd, a);
      if (cmd === "chan_call" && (a as ChanCallArgs).op === "history-list" && !(chanArgsJson(a as ChanCallArgs) as { origin?: string }).origin) {
        const v = JSON.parse(new TextDecoder().decode(new Uint8Array(r as ArrayBuffer))) as Record<string, unknown>;
        return chanReply({ ...v, groups });
      }
      return r;
    });
    const v = await opened();
    byText('[role="tab"]', copyText("history.page.byProject"))!.click();
    await flush();
    const heads = [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')];
    expect(heads.map((h) => h.textContent?.slice(0, 1))).toEqual(["p", "q", "-"]);
    expect(rows()).toEqual([]);
    expect(heads[2].textContent).toContain(copyText("history.group.failed"));
    heads[0].click();
    await flush();
    expect(titles()).toEqual(["标题 a"]);
    byText("button", copyText("history.list.expandAll"))!.click();
    await flush();
    expect(titles()).toEqual(["标题 a", "标题 b"]);
    invokeMock.mock.calls.length = 0;
    byText("button", copyText("history.group.retry"))!.click();
    await flush();
    expect(calls("history-list").map((a) => a.origin ?? "")).toEqual([""]);
    byText('[role="tab"]', copyText("history.page.byTime"))!.click();
    v.close();
  });
});

describe("筛选", () => {
  it("显示已隐藏 · 时间 · 排序都带进下一问；去掉一台 ⇒ 不再问它；筛选按钮上记几项", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    byText("button", copyText("history.page.filter"))!.click();
    await flush();
    const box = (label: string) => byText("label", label)!.querySelector("input")!;
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.showHidden")).click();
    await flush();
    expect(calls("history-list").map((a) => a.hidden)).toEqual([true, true]);
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.time7d")).click();
    box(copyText("history.filter.sortCreated")).click();
    await flush();
    expect(calls("history-list").at(-1)).toMatchObject({ within_days: 7, sort: "created", hidden: true });
    invokeMock.mock.calls.length = 0;
    box("dev").click();
    await flush();
    box(copyText("history.filter.time30d")).click();
    await flush();
    expect(calls("history-list").map((a) => a.origin ?? "")).toEqual([""]);
    expect(byText("button", copyText("history.page.filter"))!.textContent).toContain("4");
    v.close();
  });
});

describe("键盘与菜单", () => {
  it("↓ 从搜索框进列表、走行；Ctrl+回车恢复；菜单里标星 · 隐藏（可撤）· 改标题", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "b", at: NOW - 120_000 })];
    const v = await opened();
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement?.getAttribute("data-key")?.endsWith("a")).toBe(true);
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement?.getAttribute("data-key")?.endsWith("b")).toBe(true);
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true }));
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
    await flush();
    expect(vi.mocked(resumeLocalSession).mock.calls[0][0]).toMatchObject({ sid: "a", agent: "claude" });
    const v2 = await opened();
    const menuItem = (label: string) => [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((b) => b.textContent?.startsWith(label));
    const openMenuOn = (sid: string) => rows().find((r) => r.dataset.key?.endsWith(sid))!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 3, clientY: 3 }));
    openMenuOn("a");
    menuItem(copyText("history.menu.star"))!.click();
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "a", patch: { starred: true } });
    openMenuOn("b");
    menuItem(copyText("history.menu.hide"))!.click();
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "b", patch: { hidden: true } });
    const { undoToast } = await import("../../../../src/frontend/ui/kit/toast");
    expect(vi.mocked(undoToast)).toHaveBeenCalled();
    openMenuOn("a");
    menuItem(copyText("history.menu.rename"))!.click();
    await answerAskText("新名字");
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "a", patch: { customTitle: "新名字" } });
    document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    v2.close();
    v.close();
  });
});

describe("内容头 · 恢复 ▾（乙4-④⑤）", () => {
  const head = (): HTMLElement => document.querySelector<HTMLElement>('[data-role="viewer-head"]')!;
  const inHead = (text: string): HTMLButtonElement | undefined =>
    [...head().querySelectorAll<HTMLButtonElement>("button")].find((b) => (b.textContent ?? "").includes(text) || b.getAttribute("aria-label") === text);
  const menuItems = (): string[] => [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].map((e) => e.textContent ?? "");
  const menuItem = (label: string): HTMLElement | undefined =>
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((b) => b.textContent?.startsWith(label));
  // 前面那几格的筛选（关掉 dev 那台）记在本机，别带进来。
  beforeEach(() => localStorage.clear());
  const showRow = async (sid: string): Promise<void> => {
    rows().find((r) => r.dataset.key?.endsWith(sid))!.click();
    await flush(250);
  };

  it("已结束的行：头上［恢复 ▾］＋ 新窗口 ＋ ⋯；第二行 项目 · 本机 · 路径 · 时间段；▾ ＝ tmux / 不用 tmux × 账号，再加「在此目录新建会话」", async () => {
    world.local = [row({ sessionId: "a", lastAccount: "work" })];
    const v = await opened();
    await showRow("a");
    expect(viewerStub.last).toMatchObject({ displayTitle: "标题 a", origin: "<local>", cwd: "/w/p" });
    const main = inHead(copyText("history.row.resume"))!;
    expect(main.title).toBe(copyText("history.resume.hintAccount", { account: "work" }));
    expect(inHead(copyText("history.menu.openWindow"))).toBeTruthy();
    expect(inHead(copyText("history.row.more"))).toBeTruthy();
    expect(head().textContent).toContain("p");
    expect(head().textContent).toContain(copyText("history.filter.local"));
    expect(head().textContent).toContain("/w/p");
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(enumerateAccountModifiers).toHaveBeenCalledWith("<local>");
    const items = menuItems();
    expect(items[0]).toContain("tmux");
    expect(items).toContain(copyText("history.menu.newInDir"));
    expect(items.some((t) => t.startsWith("work"))).toBe(true);
    // 选「tmux」⇒ 不依赖标签页对象的那条起法，带这一行的那一家、目录、跟随的号
    menuItem("tmux")!.click();
    await flush();
    expect(vi.mocked(startInTmuxThenAttach).mock.calls[0].slice(0, 2)).toEqual([
      { origin: "<local>", agent: "claude", sid: "a", cwd: "/w/p" },
      { kind: "follow" },
    ]);
    expect(resumeLocalSession).not.toHaveBeenCalled();
    v.close();
  });

  it("主按钮 ＝ 默认那一种（跟随的号 · 不用 tmux）；没起来 ⇒ 头下一条错误条 ＋［重试］，不关历史页", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    await showRow("a");
    vi.mocked(resumeLocalSession).mockRejectedValueOnce(new Error("开不了终端"));
    inHead(copyText("history.row.resume"))!.click();
    await flush();
    expect(vi.mocked(resumeLocalSession).mock.calls[0][0]).toMatchObject({ sid: "a", account: { kind: "follow" } });
    expect(viewerStub.banner?.textContent).toContain(copyText("history.resume.failed", { machine: copyText("history.filter.local"), why: "Error: 开不了终端" }));
    expect(v.isVisible()).toBe(true);
    [...viewerStub.banner!.querySelectorAll("button")].find((b) => b.textContent === copyText("history.group.retry"))!.click();
    await flush();
    expect(resumeLocalSession).toHaveBeenCalledTimes(2);
    expect(v.isVisible(), "起了 ⇒ 关历史页").toBe(false);
  });

  it("点名一个号 ⇒ 带着它起（不用 tmux 那一支）", async () => {
    world.dev = [row({ sessionId: "r" })];
    const v = await opened();
    await showRow("r");
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(enumerateAccountModifiers).toHaveBeenCalledWith("dev");
    const { runRemoteResume } = await import("../../../../src/frontend/ui/remote-launch-run");
    const work = menuItem("work")!.parentElement!;
    [...work.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((b) => b.textContent === copyText("tabMenu.containerLeaves.direct"))!.click();
    await flush();
    expect(vi.mocked(runRemoteResume).mock.calls.at(-1)!.slice(0, 4)).toEqual(["dev", "claude", "r", "/w/p"]);
    expect(vi.mocked(runRemoteResume).mock.calls.at(-1)![5]).toEqual({ account: { kind: "named", name: "work" } });
    v.close();
  });

  it("Codex 行：▾ 不问账号，灰一行「Codex · 无账号维」；在跑的 ⇒［切过去］；分身 ⇒ 恢复灰着说为什么", async () => {
    world.local = [
      row({ sessionId: "cx", agent: "codex", agentTag: "Codex", can: { resume: "yes", accounts: false, fork: false, delete: "yes" } }),
      row({ sessionId: "l", status: "live", can: { resume: "switch", accounts: true, fork: true, delete: "live" } }),
      row({ sessionId: "bg", isBg: true, can: { resume: "bg", accounts: true, fork: false, delete: "yes" } }),
    ];
    const v = await opened();
    await showRow("cx");
    expect(inHead(copyText("history.row.resume"))!.title).toBe(copyText("history.resume.hintAgent", { agent: "Codex" }));
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(enumerateAccountModifiers).not.toHaveBeenCalled();
    const na = menuItem(copyText("history.resume.noAccounts", { agent: "Codex" }))!;
    expect((na as HTMLButtonElement).disabled).toBe(true);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await showRow("l");
    expect(inHead(copyText("history.row.switch"))).toBeTruthy();
    expect(inHead(copyText("history.resume.more"))).toBeUndefined();
    await showRow("bg");
    expect(inHead(copyText("history.row.resume"))!.getAttribute("aria-disabled")).toBe("true");
    expect(inHead(copyText("history.row.resume"))!.title).toBe(copyText("history.row.bgHint"));
    v.close();
  });
});
