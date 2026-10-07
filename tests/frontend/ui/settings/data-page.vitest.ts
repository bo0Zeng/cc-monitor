/**
 * 设置窗「文件与数据」：那台后端的 `data-report` 严格收 · 两栏照那份成品画 · 角标 ＝ 各台 `chores` 相加（读不到的不算）·
 * 那台说了没有 tmux ⇒ 恢复默认的「运行于」回落 · 改过你的文件［前往］带上那台去撤回那一处。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { disk, answers, opened, marks, clip } = vi.hoisted(() => ({
  disk: { cfg: {} as Record<string, unknown> },
  answers: new Map<string, unknown>(),
  opened: vi.fn(),
  marks: [] as Array<{ origin: string; args: unknown }>,
  clip: [] as string[],
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: () => Promise.resolve(structuredClone(disk.cfg)),
    footprint_client_facts: () => Promise.resolve({ home: "/h", path: null }),
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
      if (op !== "data-report") return Promise.reject(new Error(`没料到 ${op}`));
      const a = answers.get(origin);
      if (a === undefined) return Promise.reject(new Error(`${origin} 连不上`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(a)));
    },
  },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: opened, openPath: vi.fn() }));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { DataPage } from "../../../../src/frontend/ui/settings/data-page";
import { decodeDataReport } from "../../../../src/frontend/ui/settings/data-reads";
import { defaultPick } from "../../../../src/frontend/ui/resume-menu";
import { setResumeInTmux } from "../../../../src/frontend/ui/resume-defaults";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

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

beforeEach(() => {
  document.body.replaceChildren();
  answers.clear();
  opened.mockClear();
  marks.length = 0;
  clip.length = 0;
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: (t: string) => (clip.push(t), Promise.resolve()) } });
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
  });
});

describe("要你动手", () => {
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
    expect(clip).toEqual(["{SECRET}"]);
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
});
