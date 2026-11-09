/**
 * 设置窗「文件与数据」：一趟只问本机后端一次 `chores-merge {machines, client}`（它问到各台的 `data-report`、件数合成一份）· 两栏照每台那份成品画 ·
 * 角标与顶上那一行 ＝ 它合的那一份 ·
 * 那台说了没有 tmux ⇒ 恢复默认的「运行于」回落 · 改过你的文件［前往］带上那台去撤回那一处。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { WALL_CEILING_MS } from "../../../test-support/wall-ceiling";
import { fakeClipboard } from "../../../test-support/clipboard-fake";

const { disk, answers, opened, marks, seen, fileWin, localOpened, localRevealed, merges, SEEN } = vi.hoisted(() => ({
  /** 本机后端写好的上次读数那一格（一句认得出的字：界面照摆，不自己算时长）。 */
  SEEN: { atMs: 7, said: "采样于 夹具那一刻" },
  /** 交给本机后端 `chores-merge` 的每一趟问的那几台（替身照 `answers` 逐台答；`reply` 设了合计就照它，否则逐格相加）。 */
  merges: { asked: [] as unknown[][], reply: null as Record<string, number> | null },
  localOpened: [] as string[],
  localRevealed: [] as string[],
  seen: new Map<string, Record<string, unknown>>(),
  fileWin: [] as Array<{ host: unknown; at: unknown }>,
  /** `cfg` 是 monitor 的设置；`machines` 是本机后端那份机器表（盘上那几台，`machines-read` 照金样答）。 */
  disk: { cfg: {} as Record<string, unknown>, machines: [] as Record<string, unknown>[] },
  answers: new Map<string, unknown>(),
  opened: vi.fn(),
  marks: [] as Array<{ origin: string; args: unknown }>,
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: () => Promise.resolve(structuredClone(disk.cfg)),
    footprint_client_facts: () => Promise.resolve({ home: "/h", path: null }),
    clipboard_write: async (a: { text: string }) => (await import("../../../test-support/clipboard-fake")).viaFake(a),
    os_open: (w: { kind: string; path?: string; url?: string }) => {
      if (w.kind === "url") opened(w.url);
      else if (w.kind === "path") localOpened.push(w.path ?? "");
      else if (w.kind === "reveal") localRevealed.push(w.path ?? "");
      return Promise.resolve();
    },
  },
}));
vi.mock("../../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      if (op === "chores-merge") {
        if (origin !== "<local>") return Promise.reject(new Error("只问本机后端"));
        const req = JSON.parse(new TextDecoder().decode(body)) as { machines?: unknown; client: unknown };
        if (req.client === undefined) return Promise.reject(new Error("本机那几条事实没带"));
        if ("machines" in req) return Promise.reject(new Error("名单不是入参：本机后端照自己的机器表问"));
        // 本机后端照它自己的机器表问：本机 ＋ 盘上那几台里「连接这台」开着的（名字照表，同 `machines-read`）。
        const machines = ["<local>", ...disk.machines.filter((h) => h.connect !== false).map((h) => String(h.label || h.host))];
        merges.asked.push(machines);
        const sum: Record<string, number> = {};
        const rows = machines.map((o) => {
          const a = answers.get(o) as { counts: Record<string, number> } | undefined;
          if (a === undefined) return { origin: o, said: `${o} 断开` };
          for (const [k, v] of Object.entries(a.counts)) sum[k] = (sum[k] ?? 0) + v;
          return { origin: o, report: a };
        });
        const total = merges.reply ?? { badge: 0, open: 0, must: 0, decide: 0, install: 0, optional: 0, done: 0, ...sum };
        return Promise.resolve(new TextEncoder().encode(JSON.stringify({ total, machines: rows })));
      }
      if (op === "chores-mark") {
        marks.push({ origin, args: JSON.parse(new TextDecoder().decode(body)) });
        return Promise.resolve(new TextEncoder().encode("{}"));
      }
      // 本机后端那份上次值（`last-seen-*`）：照后端的样子记在内存里。
      if (op === "last-seen-write" || op === "last-seen-read") {
        const a = JSON.parse(new TextDecoder().decode(body)) as { origin: string; kind?: string; value?: unknown };
        if (origin !== "<local>") return Promise.reject(new Error("上次值只问本机后端"));
        const mine = seen.get(a.origin) ?? {};
        // `data` 那一份后端按回包类型核过、读回来带 `state`（这里只记合法的那一形；读不懂那一形另由判据塞）。
        if (op === "last-seen-write") seen.set(a.origin, { ...mine, [a.kind!]: a.kind === "data" ? { state: "kept", seen: SEEN, value: a.value } : { seen: SEEN, value: a.value } });
        const out = op === "last-seen-read" ? { accounts: mine.accounts ?? null, data: mine.data ?? null } : { atMs: 1 };
        return Promise.resolve(new TextEncoder().encode(JSON.stringify(out)));
      }
      // 机器表住本机后端（`machines-read`）：照这块盘上摆的那几台，交金样里录好的那一答（后端现打，`machines-fake.ts`）。
      if (op === "machines-read") {
        if (origin !== "<local>") return Promise.reject(new Error("机器表只问本机后端"));
        return import("../machines-fake").then(({ recordedMachines }) => {
          const got = recordedMachines("machines-read", disk.machines);
          if (!("reply" in got)) throw new Error("读口不拒");
          return new TextEncoder().encode(JSON.stringify(got.reply));
        });
      }
      if (op !== "data-report") return Promise.reject(new Error(`没料到 ${op}`));
      const a = answers.get(origin);
      if (a === undefined) return Promise.reject(new Error(`${origin} 断开`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(a)));
    },
  },
}));
// 各台「恢复到 tmux 里」那一格住那台后端（`prefs-read`）：桩成都开着。
vi.mock("../../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async () => ({ state: "ok", reason: null, detail: null, path: null, prefs: { launchers: {}, resumeWhere: "tmux", models: {}, permissionModes: {}, contextLimits: {} } }),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/file-window", () => ({ openOnMachine: (host: unknown, at: unknown) => (fileWin.push({ host, at }), Promise.resolve()) }));

import { DataPage } from "../../../../src/frontend/ui/settings/data-page";
import { recallDataReport } from "../../../../src/frontend/ui/settings/data-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import { noteMachine } from "../../../../src/frontend/ui/ipc/chan-caller";

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
  diff: null,
  copy: null,
  whole: null,
  wholeCovers: [],
  file: null,
  go: null,
  howUrl: null,
  action: "copySnippet",
  tag: { text: copyText("dataPage.kind.optional"), kind: "optional" },
  tone: "muted",
  ...over,
});

const counts = (over: Record<string, number> = {}) => ({ badge: 0, open: 0, must: 0, decide: 0, install: 0, optional: 0, done: 0, ...over });

const report = (over: Record<string, unknown> = {}) => ({
  home: "/h",
  changedFiles: [{ path: "~/.bashrc", what: copyText("rsToolRegistry.tools.ccmName"), undo: { page: "machine", tab: "config", anchor: "connect-terminal" } }],
  todo: [
    chore({ id: "install:claude-cli", kind: "install", name: "Claude Code", action: "how", howUrl: "https://example.invalid/claude", tag: { text: copyText("dataPage.install.kind"), kind: "install" }, tone: "warn" }),
    chore({ id: "install:git", kind: "installOptional", name: "git", action: "how", tag: { text: copyText("dataPage.install.kindOptional"), kind: "installOptional" } }),
  ],
  tmux: false,
  counts: counts({ badge: 1, open: 2, install: 1, optional: 1 }),
  rank: 0,
  own: [
    { id: "profiles", what: copyText("rsDataPaths.backend.profiles"), path: "~/.cc-monitor/profiles.toml", dir: false, class: "truth", exists: true, size: 2048 },
    { id: "accounts", what: copyText("rsDataPaths.backend.accounts"), path: "~/.cc-monitor/accounts", dir: true, class: "truth", exists: true, size: null },
    { id: "bin", what: copyText("rsDataPaths.backend.bin"), path: "~/.cc-monitor/bin", dir: true, class: "cache", exists: false, size: null },
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
  localOpened.length = 0;
  localRevealed.length = 0;
  opened.mockClear();
  marks.length = 0;
  merges.asked.length = 0;
  merges.reply = null;
  clipFake = fakeClipboard();
  disk.cfg = {};
  disk.machines = [{ label: "devbox", host: "d.lan", user: "u", port: 22 }, { label: "gpu", host: "g.lan", user: "u", port: 22 }];
});

describe("上次值", () => {
  it("★ 后端核过的那一份照用；后端说读不懂（存档不是这一版）⇒ 当没有上次值，不拿原样画", async () => {
    seen.set("devbox", { data: { state: "kept", seen: SEEN, value: report() } });
    expect(await recallDataReport("devbox")).toEqual({ report: report(), seen: SEEN });
    seen.set("devbox", { data: { state: "unreadable", seen: SEEN } });
    expect(await recallDataReport("devbox")).toBeNull();
  });
});

describe("待办", () => {
  it("★ 每台一段、照成品画要装两档；连不上的那台说离线未检查；角标 ＝ 读到的各台 counts 交本机后端合的 badge", async () => {
    answers.set(LOCAL, report());
    answers.set("devbox", report({ counts: counts({ badge: 2 }), todo: [] }));
    const { page, badge } = mount();
    page.loadNow();
    // 机器表那一问的替身头一回要现载夹具模块（`config-patch-fake`），几拍空转等不到 ⇒ 等到那一问落下。
    await vi.waitFor(() => expect(merges.asked.length).toBeGreaterThan(0), { timeout: WALL_CEILING_MS });
    await settle();
    expect(merges.asked, "一趟只问本机后端一次、交机器表那几台").toEqual([[LOCAL, "devbox", "gpu"]]);
    expect(badge.at(-1)).toBe(3);
    const local = page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL}"]`)!;
    expect([...local.querySelectorAll<HTMLElement>("[data-chore]")].map((r) => r.dataset.chore)).toEqual(["install:claude-cli", "install:git"]);
    expect(local.textContent).toContain(copyText("dataPage.install.kind"));
    expect(local.textContent).toContain(copyText("dataPage.install.kindOptional"));
    const gpu = page.element.querySelector<HTMLElement>('[data-machine="gpu"]')!;
    expect(gpu.textContent).toContain(copyText("dataPage.placed.offline", { machine: "gpu" }));
    expect(page.element.querySelector('[data-machine="devbox"]')!.textContent).toContain(copyText("dataPage.chores.noneOn", { machine: "devbox" }));
  });

  it("★ 几台照各台成品的先后键排（读不成的排最后）；顶上那一行照本机后端合的那一份画、段头照那台的 counts，界面不数", async () => {
    answers.set(LOCAL, report({ rank: 2, counts: counts() }));
    answers.set("devbox", report({ rank: 1, counts: counts({ open: 5, optional: 5 }) }));
    merges.reply = counts({ badge: 4, open: 9, must: 3, decide: 1, done: 2 });
    const { page, badge } = mount();
    page.loadNow();
    await settle();
    expect([...page.element.querySelectorAll<HTMLElement>(".data-machine")].map((e) => e.dataset.machine)).toEqual(["devbox", LOCAL, "gpu"]);
    const top = page.element.querySelector<HTMLElement>(".data-summary")!;
    expect(top.textContent).toContain(copyText("dataPage.chores.open", { n: 9 }));
    expect([...top.querySelectorAll<HTMLElement>("[data-kind]:not(button)")].map((t) => [t.dataset.kind, t.textContent])).toEqual([
      ["must", copyText("dataPage.chores.countMust", { n: 3 })],
      ["decide", copyText("dataPage.chores.countDecide", { n: 1 })],
      ["done", copyText("dataPage.chores.countDone", { n: 2 })],
    ]);
    expect(badge.at(-1)).toBe(4);
    const dev = page.element.querySelector<HTMLElement>('[data-machine="devbox"] .data-machine-head')!;
    expect([...dev.querySelectorAll<HTMLElement>("[data-kind]:not(button)")].map((t) => t.textContent)).toEqual([copyText("dataPage.chores.countOptional", { n: 5 })]);
  });

  it("★ 小标签的字与类、状态点的色照成品那几格摆，不照类 · 态自己选（夹具故意给对不上类 · 态的）", async () => {
    answers.set(LOCAL, report({ todo: [chore({ id: "odd", kind: "optional", state: "done", tag: { text: "标签·夹具", kind: "must" }, tone: "warn" })] }));
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = page.element.querySelector<HTMLElement>('[data-chore="odd"]')!;
    const t = row.querySelector<HTMLElement>(".data-item-title [data-kind]")!;
    expect([t.dataset.kind, t.textContent]).toEqual(["must", "标签·夹具"]);
    expect(row.querySelector<HTMLElement>(".data-dot")!.dataset.tone).toBe("warn");
  });

  it("［安装方法］开的是那份成品给的链接；没有链接的那件不摆这颗", async () => {
    answers.set(LOCAL, report());
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    row("install:claude-cli").querySelector("button")!.click();
    expect(opened).toHaveBeenCalledWith("https://example.invalid/claude");
    expect(row("install:git").querySelector("button"), "没有链接还摆一颗点不出东西的按钮").toBeNull();
  });

  it("★ 复制 ⇒ 剪贴板是那一件交来的整份、那件标「已复制 · 等你贴」；整份盖两件 ⇒ 两件一起标；那台认出已做 ⇒ 段头说一句、收进「已做的 N」", async () => {
    const relay = chore({ id: "relay", name: "实时显示", copy: "K=SECRET", whole: "{SECRET}", wholeCovers: ["relay", "cc-bus-hooks"], why: "为什么", steps: ["打开 f"], diff: { rows: [{ kind: "ctx", text: "{", oldNo: 3, newNo: 3 }, { kind: "add", text: "K=SECRET", newNo: 4 }], shown: 2, truncated: false, added: 1, removed: 0, stat: "+1" } });
    const hooks = chore({ id: "cc-bus-hooks", name: "收信", copy: "h", whole: "{SECRET}", wholeCovers: ["relay", "cc-bus-hooks"], why: "w" });
    answers.set(LOCAL, report({ todo: [relay, hooks] }));
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    row("relay").querySelector<HTMLButtonElement>("[aria-expanded]")!.click();
    const gutter = [...row("relay").querySelectorAll(".chore-diff-line")].map((l) => `${(l as HTMLElement).dataset.op} ${l.querySelector(".chore-diff-n")?.textContent}`);
    expect(gutter, "行差照那台交来的行画：没变的带改前行号，加的那行是「+」").toEqual(["ctx 3", "add +"]);
    [...row("relay").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.copyWholeN", { n: 2 }))!.click();
    await settle();
    expect(clipFake.written).toEqual(["{SECRET}"]);
    expect(row("relay").textContent).toContain(copyText("dataPage.state.copied"));
    expect(row("cc-bus-hooks").textContent).toContain(copyText("dataPage.state.copied"));
    answers.set(LOCAL, report({ todo: [{ ...relay, state: "done" }, hooks] }));
    page.loadNow();
    await settle();
    const local = page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL}"]`)!;
    expect(local.textContent).toContain(copyText("dataPage.chores.recognized", { names: "实时显示" }));
    expect(local.textContent).toContain(copyText("dataPage.chores.doneN", { n: 1 }));
  });

  it("★ 可选的［不用了］⇒ 交那台记下（chores-mark decline）再重读；不做的那件［还是要做］⇒ undecline；要做的没有［不用了］", async () => {
    answers.set(LOCAL, report({ todo: [chore({ id: "dead:/h/.bashrc", why: "w" }), chore({ id: "stale-ccm", kind: "must", action: "copyCommand", copy: "rm x", why: "w" })] }));
    const { page } = mount();
    page.loadNow();
    await settle();
    const row = (id: string) => page.element.querySelector<HTMLElement>(`[data-chore="${id}"]`)!;
    for (const id of ["dead:/h/.bashrc", "stale-ccm"]) row(id).querySelector<HTMLButtonElement>("[aria-expanded]")!.click();
    expect([...row("stale-ccm").querySelectorAll("button")].some((b) => b.textContent === copyText("dataPage.chore.decline"))).toBe(false);
    [...row("dead:/h/.bashrc").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.decline"))!.click();
    await settle();
    expect(marks).toEqual([{ origin: LOCAL, args: { op: "decline", id: "dead:/h/.bashrc" } }]);
    answers.set(LOCAL, report({ todo: [chore({ id: "dead:/h/.bashrc", state: "declined" })] }));
    page.loadNow();
    await settle();
    page.element.querySelector<HTMLElement>(`[data-machine="${LOCAL}"] [aria-expanded]`)!.click();
    [...row("dead:/h/.bashrc").querySelectorAll("button")].find((b) => b.textContent === copyText("dataPage.chore.undecline"))!.click();
    await settle();
    expect(marks.at(-1)).toEqual({ origin: LOCAL, args: { op: "undecline", id: "dead:/h/.bashrc" } });
  });

  it("［去定…］带上那台去别名页那一项；［先装 cc-bus］去扩展页", async () => {
    answers.set(LOCAL, report({ todo: [chore({ id: "clash:cc", kind: "decide", action: "decide", go: { page: "machine", tab: "config", anchor: "clash" } }), chore({ id: "cc-bus-hooks", state: "blocked", action: "installFirst", go: { page: "ext" } })] }));
    const { page, went } = mount();
    page.loadNow();
    await settle();
    page.element.querySelector<HTMLElement>('[data-chore="clash:cc"] button')!.click();
    page.element.querySelector<HTMLElement>('[data-chore="cc-bus-hooks"] button')!.click();
    expect(went).toEqual([{ machine: LOCAL, tab: "config", anchor: "clash" }, { page: "ext", anchor: undefined }]);
  });
});

describe("cc-monitor 放了什么", () => {
  it("★ 改过你的文件：路径 ＋ 改了什么 · 撤回在哪；［前往］带上那台去那一栏那一节", async () => {
    answers.set(LOCAL, report());
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
    answers.set(LOCAL, report());
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
    chip(copyText("control.machine.unsure")).click();
    expect(block.hidden).toBe(false);
  });

  it("★ 机器名在画的那一刻取：那台的成品晚到（先画成「机器不明」）⇒ 下一次画换成成品里的名字", async () => {
    answers.set(LOCAL, report());
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    const chips = () => [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].map((b) => b.textContent);
    expect(chips()[0]).toBe(copyText("control.machine.unsure"));
    noteMachine(LOCAL, { name: "workstation", offlineSaid: "" });
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "gpu")!.click();
    expect(chips()[0]).toBe("workstation");
  });

  it("★ 远端那台的「cc-monitor 的文件」照那台 own 排：名字 · 是什么 · 删了会怎样 · 大小；不在的不给打开；打开 ＝ 在文件窗口里打开那一样", async () => {
    answers.set(LOCAL, report());
    answers.set("devbox", report({ home: "/home/u", own: [...report().own, { id: "fresh", what: "编的一句：界面照抄", path: "~/.cc-monitor/fresh", dir: true, class: "cache", exists: false, size: null }] }));
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    const remote = pane.querySelector<HTMLElement>(".data-own-files")!;
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "devbox")!.click();
    expect(remote.hidden).toBe(false);
    const rows = [...remote.querySelectorAll<HTMLElement>("[data-own]")];
    expect(rows.map((r) => r.dataset.own)).toEqual(["profiles", "accounts", "bin", "fresh"]);
    expect(rows[3].textContent, "认不认得这个 id 都照抄后端那一句").toContain("编的一句：界面照抄");
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

  it("★ 本机那一块「cc-monitor 的文件」也照本机后端 data-report 的 own 排（与远端同一套行）；打开 ＝ 本机打开 · 在文件夹中显示；不在的两颗都不给", async () => {
    answers.set(LOCAL, report({ home: "/l", own: [...report().own, { id: "fresh", what: "编的一句：本机也照抄", path: "~/.cc-monitor/fresh.json", dir: false, class: "cache", exists: true, size: 7 }] }));
    const own = document.createElement("div");
    own.dataset.host = "monitor-own";
    const page = new DataPage({ claudeDir: document.createElement("div"), ownFiles: own, goTo: () => {}, setBadge: () => {} });
    document.body.appendChild(page.element);
    page.openTab("placed");
    page.loadNow();
    await settle();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    const box = pane.querySelector<HTMLElement>(".data-own-files")!;
    expect(box.hidden, "本机 chip 上也出这一块").toBe(false);
    const rows = [...box.querySelectorAll<HTMLElement>("[data-own]")];
    expect(rows.map((r) => r.dataset.own)).toEqual(["profiles", "accounts", "bin", "fresh"]);
    expect(rows[3].textContent, "本机那一句也照抄后端").toContain("编的一句：本机也照抄");
    expect(rows[0].textContent).toContain(copyText("data.class.keep"));
    expect(rows[2].querySelectorAll("button:not([disabled])").length, "不在的那一样没有可打开的").toBe(0);
    const btn = (r: HTMLElement, text: string) => [...r.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text)!;
    btn(rows[0], copyText("mobTerms.row.attach")).click();
    btn(rows[0], copyText("data.item.reveal")).click();
    await settle();
    expect(localOpened).toEqual(["/l/.cc-monitor/profiles.toml"]);
    expect(localRevealed).toEqual(["/l/.cc-monitor/profiles.toml"]);
    expect(fileWin, "本机不开文件窗口").toEqual([]);
    // 宿主交进来的 monitor 那一块照旧挂在后面（界面自己那几份：数据目录 · 网页引擎 · 本地存储）。
    const host = pane.querySelector<HTMLElement>('[data-host="monitor-own"]')!;
    expect(host.closest<HTMLElement>(".data-local-only")!.hidden).toBe(false);
    expect(box.compareDocumentPosition(host) & Node.DOCUMENT_POSITION_FOLLOWING, "后端那几样在前、界面那几份在后").toBeTruthy();
  });

  it("★ 连不上的那台：本机后端记着上次读成的那一份 ⇒ 照上次的画 ＋ 警告条说多旧（跨重启：上次值不住界面）", async () => {
    answers.set(LOCAL, report());
    answers.set("devbox", report({ changedFiles: [{ path: "~/.zshrc", what: "上次那一份", undo: null }] }));
    const first = mount();
    first.page.loadNow();
    await settle();
    expect(seen.get("devbox")?.data, "读成了那台 ⇒ 交本机后端记下").toBeTruthy();
    expect(seen.has(LOCAL), "本机那台不记").toBe(false);
    // 「重启」：那台这回断开。
    answers.delete("devbox");
    document.body.replaceChildren();
    const { page } = mount();
    page.openTab("placed");
    page.loadNow();
    await settle();
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "devbox")!.click();
    const pane = page.element.querySelector<HTMLElement>('[data-pane="placed"]')!;
    expect(pane.textContent).toContain("~/.zshrc");
    expect(pane.textContent).toContain(copyText("dataPage.placed.stale", { machine: "devbox", seen: SEEN.said }));
    expect(pane.querySelectorAll(".data-own-files [data-own]").length).toBe(3);
    expect([...pane.querySelectorAll<HTMLButtonElement>(".data-own-files [data-own] button")].every((b) => b.disabled), "断开的那台打不开文件窗口 ⇒ 置灰").toBe(true);
    // 从没读成过的那台照旧说读不到。
    [...page.element.querySelectorAll<HTMLButtonElement>(".data-chip")].find((b) => b.textContent === "gpu")!.click();
    expect(pane.textContent).toContain(copyText("dataPage.placed.offline", { machine: "gpu" }));
  });
});
