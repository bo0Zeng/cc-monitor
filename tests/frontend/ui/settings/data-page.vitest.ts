/**
 * 设置窗「文件与数据」：那台后端的 `data-report` 严格收 · 两栏照那份成品画 · 角标 ＝ 各台 `chores` 相加（读不到的不算）·
 * 那台说了没有 tmux ⇒ 恢复默认的「运行于」回落 · 改过你的文件［前往］带上那台去撤回那一处。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fakeClipboard } from "../../../test-support/clipboard-fake";

const { disk, answers, opened, marks, seen, fileWin } = vi.hoisted(() => ({
  seen: new Map<string, Record<string, { atMs: number; value: unknown }>>(),
  fileWin: [] as Array<{ host: unknown; at: unknown }>,
  disk: { cfg: {} as Record<string, unknown> },
  answers: new Map<string, unknown>(),
  opened: vi.fn(),
  marks: [] as Array<{ origin: string; args: unknown }>,
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: () => Promise.resolve(structuredClone(disk.cfg)),
    footprint_client_facts: () => Promise.resolve({ home: "/h", path: null }),
    clipboard_write: async (a: { text: string }) => (await import("../../../test-support/clipboard-fake")).viaFake(a),
  },
}));
vi.mock("../../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      if (op === "chores-mark") {
        marks.push({ origin, args: JSON.parse(new TextDecoder().decode(body)) });
        return Promise.resolve(new TextEncoder().encode("{}"));
      }
      // 本机后端那份上次值（`last-seen-*`）：照后端的样子记在内存里。
      if (op === "last-seen-write" || op === "last-seen-read") {
        const a = JSON.parse(new TextDecoder().decode(body)) as { origin: string; kind?: string; value?: unknown };
        if (origin !== "<local>") return Promise.reject(new Error("上次值只问本机后端"));
        const mine = seen.get(a.origin) ?? {};
        if (op === "last-seen-write") seen.set(a.origin, { ...mine, [a.kind!]: { atMs: Date.now(), value: a.value } });
        const out = op === "last-seen-read" ? { accounts: mine.accounts ?? null, data: mine.data ?? null } : { atMs: 1 };
        return Promise.resolve(new TextEncoder().encode(JSON.stringify(out)));
      }
      if (op !== "data-report") return Promise.reject(new Error(`没料到 ${op}`));
      const a = answers.get(origin);
      if (a === undefined) return Promise.reject(new Error(`${origin} 连不上`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(a)));
    },
  },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: opened, openPath: vi.fn() }));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/file-window", () => ({ openFileWindow: (host: unknown, at: unknown) => (fileWin.push({ host, at }), Promise.resolve()) }));

import { DataPage } from "../../../../src/frontend/ui/settings/data-page";
import { decodeDataReport } from "../../../../src/frontend/ui/settings/data-reads";
import { defaultPick } from "../../../../src/frontend/ui/resume-menu";
import { setResumeInTmux } from "../../../../src/frontend/ui/resume-defaults";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { __resetLastSeenForTests } from "../../../../src/frontend/ui/last-seen";

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

const chore = (over: Record<string, unknown>) => ({
  id: "x",
  kind: "optional",
  state: "todo",
  name: "名字",
  loc: "",
  said: "现状",
  why: "",
  steps: [],
  diff: [],
  copy: null,
  whole: null,
  wholeCovers: [],
  file: null,
  go: null,
  howUrl: null,
  mask: null,
  action: "copySnippet",
  ...over,
});

const report = (over: Record<string, unknown> = {}) => ({
  home: "/h",
  changedFiles: [{ path: "~/.bashrc", what: copyText("rsToolRegistry.tools.ccmName"), undo: { page: "machine", tab: "config", anchor: "connect-terminal" } }],
  todo: [
    chore({ id: "install:claude-cli", kind: "install", name: "Claude Code", action: "how", howUrl: "https://example.invalid/claude" }),
    chore({ id: "install:git", kind: "installOptional", name: "git", action: "how" }),
  ],
  tmux: false,
  chores: 1,
  own: [
    { id: "profiles", path: "~/.cc-monitor/profiles.toml", dir: false, class: "truth", exists: true, size: 2048 },
    { id: "accounts", path: "~/.cc-monitor/accounts", dir: true, class: "truth", exists: true, size: null },
    { id: "bin", path: "~/.cc-monitor/bin", dir: true, class: "cache", exists: false, size: null },
  ],
  ...over,
});

function mount(): { page: DataPage; badge: number[]; went: unknown[] } {
  const badge: number[] = [];
  const went: unknown[] = [];
  const page = new DataPage({
    claudeDir: document.createElement("div"),
    ownFiles: document.createElement("div"),
    goTo: (t) => went.push(t),
    setBadge: (n) => badge.push(n),
  });
  document.body.appendChild(page.element);
  return { page, badge, went };
}

let clipFake: ReturnType<typeof fakeClipboard>;
afterEach(() => clipFake.restore());

beforeEach(() => {
  document.body.replaceChildren();
  answers.clear();
  seen.clear();
  fileWin.length = 0;
  __resetLastSeenForTests();
  opened.mockClear();
  marks.length = 0;
  clipFake = fakeClipboard();
  disk.cfg = { remote: { hosts: [{ label: "devbox", host: "d.lan", user: "u", port: 22 }, { label: "gpu", host: "g.lan", user: "u", port: 22 }] } };
  setResumeInTmux(true);
});

describe("data-report 的应答严格收", () => {
  it("★ 照形状收；多一格 / 缺一格 / 类型不对 ⇒ 抛（不拿半份当整份画）", () => {
    expect(decodeDataReport(report()).chores).toBe(1);
    expect(() => decodeDataReport({ ...report(), extra: 1 })).toThrow();
    const { chores: _c, ...noChores } = report();
    expect(() => decodeDataReport(noChores)).toThrow();
    expect(() => decodeDataReport(report({ tmux: "no" }))).toThrow();
    expect(() => decodeDataReport(report({ todo: [chore({ kind: "maybe" })] }))).toThrow();
    expect(() => decodeDataReport(report({ todo: [chore({ state: "copied" })] })), "「已复制」只住界面").toThrow();
    expect(() => decodeDataReport(report({ todo: [{ ...chore({}), extra: 1 }] }))).toThrow();
    expect(() => decodeDataReport(report({ todo: [chore({ diff: [{ n: 0, op: "add", text: "" }] })] }))).toThrow();
    expect(() => decodeDataReport(report({ own: [{ id: "x", path: "~/.cc-monitor/x", dir: false, class: "maybe", exists: true, size: 1 }] })), "class 闭集").toThrow();
    expect(() => decodeDataReport(report({ own: [{ id: "x", path: "~/.cc-monitor/x", dir: false, class: "cache", exists: true, size: -1 }] }))).toThrow();
    const { own: _o, ...noOwn } = report();
    expect(() => decodeDataReport(noOwn)).toThrow();
  });
});

describe("待办", () => {
  it("★ 每台一段、照成品画要装两档；连不上的那台说离线未检查；角标 ＝ 读到的各台 chores 相加", async () => {
    answers.set(LOCAL_ORIGIN, report());
    answers.set("devbox", report({ chores: 2, todo: [] }));
    const { page, badge } = mount();
    page.loadNow();
    await settle();
    expect(badge.at(-1), "角标不是各台 chores 相加（读不到的那台不该算成 0 以外的数）").toBe(3);
    const local = page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL_ORIGIN}"]`)!;
    expect([...local.querySelectorAll<HTMLElement>("[data-chore]")].map((r) => r.dataset.chore)).toEqual(["install:claude-cli", "install:git"]);
    expect(local.textContent).toContain(copyText("dataPage.install.kind"));
    expect(local.textContent).toContain(copyText("dataPage.install.kindOptional"));
    const gpu = page.element.querySelector<HTMLElement>('[data-machine="gpu"]')!;
    expect(gpu.textContent).toContain(copyText("dataPage.chores.offline", { machine: "gpu" }));
    expect(page.element.querySelector('[data-machine="devbox"]')!.textContent).toContain(copyText("dataPage.chores.noneOn", { machine: "devbox" }));
  });

  it("［安装方法］开的是那份成品给的链接；没有链接的那件不摆这颗", async () => {
    answers.set(LOCAL_ORIGIN, report());
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    row("install:claude-cli").querySelector("button")!.click();
    expect(opened).toHaveBeenCalledWith("https://example.invalid/claude");
    expect(row("install:git").querySelector("button"), "没有链接还摆一颗点不出东西的按钮").toBeNull();
  });

  it("★ 复制 ⇒ 剪贴板是真内容（钥匙不遮）、那件标「已复制 · 等你贴」；整份盖两件 ⇒ 两件一起标；那台认出已做 ⇒ 段头说一句、收进「已做的 N」", async () => {
    const relay = chore({ id: "relay", name: "实时显示", copy: "K=SECRET", whole: "{SECRET}", wholeCovers: ["relay", "cc-bus-hooks"], mask: "SECRET", why: "为什么", steps: ["打开 f"], diff: [{ n: 3, op: "same", text: "{" }, { n: null, op: "add", text: "K=SECRET" }] });
    const hooks = chore({ id: "cc-bus-hooks", name: "收信", copy: "h", whole: "{SECRET}", wholeCovers: ["relay", "cc-bus-hooks"], why: "w" });
    answers.set(LOCAL_ORIGIN, report({ todo: [relay, hooks], chores: 0 }));
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    row("relay").querySelector<HTMLButtonElement>("[aria-expanded]")!.click();
    expect(row("relay").querySelector(".chore-diff")!.textContent, "显示时钥匙要遮住").not.toContain("SECRET");
    [...row("relay").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.copyWholeN", { n: 2 }))!.click();
    await settle();
    expect(clipFake.written).toEqual(["{SECRET}"]);
    expect(row("relay").textContent).toContain(copyText("dataPage.state.copied"));
    expect(row("cc-bus-hooks").textContent).toContain(copyText("dataPage.state.copied"));
    answers.set(LOCAL_ORIGIN, report({ todo: [{ ...relay, state: "done" }, hooks], chores: 0 }));
    page.loadNow();
    await settle();
    const local = page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL_ORIGIN}"]`)!;
    expect(local.textContent).toContain(copyText("dataPage.chores.recognized", { names: "实时显示" }));
    expect(local.textContent).toContain(copyText("dataPage.chores.doneN", { n: 1 }));
  });

  it("★ 可选的［不用了］⇒ 交那台记下（chores-mark decline）再重读；不做的那件［还是要做］⇒ undecline；要做的没有［不用了］", async () => {
    answers.set(LOCAL_ORIGIN, report({ todo: [chore({ id: "dead:/h/.bashrc", why: "w" }), chore({ id: "stale-ccm", kind: "must", action: "copyCommand", copy: "rm x", why: "w" })] }));
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    for (const id of ["dead:/h/.bashrc", "stale-ccm"]) row(id).querySelector<HTMLButtonElement>("[aria-expanded]")!.click();
    expect([...row("stale-ccm").querySelectorAll("button")].some((b) => b.textContent === copyText("dataPage.chore.decline"))).toBe(false);
    [...row("dead:/h/.bashrc").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.decline"))!.click();
    await settle();
    expect(marks).toEqual([{ origin: LOCAL_ORIGIN, args: { op: "decline", id: "dead:/h/.bashrc" } }]);
    answers.set(LOCAL_ORIGIN, report({ todo: [chore({ id: "dead:/h/.bashrc", state: "declined" })] }));
    page.loadNow();
    await settle();
    page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL_ORIGIN}"] [aria-expanded]`)!.click();
    [...row("dead:/h/.bashrc").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.undecline"))!.click();
    await settle();
    expect(marks.at(-1)).toEqual({ origin: LOCAL_ORIGIN, args: { op: "undecline", id: "dead:/h/.bashrc" } });
  });

  it("［去定…］带上那台去别名页那一项；［先装 cc-bus］去扩展页", async () => {
    answers.set(LOCAL_ORIGIN, report({ todo: [chore({ id: "clash:cc", kind: "decide", action: "decide", go: { page: "machine", tab: "config", anchor: "clash" } }), chore({ id: "cc-bus-hooks", state: "blocked", action: "installFirst", go: { page: "ext" } })] }));
    const { page, went } = mount();
    page.loadNow();
    await settle();
    page.element.querySelector<HTMLElement>('[data-chore="clash:cc"] button')!.click();
    page.element.querySelector<HTMLElement>('[data-chore="cc-bus-hooks"] button')!.click();
    expect(went).toEqual([{ machine: LOCAL_ORIGIN, tab: "config", anchor: "clash" }, { page: "ext", anchor: undefined }]);
  });

  it("★ 那台说了没有 tmux ⇒ 恢复默认的「运行于」那台回落不用 tmux", async () => {
    answers.set(LOCAL_ORIGIN, report({ tmux: true }));
    answers.set("devbox", report({ tmux: false }));
    const { page } = mount();
    page.loadNow();
    await settle();
    expect(defaultPick("devbox").tmux).toBe(false);
    expect(defaultPick(LOCAL_ORIGIN).tmux).toBe(true);
  });
});

describe("cc-monitor 放了什么", () => {
  it("★ 改过你的文件：路径 ＋ 改了什么 · 撤回在哪；［前往］带上那台去那一栏那一节", async () => {
    answers.set(LOCAL_ORIGIN, report());
    answers.set("devbox", report());
    const { page, went } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "devbox")!.click();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    expect(pane.textContent).toContain("~/.bashrc");
    expect(pane.textContent).toContain(copyText("dataPage.changed.line", { what: copyText("rsToolRegistry.tools.ccmName"), where: copyText("dataPage.undo.config") }));
    [...pane.querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.changed.go"))!.click();
    expect(went).toEqual([{ machine: "devbox", tab: "config", anchor: "connect-terminal" }]);
  });

  it("本机那两块只在本机 chip 上出现；连不上的那台给一条离线条 ＋［重试］", async () => {
    answers.set(LOCAL_ORIGIN, report());
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    const chip = (name: string) => [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === name)!;
    chip("gpu").click();
    expect(pane.textContent).toContain(copyText("dataPage.placed.offline", { machine: "gpu" }));
    const claudeHead = [...pane.querySelectorAll<HTMLElement>(".data-section-head")].find((h) => h.textContent?.includes(copyText("dataPage.claudeDir.title")))!;
    const block = claudeHead.closest<HTMLElement>(".data-local-only")!;
    expect(block.hidden, "远端那台不该出本机的 Claude 目录").toBe(true);
    chip(copyText("remote.cards.local")).click();
    expect(block.hidden).toBe(false);
  });

  it("★ 远端那台的「cc-monitor 的文件」照那台 own 排：名字 · 是什么 · 删了会怎样 · 大小；不在的不给打开；打开 ＝ 在文件窗口里打开那一样", async () => {
    answers.set(LOCAL_ORIGIN, report());
    answers.set("devbox", report({ home: "/home/u" }));
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    const remote = pane.querySelector<HTMLElement>(".data-remote-only")!;
    expect(remote.hidden, "本机 chip 上不出远端那一块").toBe(true);
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "devbox")!.click();
    expect(remote.hidden).toBe(false);
    const rows = [...remote.querySelectorAll<HTMLElement>("[data-own]")];
    expect(rows.map((r) => r.dataset.own)).toEqual(["profiles", "accounts", "bin"]);
    expect(rows[0].textContent).toContain("profiles.toml");
    expect(rows[0].textContent).toContain(copyText("rsDataPaths.backend.profiles"));
    expect(rows[0].textContent).toContain(copyText("data.class.keep"));
    expect(rows[1].textContent).toContain("accounts/");
    expect(rows[2].textContent).toContain(copyText("data.class.disposable"));
    expect(rows[2].querySelector("button")!.disabled, "不在的那一样没有可打开的").toBe(true);
    rows[0].querySelector("button")!.click();
    await settle();
    expect(fileWin.map((f) => f.at)).toEqual([{ revealFile: "/home/u/.cc-monitor/profiles.toml" }]);
  });

  it("★ 连不上的那台：本机后端记着上次读成的那一份 ⇒ 照上次的画 ＋ 警告条说多旧（跨重启：上次值不住界面）", async () => {
    answers.set(LOCAL_ORIGIN, report());
    answers.set("devbox", report({ changedFiles: [{ path: "~/.zshrc", what: "上次那一份", undo: null }] }));
    const first = mount();
    first.page.loadNow();
    await settle();
    expect(seen.get("devbox")?.data, "读成了那台 ⇒ 交本机后端记下").toBeTruthy();
    expect(seen.has(LOCAL_ORIGIN), "本机那台不记").toBe(false);
    // 「重启」：界面那份记忆清掉，那台这回连不上。
    __resetLastSeenForTests();
    answers.delete("devbox");
    document.body.replaceChildren();
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "devbox")!.click();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    expect(pane.textContent).toContain("~/.zshrc");
    expect(pane.textContent).toContain(copyText("dataPage.placed.stale", { machine: "devbox", ago: copyText("acctPage.ago.minutes", { n: 0 }) }));
    expect(pane.querySelectorAll(".data-remote-only [data-own]").length).toBe(3);
    expect([...pane.querySelectorAll<HTMLButtonElement>(".data-remote-only [data-own] button")].every((b) => b.disabled), "连不上的那台打不开文件窗口 ⇒ 置灰").toBe(true);
    // 从没读成过的那台照旧说读不到。
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "gpu")!.click();
    expect(pane.textContent).toContain(copyText("dataPage.placed.offline", { machine: "gpu" }));
  });
});
