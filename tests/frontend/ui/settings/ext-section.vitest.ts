/**
 * 要求：设置「扩展」页 —— 一张表（搜索 · 种类筛选 · 一行一个条目 · 每台一个点（已装 · 内容不同 · 未装 · 仅项目四个图标），悬停是机器名 · 新见到的行上一个「新」·
 * 有备注的行带一个记号）＋ 一个抽屉（顶部备注；每台一行展开成它的各处，各自的态与「卸载」；机器那一行一个「装到…」）
 * ＋ 一张确认卡（「装到哪」由用户选：后端给的各处，不能选的显示但置灰、旁注为什么；一个确认按钮；后端答 stale 就在卡上说一句、给「重看」）；
 * cc-bus 那一行：内置备注下面每台一行钩子状态与要加的内容，都由那台后端给（界面不读那份配置文件）。
 * 做完之后重读那张表，那一格的点自己变。界面只画、只问：不比较指纹、不做判定（扫描判据在本文件末尾）。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { ExtSection } from "../../../../src/frontend/ui/settings/ext-section";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { stripComments } from "../../../test-support/strip-comments";
import { noteMachine } from "../../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";

// 本机那一列的名字照表里那一台的成品 `name`（后端经 `machine_word` 写好），界面不自取、也不另走机器成品那条路：
// 机器成品那边记一个不同的字，画出来的要是表里那一个。
const HERE_NAME = "〈表里本机那一台的名字〉";
noteMachine(LOCAL, { name: "〈机器成品里本机的名字〉", offlineSaid: "" });

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const ops = () => invokeMock.mock.calls.filter(([c]) => c === "chan_call").map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 8; i++) await tick();
};

const user = { level: "user" } as const;
const proj = { level: "project", dir: "/g/p" } as const;
const machines = [
  { key: null, here: true, reachable: true, name: HERE_NAME, projects: ["/h/p"] },
  { key: "laptop", here: false, reachable: true, name: "laptop", projects: ["/g/p"] },
];
/** 后端写好的那几句（夹具：界面只原样画，不自己按态取文）。 */
const SAID: Record<string, string> = { same: "〈装着〉", differs: "〈不一样〉", missing: "〈没有〉", project: "〈只在项目〉", off: "〈关着〉" };
const KIND_SAID: Record<string, string> = { skill: "〈S〉", mcp: "〈M〉", plugin: "〈P〉" };
const LEGEND = ["same", "differs", "missing", "project"].map((state) => ({ state, said: `〈图例 ${state}〉` }));
const place = (at: object, state: string, uninstall = false, note: string | null = null, dir: string | null = null) => ({ at, state, said: SAID[state], dir, uninstall, note });
const cell = (state: string, places: object[], bring: unknown = null, note: string | null = null) => ({ state, said: SAID[state], places, bring, note });
const targets = (globalOk = true, globalNote: string | null = null) => [
  { at: user, ok: globalOk, note: globalNote },
  { at: proj, ok: true, note: null },
];
const bring = (over: Record<string, unknown> = {}) => ({ from: null, fromName: "本机", scope: { from: user, to: user }, targets: targets(), ...over });
const row = (kind: string, name: string, cells: unknown[], over: Record<string, unknown> = {}) => ({ kind, kindSaid: KIND_SAID[kind], name, about: null, detail: [], new: false, known: null, note: null, cells, ...over });
const here = cell("same", [place(user, "same", true)]);
const demoMissing = cell("missing", [place(user, "missing")], bring());
const listWith = (there: unknown, extra: ReturnType<typeof row>[] = []) => ({
  machines,
  problems: [],
  legend: LEGEND,
  knownTitle: "〈认得〉",
  rows: [
    row("skill", "demo", [here, there], { about: "does demo", detail: [{ label: copyText("mobDrawer.item.files"), value: copyText("machineList.summary.count", { n: "1" }) }], new: true }),
    row("mcp", "fs", [cell("same", [place(user, "same", false, copyText("agentWindow.status.plain"))], bring({ scope: { from: user, to: proj }, targets: targets(false, "全局的只读") })), cell("missing", [place(user, "missing")], null, "没项目")], {
      about: "npx fs",
    }),
    ...extra,
  ],
});
const card = { kind: "skill", name: "demo", path: "/g/.claude/skills/demo", writes: ["SKILL.md"], unchanged: false, suspects: [], stop: null, config: null, slots: [], tokens: { source: "s", target: "t" } };
const synced = { self: "h", synced: [], reach: [{ origin: "laptop", machine: "g" }] };
type TrustFixture = { trusted: boolean; said: string; tone: string; file: string; line: string | null };
/** 一家那一份（台架：一家「甲」，钩子文件 `/h/a/hooks.json`；信任那一格可给）。 */
const family = (snippet: string | null, trust: TrustFixture | null = null) => ({
  kind: "fake",
  name: "甲",
  diagnosis: { stop: { kind: "installed-at-path", command: "x", path: "$HOME/.claude/skills/cc-bus/scripts/cc-bus-stop-hook" }, said: "台架：后端写好的那一态", tone: "ok", note: "" },
  snippet,
  source: "/h/a/hooks.json",
  trust,
});
const diag = (snippet: string | null, supported = true, families = [family(snippet)]) => ({ supported, families });

type ManyArgs = { kind: string; name: string; to: (string | null)[]; place: unknown; fill?: unknown; tokens?: unknown };
const nameOf = (to: string | null) => (to === null ? HERE_NAME : to);
/** 本机后端并好的那一张卡（判据只排版：落点交集 · 并好的要填格都照它画）。 */
const manyCard = (a: ManyArgs, over: Record<string, unknown> = {}) => ({
  places: targets(),
  place: a.place ?? user,
  slots: [],
  machines: a.to.map((t) => ({ to: t, name: nameOf(t), card, files: ["/g/.claude/skills/demo · SKILL.md"], error: null, detail: null })),
  ...over,
});
const manyDone = (a: ManyArgs) => chanReply({ machines: a.to.map((t) => ({ to: t, name: nameOf(t), done: { path: "/g", changed: ["SKILL.md"], note: null }, error: null, detail: null })) });


/** 按命令名答（`ext-list` 依次答给定的几份）。 */
function backend(
  lists: unknown[],
  apply: (a: ManyArgs) => unknown = manyDone,
  hooks: (origin: string) => unknown = () => diag(null),
  preview: (a: ManyArgs) => unknown = (a) => manyCard(a),
) {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call") return undefined;
    switch (a.op) {
      case "ext-list":
        return chanReply(lists.length > 1 ? lists.shift() : lists[0]);
      case "assets-sync":
        return chanReply(synced);
      case "ext-hub-preview":
        return chanReply(preview(chanArgsJson(a) as ManyArgs));
      case "ext-hub-apply":
        return apply(chanArgsJson(a) as ManyArgs);
      case "ext-note-set":
        return chanReply({ note: (chanArgsJson(a) as { text: string }).text || null });
      case "hooks-diag":
        return chanReply(hooks(a.origin));
      case "login-start":
        return chanReply({ job: "job-1", url: "https://login.example/x", paste: "redirectUrl", auto: false });
      case "login-cancel":
        return chanReply({ was: true });
    }
    throw new Error(`没料到这一问：${a.op}`);
  });
}

async function page(lists: unknown[], apply?: (a: ManyArgs) => unknown, preview?: (a: ManyArgs) => unknown): Promise<ExtSection> {
  backend(lists, apply, undefined, preview);
  const s = new ExtSection();
  document.body.replaceChildren(s.element);
  s.loadNow();
  await settle();
  return s;
}

/** 一个点画的是哪个图标（代码画的 Phosphor；有字就原样带出来，好让「拿字符当图标」当场红）。 */
const glyph = (d: Element) => d.textContent || `icon:${d.querySelector<SVGElement>("svg")?.dataset.icon ?? ""}`;
const dots = (s: ExtSection, name: string) =>
  [...s.element.querySelectorAll(`.ext-row[data-key$="/${name}"] .ext-dot`)].map((d) => [glyph(d), (d as HTMLElement).title]);
const open = (s: ExtSection, key: string) => (s.element.querySelector(`.ext-row[data-key="${key}"]`) as HTMLButtonElement).click();
const machineLines = (s: ExtSection) => [...s.element.querySelectorAll(".ext-drawer .ext-machine")];
/** 抽屉里勾那一台（键 ＝ 机器名；本机是空串）。 */
const pick = (s: ExtSection, key: string) => (s.element.querySelector(`.ext-drawer .ext-pick[data-machine="${key}"]`) as HTMLInputElement).click();
const installBox = (s: ExtSection) => s.element.querySelector(".ext-install")!;
const installBtn = (s: ExtSection) => installBox(s).querySelector<HTMLButtonElement>('[data-action="install-many"]')!;

describe("扩展页：表 · 抽屉 · 确认卡", () => {
  beforeEach(() => invokeMock.mockReset());

  it("后端报了读不出的几处：状态行说件数 ＋ 名单（计数事实，不另加状态头）", async () => {
    const s = await page([{ ...listWith(demoMissing), problems: ["~/.claude.json", "plugins"] }]);
    const sep = copyText("kit.text.sep");
    expect(s.element.querySelector(".ext-status")!.textContent).toContain(copyText("extPage.status.problems", { n: "2", list: ["~/.claude.json", "plugins"].join(sep) }));
    expect(copyText("extPage.status.problems", { n: "2", list: "x" })).toMatch(/^2 /);
  });

  it("表：一行一个条目，每台一个点（悬停是机器名 ＋ 态），新见到的行上一个「新」、有备注的带一个记号；搜索与种类筛选只看后端给的字", async () => {
    const noted = row("skill", "noted", [here, demoMissing], { note: "我写的" });
    const s = await page([listWith(demoMissing, [noted])]);
    expect(dots(s, "demo")).toEqual([
      ["icon:success", copyText("extPage.dot.title", { machine: HERE_NAME, state: SAID.same })],
      ["icon:ring", copyText("extPage.dot.title", { machine: "laptop", state: SAID.missing })],
    ]);
    const rows = () => [...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"));
    expect(rows()).toEqual(["skill/demo", "mcp/fs", "skill/noted"]);
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"] .ext-new')?.textContent).toBe(copyText("extPage.row.new"));
    expect(s.element.querySelector('.ext-row[data-key="mcp/fs"] .ext-new')).toBeNull();
    const marked = [...s.element.querySelectorAll(".ext-row[data-key]")].filter((r) => r.querySelector(".ext-note-mark")).map((r) => r.getAttribute("data-key"));
    expect(marked).toEqual(["skill/noted"]);
    const mark = s.element.querySelector<HTMLElement>(".ext-note-mark")!;
    expect([glyph(mark), mark.getAttribute("aria-label")]).toEqual(["icon:note", copyText("extPage.row.noteMarkTitle")]);
    const search = s.element.querySelector(".ext-search") as HTMLInputElement;
    search.value = "npx";
    search.dispatchEvent(new Event("input"));
    expect(rows()).toEqual(["mcp/fs"]);
    search.value = "";
    search.dispatchEvent(new Event("input"));
    const [, skillOnly] = [...s.element.querySelectorAll(".ext-filter button")] as HTMLButtonElement[];
    skillOnly.click();
    expect(rows()).toEqual(["skill/demo", "skill/noted"]);
    // 进页那一问算「来看了一次」，随后同步一趟、再读（不算来看）。MCP 那几格的小标随 `ext-list` 的回包来（各台那一格 `mcp`），不另问各台。
    expect(ops()).toEqual([
      ["<local>", "ext-list", { visit: true }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
  });

  it("★ 筛选只挑行、不重建行：筛掉又放回的那一行还是原来那个元素（几百行 × 十几台时每敲一个字整张表重建一遍）", async () => {
    const s = await page([listWith(demoMissing)]);
    const demo = s.element.querySelector('.ext-row[data-key="skill/demo"]');
    const search = s.element.querySelector(".ext-search") as HTMLInputElement;
    search.value = "npx";
    search.dispatchEvent(new Event("input"));
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]'), "筛掉的不在表里").toBeNull();
    search.value = "";
    search.dispatchEvent(new Event("input"));
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]')).toBe(demo);
    const [, skillOnly] = [...s.element.querySelectorAll(".ext-filter button")] as HTMLButtonElement[];
    skillOnly.click();
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]')).toBe(demo);
  });

  it("★ 重读回来还是同一份表 ⇒ 不重画（进页先画手上那份、同步完再读一遍：两份一样时整张表白建一遍）；变了才重画", async () => {
    const s = await page([listWith(demoMissing)]);
    const demo = s.element.querySelector('.ext-row[data-key="skill/demo"]');
    s.loadNow();
    await settle();
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]'), "同一份表：行还是原来那个").toBe(demo);
    backend([listWith(cell("same", [place(user, "same", true)]))]);
    s.loadNow();
    await settle();
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]'), "表变了：重画").not.toBe(demo);
    expect(dots(s, "demo")[1][1]).toBe(copyText("extPage.dot.title", { machine: "laptop", state: SAID.same }));
  });

  it("抽屉：安装位置每台一行（装得了的才给勾 · 现状 · 只有全局一处的「卸载…」跟在后面）；装在项目里的逐处列；没有可装的说后端给的那一句", async () => {
    const both = cell("project", [place(user, "missing"), place(proj, "same", true)], bring());
    const s = await page([listWith(both)]);
    open(s, "mcp/fs");
    const lines = machineLines(s);
    expect(lines.map((l) => (l.querySelector(".ext-pick") as HTMLInputElement).disabled), "有 bring 才能勾").toEqual([false, true]);
    expect(lines.map((l) => [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent))).toEqual([[], []]);
    expect(lines[1].querySelector(":scope > .settings-hint")?.textContent).toBe("没项目");
    expect([...lines[0].querySelectorAll(".ext-place .settings-hint")].map((h) => h.textContent)).toEqual([copyText("agentWindow.status.plain")]);
    open(s, "skill/demo");
    const tops = machineLines(s).map((l) => [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent));
    expect(tops).toEqual([[copyText("extPage.button.uninstall")], []]);
    const places = machineLines(s).map((l) =>
      [...l.querySelectorAll(".ext-place")].map((p) => [p.querySelector(".ext-place-at")?.textContent, [...p.querySelectorAll("button")].map((b) => b.textContent)]),
    );
    expect(places).toEqual([
      [],
      [
        [copyText("extPage.loc.user"), []],
        [copyText("extPage.loc.project", { dir: "/g/p" }), [copyText("extPage.button.uninstall")]],
      ],
    ]);
  });

  it("★ 表头每台一列（列头是机器名）· 表下一行图例 · 连不上的那台整列半透明 ＋ 表上方一条", async () => {
    const list = listWith(demoMissing);
    list.machines = [machines[0], { ...machines[1], reachable: false }];
    const s = await page([list]);
    const head = s.element.querySelector(".ext-head")!;
    expect([...head.querySelectorAll(".ext-col")].map((c) => [c.textContent, c.classList.contains("is-offline")])).toEqual([
      [HERE_NAME, false],
      ["laptop", true],
    ]);
    expect(s.element.querySelector(".ext-offline")?.textContent).toBe(copyText("extPage.offline.bar", { machine: "laptop" }));
    expect([...s.element.querySelectorAll('.ext-row[data-key="skill/demo"] .ext-dot')].map((d) => d.classList.contains("is-offline"))).toEqual([false, true]);
    expect([...s.element.querySelectorAll(".ext-legend-item")].map((i) => [glyph(i.querySelector(".ext-dot")!), i.textContent])).toEqual([
      ["icon:success", LEGEND[0].said],
      ["icon:half", LEGEND[1].said],
      ["icon:ring", LEGEND[2].said],
      ["icon:folder", LEGEND[3].said],
      ["icon:success", copyText("acct.tag.login")],
      ["icon:success", copyText("beServer.words.cantConnect")],
    ]);
    open(s, "skill/demo");
    expect((machineLines(s)[1].querySelector(".ext-pick") as HTMLInputElement).disabled, "断开的那台勾不了").toBe(true);
  });

  it("★ 抽屉里连不上的那台：这一行变淡、勾不了、卸不了，现状后面点明是上次同步的样子；连得上的照常", async () => {
    const list = listWith(cell("same", [place(user, "same", true)], bring()));
    list.machines = [machines[0], { ...machines[1], reachable: false }];
    const s = await page([list]);
    open(s, "skill/demo");
    const [on, off] = machineLines(s);
    expect([on.classList.contains("is-offline"), off.classList.contains("is-offline")]).toEqual([false, true]);
    expect(off.querySelector(".ext-machine-state")?.textContent).toBe(copyText("extPage.install.offline", { state: SAID.same }));
    expect(on.querySelector(".ext-machine-state")?.textContent).not.toContain(copyText("extPage.install.offline", { state: "" }).trim());
    expect((off.querySelector(".ext-pick") as HTMLInputElement).disabled).toBe(true);
    expect([...off.querySelectorAll("button")].map((b) => [b.textContent, b.disabled]), "离线那台的「卸载」不可用").toEqual([[copyText("extPage.button.uninstall"), true]]);
    expect([...on.querySelectorAll("button")].map((b) => b.disabled), "连得上的那台照常").toEqual([false]);
  });

  it("skill 某一处有目录 ⇒ 每一处多一颗看目录的按钮，本机远端都开文件窗口进那个目录", async () => {
    const there = cell("same", [place(user, "same", true, null, "/g/.claude/skills/demo")]);
    const list = listWith(there);
    list.rows[0].cells[0] = cell("same", [place(user, "same", true, null, "/h/.claude/skills/demo")]);
    const s = await page([list]);
    open(s, "skill/demo");
    const per = machineLines(s).map((l) => [...l.querySelectorAll(".ext-place button")].map((b) => b.textContent));
    // 只有全局一处 ⇒「卸载…」跟在机器那一行后面，那一处下面只剩看目录的那一颗。
    // 本机远端同一句：不按本机 / 远端挑字。
    expect(per).toEqual([[copyText("data.item.reveal")], [copyText("data.item.reveal")]]);
    expect(machineLines(s).map((l) => [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent))).toEqual([
      [copyText("extPage.button.uninstall")],
      [copyText("extPage.button.uninstall")],
    ]);
    invokeMock.mockClear();
    (machineLines(s)[0].querySelector(".ext-place button") as HTMLButtonElement).click();
    (machineLines(s)[1].querySelector(".ext-place button") as HTMLButtonElement).click();
    await settle();
    // 两颗都直接调 `openFileWindow`：本机远端都是文件窗口开进那台那个目录，不交系统文件管理器。
    expect(invokeMock.mock.calls.filter(([c]) => c === "os_open")).toEqual([]);
    expect(
      invokeMock.mock.calls.filter(([c]) => c === "open_file_window").map(([, a]) => [(a as { origin: string }).origin, (a as { path: string }).path]),
    ).toEqual([
      ["<local>", "/h/.claude/skills/demo"],
      ["laptop", "/g/.claude/skills/demo"],
    ]);
  });

  it("勾上几台 ⇒ 问本机后端一张卡（勾上的几台 ＋ 选的那一处）；装到哪照它给的各处画；改选 ⇒ 带上那一处重问；［装到 N 台］⇒ 交回那一处与各台的记号 ⇒ 各台同步一趟、重读，点变「仅项目」", async () => {
    const after = cell("project", [place(user, "missing"), place(proj, "same", true)], bring());
    const s = await page([listWith(demoMissing), listWith(demoMissing), listWith(after)]);
    open(s, "skill/demo");
    expect(installBtn(s).disabled, "没勾不给装").toBe(true);
    invokeMock.mockClear();
    pick(s, "laptop");
    await settle();
    expect(ops()).toEqual([["<local>", "ext-hub-preview", { kind: "skill", name: "demo", to: ["laptop"], place: null }]]);
    const radios = () => [...installBox(s).querySelectorAll<HTMLInputElement>(".ext-targets input[type=radio]")];
    expect(radios().map((r) => [r.checked, r.disabled])).toEqual([
      [true, false],
      [false, false],
    ]);
    expect([...installBox(s).querySelectorAll(".ext-install-files")].map((l) => l.textContent)).toEqual([
      copyText("extPage.install.filesLine", { machine: "laptop", files: "/g/.claude/skills/demo · SKILL.md" }),
    ]);
    invokeMock.mockClear();
    radios()[1].checked = true;
    radios()[1].dispatchEvent(new Event("change"));
    await settle();
    expect(ops()).toEqual([["<local>", "ext-hub-preview", { kind: "skill", name: "demo", to: ["laptop"], place: proj }]]);
    expect(radios().map((r) => r.checked)).toEqual([false, true]);
    expect(installBtn(s).textContent).toBe(copyText("extPage.install.confirm", { n: 1 }));
    invokeMock.mockClear();
    installBtn(s).click();
    await settle();
    expect(ops().slice(0, 3)).toEqual([
      ["<local>", "ext-hub-apply", { kind: "skill", name: "demo", to: ["laptop"], place: proj, tokens: { laptop: { source: "s", target: "t" } }, fill: {} }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(dots(s, "demo").map(([d]) => d)).toEqual(["icon:success", "icon:folder"]);
    expect(installBox(s).querySelector('[data-result="laptop"]')?.textContent).toBe(copyText("extPage.install.doneOne", { machine: "laptop", said: copyText("extPage.done.written", { n: "1" }) }));
  });

  it("★ 两家同名的两行各是一行（键带家）；点开第二家那一行装 ⇒ 问的那一张卡带上行上的家，默认那一家那一行不带", async () => {
    const second = row("skill", "demo", [here, demoMissing], { agent: "fake2" });
    const s = await page([listWith(demoMissing, [second])]);
    const keys = [...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"));
    expect(keys.filter((k) => k?.endsWith("/demo"))).toEqual(["skill/demo", "fake2/skill/demo"]);
    open(s, "fake2/skill/demo");
    invokeMock.mockClear();
    pick(s, "laptop");
    await settle();
    expect(ops()).toEqual([["<local>", "ext-hub-preview", { kind: "skill", name: "demo", agent: "fake2", to: ["laptop"], place: null }]]);
  });

  it("不能选的那一处照后端给的列、置灰、旁注它那一句；那一处是后端定的", async () => {
    const off = [
      { at: user, ok: false, note: "全局的只读" },
      { at: proj, ok: true, note: null },
    ];
    const s = await page([listWith(demoMissing)], undefined, (a) => manyCard(a, { places: off, place: proj }));
    open(s, "mcp/fs");
    pick(s, "");
    await settle();
    const rows = [...installBox(s).querySelectorAll(".ext-target")];
    expect(
      rows.map((r) => [r.classList.contains("is-off"), (r.querySelector("input") as HTMLInputElement).disabled, (r.querySelector("input") as HTMLInputElement).checked, r.querySelector(".ext-target-note")?.textContent ?? null]),
    ).toEqual([
      [true, true, false, "全局的只读"],
      [false, false, true, null],
    ]);
  });

  it("各台结局逐台说：一台没成（如看过之后变了）不挡别台；做完重问那张卡（没成的那台拿到新卡，再点就是重试它）", async () => {
    const m3 = [...machines, { key: "nano", here: false, reachable: true, name: "nano", projects: [] }];
    const lst = { machines: m3, problems: [], legend: LEGEND, knownTitle: "〈认得〉", rows: [row("skill", "demo", [here, demoMissing, demoMissing])] };
    const s = await page([lst], (a) =>
      chanReply({
        machines: a.to.map((t) =>
          t === "nano"
            ? { to: t, name: "nano", done: null, error: "看过之后又变了，一个字节都没写。", detail: null }
            : { to: t, name: t, done: { path: "/g", changed: ["SKILL.md"], note: null }, error: null, detail: null },
        ),
      }),
    );
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    pick(s, "nano");
    await settle();
    invokeMock.mockClear();
    installBtn(s).click();
    await settle();
    expect(installBox(s).querySelector('[data-result="laptop"]')?.classList.contains("ext-done")).toBe(true);
    expect(installBox(s).querySelector('[data-result="nano"]')!.textContent).toBe(copyText("extPage.error.install", { machine: "nano", said: "看过之后又变了，一个字节都没写。" }));
    const previews = ops().filter(([, op]) => op === "ext-hub-preview");
    expect(previews.at(-1)?.[2], "装好的那台不再勾着；没成的那台重问一张卡").toEqual({ kind: "skill", name: "demo", to: ["nano"], place: null });
    expect(installBtn(s).disabled).toBe(false);
  });

  it("那台没拼成卡 ⇒ 会写的文件那一行就是「装到 <那台> 失败：」＋ 它那一句本身，码不上屏；别台照装", async () => {
    const said = copyText("beMcpEdit.path.notAbsolute", { dir: "w/x" });
    const s = await page([listWith(demoMissing)], undefined, (a) =>
      manyCard(a, { machines: a.to.map((t) => ({ to: t, name: nameOf(t), card: null, files: [], error: said, detail: null })) }),
    );
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    expect(installBox(s).querySelector('.ext-install-files[data-machine="laptop"]')!.textContent).toBe(copyText("extPage.error.install", { machine: "laptop", said }));
    expect(installBox(s).querySelectorAll(".ext-targets input").length).toBe(2);
    expect(installBtn(s).disabled, "一台都装不了 ⇒ 不给点").toBe(true);
  });

  it("★ 那台没成且带原话（如后端版本不同）⇒ 那一行只说那一句，原话进那一行的［复制详情］；卡上与结局两处都是", async () => {
    const said = "laptop 后端版本不同";
    const detail = "skill-install-apply: missing field `changed`";
    const text = copyText("extPage.error.install", { machine: "laptop", said });
    const copied = async (line: Element): Promise<string[]> => {
      expect(line.firstChild?.textContent).toBe(text);
      expect(line.textContent).not.toContain("missing field");
      const copy = line.querySelector<HTMLElement>('[data-part="copy-detail"] button');
      expect(copy).not.toBeNull();
      invokeMock.mockClear();
      copy!.click();
      await settle();
      return invokeMock.mock.calls.filter(([c]) => c === "clipboard_write").map(([, a]) => (a as { text: string }).text);
    };
    // 卡上：那台没拼成卡。
    let s = await page([listWith(demoMissing)], undefined, (a) =>
      manyCard(a, { machines: a.to.map((t) => ({ to: t, name: nameOf(t), card: null, files: [], error: said, detail })) }),
    );
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    const onCard = await copied(installBox(s).querySelector('.ext-install-files[data-machine="laptop"]')!);
    expect(onCard.length).toBe(1);
    expect(onCard[0]).toContain(detail);
    // 结局：卡拼成了、装那一步没成。
    s = await page([listWith(demoMissing)], (a) =>
      chanReply({ machines: a.to.map((t) => ({ to: t, name: nameOf(t), done: null, error: said, detail })) }),
    );
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    installBtn(s).click();
    await settle();
    const onResult = await copied(installBox(s).querySelector('[data-result="laptop"]')!);
    expect(onResult.length).toBe(1);
    expect(onResult[0]).toContain(detail);
  });
});

describe("备注 · cc-bus 那一行", () => {
  beforeEach(() => invokeMock.mockReset());
  const ccBus = (cells: unknown[], note: string | null = null) => row("skill", "cc-bus", cells, { known: { from: "〈自带〉", note: "内置：要加两条钩子", hooks: true }, note });

  it("认得的那几样：后端排在最前的、带 known 的行前面一条组头，名字后面那一句是后端写的来源；不带 known 的同名行不进组（界面不认名字）", async () => {
    const pb = row("plugin", "planned-build", [cell("missing", [place(user, "missing")], bring()), here], { known: { from: "〈发布处〉", note: null, hooks: false } });
    const base = listWith(demoMissing);
    const list = { ...base, rows: [pb, ...base.rows, row("skill", "code-picture", [here, here])] };
    const s = await page([list]);
    const table = s.element.querySelector(".ext-table") as HTMLElement;
    const order = [...table.children].map((e) => (e.classList.contains("ext-drawer-section") ? "组头" : ((e as HTMLElement).dataset.key ?? null))).filter((k) => k !== null);
    expect(order).toEqual(["组头", "plugin/planned-build", "skill/demo", "mcp/fs", "skill/code-picture"]);
    expect(table.querySelector(".ext-drawer-section")?.textContent).toBe("〈认得〉");
    const name = table.querySelector('[data-key="plugin/planned-build"] .ext-name') as HTMLElement;
    expect(name.textContent).toBe("planned-build 〈发布处〉");
    // 没有备注 ⇒ 不带备注记号。
    expect(name.querySelector(".ext-note-mark")).toBeNull();
    // 没有认得的那几行 ⇒ 组头也不在。
    const plain = await page([base]);
    expect(plain.element.querySelector(".ext-table .ext-drawer-section")).toBeNull();
  });

  it("备注：内置的照画在抽屉顶部；用户写 ⇒ 交本机后端记进目录 ⇒ 让它同步一趟各台、重读", async () => {
    const list = listWith(demoMissing, [ccBus([here, demoMissing])]);
    const s = await page([list, list, { ...list, rows: [...list.rows.slice(0, 2), ccBus([here, demoMissing], "我的话")] }]);
    open(s, "skill/cc-bus");
    await settle();
    expect(s.element.querySelector(".ext-drawer .ext-note-builtin")?.textContent).toBe("内置：要加两条钩子");
    (s.element.querySelector(".ext-notes button") as HTMLButtonElement).click();
    const input = s.element.querySelector(".ext-note-input") as HTMLTextAreaElement;
    input.value = "我的话";
    input.dispatchEvent(new Event("input"));
    invokeMock.mockClear();
    (s.element.querySelector('.ext-notes [data-kind="primary"]') as HTMLButtonElement).click();
    await settle();
    expect(ops().filter(([, op]) => op !== "hooks-diag")).toEqual([
      ["<local>", "ext-note-set", { kind: "skill", name: "cc-bus", text: "我的话" }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(s.element.querySelector(".ext-drawer .ext-note-user")?.textContent).toBe("我的话");
  });

  it("★ 两家同名：给第二家那一行写备注 ⇒ 发出去带行上的家，重读后只那一行回填备注", async () => {
    const second = (note: string | null) => row("skill", "demo", [here, demoMissing], { agent: "fake2", note });
    const before = listWith(demoMissing, [second(null)]);
    const after = listWith(demoMissing, [second("第二家的")]);
    const s = await page([before, before, after]);
    open(s, "fake2/skill/demo");
    await settle();
    (s.element.querySelector(".ext-notes button") as HTMLButtonElement).click();
    const input = s.element.querySelector(".ext-note-input") as HTMLTextAreaElement;
    input.value = "第二家的";
    input.dispatchEvent(new Event("input"));
    invokeMock.mockClear();
    (s.element.querySelector('.ext-notes [data-kind="primary"]') as HTMLButtonElement).click();
    await settle();
    expect(ops().filter(([, op]) => op === "ext-note-set")).toEqual([
      ["<local>", "ext-note-set", { kind: "skill", name: "demo", agent: "fake2", text: "第二家的" }],
    ]);
    expect(s.element.querySelector(".ext-drawer .ext-note-user")?.textContent).toBe("第二家的");
    expect(s.element.querySelector('[data-key="skill/demo"] .ext-note-mark')).toBeNull();
  });

  it("cc-bus：点开 ⇒ 每台问它自己的后端一次（只问 hooks-diag）；态照后端画；那台没装 cc-bus ⇒ 只说先装上、不给内容；装着 ⇒ 要加的内容就是后端给的那一段", async () => {
    const snippet = '{\n  "hooks": {}\n}';
    const list = listWith(demoMissing, [ccBus([here, demoMissing])]);
    backend([list], undefined, (o) => diag(o === "<local>" ? snippet : null));
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    invokeMock.mockClear();
    open(s, "skill/cc-bus");
    await settle();
    expect(ops().sort()).toEqual([
      ["<local>", "hooks-diag", {}],
      ["laptop", "hooks-diag", {}],
    ]);
    const lines = [...s.element.querySelectorAll(".ext-hooks .ext-hook")];
    expect(lines.map((l) => [...l.querySelectorAll(".ext-hook-state")].map((x) => (x as HTMLElement).dataset.kind))).toEqual([
      ["installed-at-path"],
      ["installed-at-path"],
    ]);
    expect(lines[0].querySelector(".ext-hook-state")?.className).toBe("ext-hook-state is-ok");
    expect(lines[0].querySelector(".ext-hook-state")?.textContent, "那一态照抄后端写好的那一句").toContain("台架：后端写好的那一态");
    expect((lines[0].querySelector(".paste-block-out") as HTMLTextAreaElement).value).toBe(snippet);
    expect(lines[0].querySelector(".paste-block-target")?.textContent).toContain("/h/a/hooks.json");
    expect(lines[0].querySelector(".ext-hook-state")?.textContent, "那一家的名字照后端给的").toContain("甲");
    expect(lines[1].querySelector(".paste-block-out")).toBeNull();
    expect(lines[1].querySelector(".ext-hook-install-first")?.textContent).toBe(copyText("extPage.hooks.installFirst"));
  });

  it("cc-bus：一台里每家一份；声明了信任的那一家多一句信任（照抄后端那一句与色调），未对上 ⇒ 再给要并进信任那份的那一段；对上 ⇒ 不给", async () => {
    const snippet = '{\n  "onStop": []\n}';
    const bad = { trusted: false, said: "台架：信任那一句", tone: "bad", file: "/h/a/trust.txt", line: "k = v\n" };
    const ok = { ...bad, trusted: true, tone: "ok", line: null };
    const list = listWith(demoMissing, [ccBus([here, demoMissing])]);
    backend([list], undefined, (o) => diag(snippet, true, o === "<local>" ? [family(snippet, bad), family(snippet)] : [family(snippet, ok)]));
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "skill/cc-bus");
    await settle();
    const [local, laptop] = [...s.element.querySelectorAll(".ext-hooks .ext-hook")];
    expect(local.querySelectorAll(".ext-hook-state[data-kind]"), "每家一句钩子态").toHaveLength(2);
    const trust = local.querySelector(".ext-hook-state[data-trust]") as HTMLElement;
    expect(trust.dataset.trust).toBe("untrusted");
    expect(trust.className).toBe("ext-hook-state is-bad");
    expect(trust.textContent).toContain("台架：信任那一句");
    const outs = [...local.querySelectorAll(".paste-block-out")].map((x) => (x as HTMLTextAreaElement).value);
    expect(outs).toEqual([snippet, "k = v\n", snippet]);
    expect([...local.querySelectorAll(".paste-block-target")].map((x) => x.textContent)[1]).toContain("/h/a/trust.txt");
    expect((laptop.querySelector(".ext-hook-state[data-trust]") as HTMLElement).dataset.trust).toBe("trusted");
    expect(laptop.querySelectorAll(".paste-block-out"), "对上了只给钩子那一段").toHaveLength(1);
  });

  it("cc-bus：跑不了它的那台（没有 tmux）⇒ 那一行只说这台不支持自动收信，不列钩子态、不给内容", async () => {
    const list = listWith(demoMissing, [ccBus([here, demoMissing])]);
    backend([list], undefined, (o) => diag(o === "laptop" ? null : '{"hooks":{}}', o !== "laptop"));
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "skill/cc-bus");
    await settle();
    const laptop = [...s.element.querySelectorAll(".ext-hooks .ext-hook")][1];
    expect(laptop.querySelector(".ext-hook-unsupported")?.textContent).toBe(copyText("extPage.hooks.unsupported"));
    expect(laptop.querySelectorAll(".ext-hook-state, .paste-block-out")).toHaveLength(0);
  });
});

/** 一份源码里「比较指纹」的痕迹：出现摘要这个词，或拿记号去比。注释先剥掉。 */
export function comparesFingerprints(src: string): string[] {
  const code = stripComments(src, "ts");
  const hits: string[] = [];
  for (const m of code.matchAll(/[^\n]*\bdigest\b[^\n]*/gi)) hits.push(m[0].trim());
  for (const m of code.matchAll(/[^\n]*\btokens?\b[^\n]*(===|!==|==|!=)[^\n]*/g)) hits.push(m[0].trim());
  return hits;
}

// 要求：MCP 状态那一块（每台那个点右上角一个小标：黄 = 需登录 · 红 = 断开；没小标只说「装了」；图例补两项；
// 停用的不画——停用按项目记，一格一台说不清是哪个项目）与甲 2（抽屉里那台下面一行：需登录那一句 ＋［登录…］开登录抽屉（别的入口；
// 原来那颗「去登录 · 复制 /mcp 去会话里粘」作废）；
// 断开那一句 ＋ 原话 ＋［复制详情］；两种都没有 ⇒ 不画）。字与哪一种都是那台后端 `mcp-read` 写好的，界面照抄。
describe("MCP：每台的小标与抽屉那一行", () => {
  /** 各台那一格 `mcp`（本机后端问到的那台 `mcp-read` 成品，按 origin；没写 ⇒ `null`：没问到）。 */
  let mcpAnswers: Record<string, unknown> = {};
  beforeEach(() => {
    invokeMock.mockReset();
    mcpAnswers = {};
  });
  const both = cell("same", [place(user, "same")]);
  const withMcp = (ms: typeof machines) => ms.map((m) => ({ ...m, mcp: mcpAnswers[m.here ? "<local>" : (m.key ?? "")] ?? null }));
  const mcpList = () => ({ machines: withMcp(machines), problems: [], legend: LEGEND, knownTitle: "〈认得〉", rows: [row("skill", "demo", [here, here]), row("mcp", "tracker", [both, both])] });
  const entry = (name: string, over: Record<string, unknown>) => ({ scope: "user", name, server: {}, sourcePath: "/h/.claude.json", status: "unknown", loginIn: [], seenAt: null, mark: null, login: null, failed: null, ...over });
  const fixArgs = { kind: "mcp" as const, agent: "claude", account: "acct-a", server: "tracker", projectDir: null };
  const login = { said: copyText("beMcp.mark.login", { accounts: "acct-a", time: "10:42" }), tip: copyText("beMcp.mark.loginTip", { accounts: "acct-a" }), fix: [{ account: "acct-a", fix: { command: "login-start", args: fixArgs } }] };
  const failed = { said: copyText("beMcp.mark.failed", { title: "会话-甲", time: "10:31" }), tip: copyText("beServer.words.cantConnect"), detail: "err: refused" };
  const laterSaid = copyText("beMcp.mark.failed", { title: "会话-乙", time: "10:50" });
  const answers = () => {
    mcpAnswers = {
      "<local>": { entries: [entry("tracker", { status: "needsLogin", loginIn: ["acct-a"], seenAt: 1, mark: "needsLogin", login })], dirs: [], problems: [] },
      laptop: { entries: [entry("tracker", { mark: "failed", failed })], dirs: [], problems: [] },
    };
  };
  const marks = (s: ExtSection, name: string) =>
    [...s.element.querySelectorAll(`.ext-row[data-key$="/${name}"] .ext-dot`)].map((d) => [d.querySelector(".ext-dot-mark")?.className ?? null, (d as HTMLElement).title]);
  const sameSaid = SAID.same;

  it("小标：哪一种照那台后端的 mark 画，悬停在态后面接它写好的那一截；skill 行不问也不画", async () => {
    answers();
    const s = await page([mcpList()]);
    expect(marks(s, "tracker")).toEqual([
      ["ext-dot-mark is-login", copyText("extPage.dot.titleMark", { machine: HERE_NAME, state: sameSaid, mark: login.tip })],
      ["ext-dot-mark is-fail", copyText("extPage.dot.titleMark", { machine: "laptop", state: sameSaid, mark: failed.tip })],
    ]);
    expect(marks(s, "demo").map(([m]) => m)).toEqual([null, null]);
  });

  it("没小标只说装了：那台答的那一条 mark 为空 · 本机后端没问到那台（`mcp` 为 null）· 那台没连上 ⇒ 都不画", async () => {
    mcpAnswers = { "<local>": { entries: [entry("tracker", {})], dirs: [], problems: [] } };
    const s = await page([mcpList()]);
    expect(marks(s, "tracker")).toEqual([
      [null, copyText("extPage.dot.title", { machine: HERE_NAME, state: sameSaid })],
      [null, copyText("extPage.dot.title", { machine: "laptop", state: sameSaid })],
    ]);
    open(s, "mcp/tracker");
    expect(s.element.querySelector(".ext-drawer .ext-mcp")).toBeNull();
    answers();
    const list = mcpList();
    list.machines = [list.machines[0], { ...list.machines[1], reachable: false }];
    const t = await page([list]);
    expect(marks(t, "tracker").map(([m]) => m)).toEqual(["ext-dot-mark is-login", null]);
  });

  it("★ 各台的 MCP 随 `ext-list` 一条来：界面一台都不问 `mcp-read`", async () => {
    answers();
    invokeMock.mockClear();
    await page([mcpList()]);
    expect(ops().map(([o, op]) => `${o} ${op}`)).toEqual(["<local> ext-list", "<local> assets-sync", "<local> ext-list"]);
  });

  it("★ 抽屉：需登录那一句 ＋［登录…］（那一格自己带的解法 `fix`，开登录抽屉、交它的 args）；不复制 /mcp；连不上那一句 ＋ 原话 ＋［复制详情］", async () => {
    answers();
    const args = fixArgs;
    const s = await page([mcpList()]);
    open(s, "mcp/tracker");
    const [mine, theirs] = machineLines(s);
    expect(mine.querySelector(".ext-mcp.is-login .ext-mcp-said")?.textContent).toBe(login.said);
    expect(mine.querySelector(".ext-mcp.is-fail")).toBeNull();
    expect(theirs.querySelector(".ext-mcp.is-fail .ext-mcp-said")?.textContent).toBe(failed.said);
    expect(theirs.querySelector(".ext-mcp-raw")?.textContent).toBe(failed.detail);
    expect(theirs.querySelector('.ext-mcp.is-fail [data-part="copy-detail"]')).not.toBeNull();
    expect(theirs.querySelector(".ext-mcp.is-login")).toBeNull();
    const go = [...mine.querySelectorAll<HTMLButtonElement>('[data-action="mcp-login"]')];
    expect(go.map((b) => b.textContent), "每个要登的号一颗").toEqual([copyText("newSession.account.login")]);
    invokeMock.mockClear();
    go[0].click();
    await settle();
    expect(ops().filter(([, op]) => op === "login-start")).toEqual([["<local>", "login-start", { ...args, browserHere: true }]]);
    expect(invokeMock.mock.calls.filter(([c]) => c === "clipboard_write"), "不再复制 /mcp").toEqual([]);
    expect(document.querySelector("aside[data-login-kind]")?.getAttribute("aria-label")).toBe(copyText("login.sheet.mcp"));
    document.querySelector<HTMLButtonElement>('aside[data-login-kind] [data-action="login-bottom"]')!.click();
    await settle();
  });

  it("★ 同名同配置两个号：一行；抽屉那台显后端写好的「在哪几个号里」那一句（界面不看 sourcePath 认号）", async () => {
    const held = { said: "〈在 acct-a · acct-b 里〉", tip: null };
    mcpAnswers = { "<local>": { entries: [entry("tracker", { accounts: ["acct-a", "acct-b"], held })], dirs: [], problems: [] } };
    const s = await page([mcpList()]);
    expect([...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"))).toEqual(["skill/demo", "mcp/tracker"]);
    open(s, "mcp/tracker");
    const [mine, theirs] = machineLines(s);
    expect(mine.querySelector(".ext-mcp.is-held .ext-mcp-said")?.textContent).toBe(held.said);
    expect(mine.querySelector(".ext-mcp.is-mismatch")).toBeNull();
    expect(theirs.querySelector(".ext-mcp")).toBeNull();
  });

  it("★ 同名不一致：后端拆成的两行各一行、名字后带那一行的标记；抽屉照行上各台的下标（picks）挑那一条、显各自那一句与悬停", async () => {
    const tip = "〈各号配置不同〉";
    const heldA = { said: "〈acct-a 那份不一致〉", tip };
    const heldB = { said: "〈acct-b 那份不一致〉", tip };
    const heldC = { said: "〈在 acct-c 里〉", tip: null };
    mcpAnswers = {
      "<local>": {
        entries: [entry("tracker", { accounts: ["acct-a"], mismatch: true, held: heldA }), entry("tracker", { accounts: ["acct-b"], mismatch: true, held: heldB })],
        dirs: [],
        problems: [],
      },
      laptop: { entries: [entry("tracker", { accounts: ["acct-c"], held: heldC })], dirs: [], problems: [] },
    };
    const list = mcpList();
    list.rows = [
      row("skill", "demo", [here, here]),
      row("mcp", "tracker", [both, both], { variant: 0, picks: [0, null], heldSaid: heldA.said }),
      row("mcp", "tracker", [both, both], { variant: 1, picks: [1, 0], heldSaid: heldB.said }),
    ];
    const s = await page([list]);
    const keys = [...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"));
    expect(keys).toEqual(["skill/demo", "mcp/tracker#0", "mcp/tracker#1"]);
    expect([...s.element.querySelectorAll(".ext-row[data-key^='mcp/'] .ext-mismatch")].map((x) => x.textContent)).toEqual([heldA.said, heldB.said]);
    for (const [key, held, theirs] of [["mcp/tracker#0", heldA, null], ["mcp/tracker#1", heldB, heldC.said]] as const) {
      open(s, key);
      const [mine, laptop] = machineLines(s);
      const box = mine.querySelector<HTMLElement>(".ext-mcp.is-mismatch");
      expect(box?.querySelector(".ext-mcp-said")?.textContent).toBe(held.said);
      expect(box?.title).toBe(tip);
      expect(laptop.querySelector(".ext-mcp.is-held .ext-mcp-said")?.textContent ?? null, "laptop 那一条照下标落在它那一份的行上").toBe(theirs);
    }
  });

  it("后台重读回来变了：只重建 MCP 那几行（skill 那一行还是原来那个元素），抽屉不关、换成新的字", async () => {
    answers();
    const first = mcpList();
    mcpAnswers = { "<local>": { entries: [entry("tracker", {})], dirs: [], problems: [] }, laptop: { entries: [entry("tracker", { mark: "failed", failed: { ...failed, said: laterSaid } })], dirs: [], problems: [] } };
    const s = await page([first, first, mcpList()]);
    open(s, "mcp/tracker");
    const skillRow = s.element.querySelector('.ext-row[data-key="skill/demo"]');
    s.loadNow();
    await settle();
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"]')).toBe(skillRow);
    expect(marks(s, "tracker").map(([m]) => m)).toEqual([null, "ext-dot-mark is-fail"]);
    expect(s.element.querySelector(".ext-drawer")?.hasAttribute("hidden")).toBe(false);
    const [mine, theirs] = machineLines(s);
    expect(mine.querySelector(".ext-mcp")).toBeNull();
    expect(theirs.querySelector(".ext-mcp-said")?.textContent).toBe(laterSaid);
  });
});

describe("插件：一种 · 一颗筛选 · 关着一态（只读）", () => {
  beforeEach(() => invokeMock.mockReset());
  const line = "1.4.0 · 开着 · 用户设置";
  const plugin = row(
    "plugin",
    "github",
    [
      cell("same", [{ ...place(user, "same"), line }], null, "插件只读"),
      cell("off", [{ ...place(user, "off"), line: "1.3.2 · 关着 · 用户设置" }], null, "插件只读"),
    ],
    { about: "读写 GitHub" },
  );

  it("★ 种类一格画后端那一句；点多一态「关着」（自己一个图标 · 悬停是后端那一句）；图例照后端给的列（含「关着」）", async () => {
    const offLegend = { state: "off", said: "〈图例 off〉" };
    const s = await page([{ ...listWith(demoMissing, [plugin]), legend: [...LEGEND, offLegend] }]);
    expect(s.element.querySelector('.ext-row[data-key="plugin/github"] .ext-kind')?.textContent).toBe(KIND_SAID.plugin);
    const got = dots(s, "github");
    expect(got[1]).toEqual(["icon:stop", copyText("extPage.dot.title", { machine: "laptop", state: SAID.off })]);
    expect(new Set(got.map(([g]) => g)).size, "关着与开着不是同一个图标").toBe(2);
    const legend = [...s.element.querySelectorAll(".ext-legend-item")].map((i) => [glyph(i.querySelector(".ext-dot")!), i.textContent]);
    expect(legend.slice(0, 5)).toEqual([
      ["icon:success", LEGEND[0].said],
      ["icon:half", LEGEND[1].said],
      ["icon:ring", LEGEND[2].said],
      ["icon:folder", LEGEND[3].said],
      ["icon:stop", offLegend.said],
    ]);
  });

  it("★ 筛选多一颗「插件」：只留插件行；别的几颗不挑插件", async () => {
    const s = await page([listWith(demoMissing, [plugin])]);
    const rows = () => [...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"));
    const buttons = [...s.element.querySelectorAll(".ext-filter button")] as HTMLButtonElement[];
    expect(buttons.map((b) => b.textContent)).toEqual([
      copyText("extPage.filter.all"),
      copyText("mobPlus.row.skills"),
      copyText("extPage.filter.mcp"),
      copyText("beExt.kind.plugin"),
    ]);
    buttons[3].click();
    expect(rows()).toEqual(["plugin/github"]);
    buttons[1].click();
    expect(rows()).not.toContain("plugin/github");
  });

  it("★ 抽屉：每台没有勾（只读）、说后端那一句；那一处后端写好的那一句（版本 · 开关 · 出处）原样列出", async () => {
    const s = await page([listWith(demoMissing, [plugin])]);
    open(s, "plugin/github");
    const lines = machineLines(s);
    expect(lines.map((l) => (l.querySelector(".ext-pick") as HTMLInputElement).disabled)).toEqual([true, true]);
    expect(lines.map((l) => l.querySelector(":scope > .settings-hint")?.textContent)).toEqual(["插件只读", "插件只读"]);
    expect(lines.map((l) => l.querySelector(".ext-place-line")?.textContent)).toEqual([line, "1.3.2 · 关着 · 用户设置"]);
    expect(s.element.querySelectorAll(`.ext-drawer [data-action="install-many"]`).length).toBe(0);
  });
});

describe("界面层零判定：扩展页不比较指纹、不读配置文件", () => {
  it("扩展页与它的读口两份源码里零处（正控：同一把尺子认得出一处现造的比较）", () => {
    expect(comparesFingerprints("if (a.digest === b.digest) x();")).toHaveLength(1);
    expect(comparesFingerprints("if (card.tokens.source !== t) x();")).toHaveLength(1);
    for (const f of ["src/frontend/ui/settings/ext-section.ts", "src/frontend/ui/ext-reads.ts"]) {
      expect(comparesFingerprints(readFileSync(resolve(REPO_ROOT, f), "utf8")), f).toEqual([]);
    }
  });

  it("扩展页自己不直呼通道、不读文件：问什么都经那两份读口（钩子状态也是那台后端给的成品）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/settings/ext-section.ts"), "utf8");
    expect([...src.matchAll(/chan\.call\(/g)]).toHaveLength(0);
    expect(src).not.toMatch(/files-peek|files-browse|read_text|settings\.json/);
  });
});

// 「装到…」卡上填的密钥跟着卡走：抽屉因为别的事重画（另一台点了「装到…」、后台同步回来）不许把它清空，确认时交的是填的那份。
describe("一次装到几台：要填的值只填一次", () => {
  beforeEach(() => invokeMock.mockReset());

  it("★ 后端并好的那一格只出一次、说哪台已有这个值；填一次 ⇒ 交上去的就是它（每台只拿它那张卡要的几格由后端分）", async () => {
    const m3 = [...machines, { key: "nano", here: false, reachable: true, name: "nano", projects: [] }];
    const mcpMissing = cell("missing", [place(user, "missing")], bring());
    const lst = { machines: m3, problems: [], legend: LEGEND, knownTitle: "〈认得〉", rows: [row("mcp", "srv", [cell("same", [place(user, "same", true)]), mcpMissing, mcpMissing])] };
    let applied: ManyArgs | null = null;
    const s = await page(
      [lst],
      (a) => {
        applied = a;
        return manyDone(a);
      },
      (a) => manyCard(a, { slots: [{ field: "env", key: "API_KEY", kept: a.to.includes("nano") ? ["nano"] : [] }] }),
    );
    open(s, "mcp/srv");
    pick(s, "laptop");
    await settle();
    pick(s, "nano");
    await settle();
    const secrets = [...installBox(s).querySelectorAll<HTMLInputElement>(".ext-card-secret")];
    expect(secrets.length, "那一格只出一次").toBe(1);
    expect(installBox(s).textContent).toContain(copyText("extPage.install.kept", { machines: "nano", key: "API_KEY" }));
    secrets[0].value = "sk-123";
    secrets[0].dispatchEvent(new Event("input"));
    expect(installBtn(s).textContent).toBe(copyText("extPage.install.confirm", { n: 2 }));
    installBtn(s).click();
    await settle();
    expect(applied).toMatchObject({ to: ["laptop", "nano"], fill: { env: { API_KEY: "sk-123" } } });
  });
});

// 要求：两家同名的行显出是哪一家 —— 行上一格家名，字是后端给的那一家的叫法（成品，`agentSaid`），界面不写死哪一家；
// MCP 行的小标照名字与家一起挑 `mcp-read` 那一条（每条带可缺的 `agent`：默认那一家不写）。
describe("两家同名：行上显家名 · 小标照家挑", () => {
  beforeEach(() => invokeMock.mockReset());
  const family = (s: ExtSection, key: string) => s.element.querySelector(`.ext-row[data-key="${key}"] .ext-family`)?.textContent ?? null;

  it("★ 两家同名 skill ⇒ 两行各显各家的叫法（后端给的字原样）；行上没给家名 ⇒ 不画那一格；抽屉头上也显", async () => {
    const mine = row("skill", "demo", [here, demoMissing], { agentSaid: "〈家甲〉" });
    const theirs = row("skill", "demo", [here, demoMissing], { agent: "fake2", agentSaid: "〈家乙〉" });
    const lst = { machines, problems: [], legend: LEGEND, knownTitle: "〈认得〉", rows: [mine, theirs, row("skill", "solo", [here, here])] };
    const s = await page([lst]);
    expect([family(s, "skill/demo"), family(s, "fake2/skill/demo"), family(s, "skill/solo")]).toEqual(["〈家甲〉", "〈家乙〉", null]);
    open(s, "fake2/skill/demo");
    expect(s.element.querySelector(".ext-drawer-head .ext-family")?.textContent).toBe("〈家乙〉");
  });

  it("★ 同名插件两个来源 ⇒ 两行（键带来源）、各显后端写好的来源那一句，抽屉头上也显；点开一行装 ⇒ 那张卡带那一行的来源键", async () => {
    const a = row("plugin", "demo", [here, demoMissing], { source: { key: "demo@shelf", said: "〈来自甲〉" } });
    const b = row("plugin", "demo", [here, demoMissing], { source: { key: "demo@rack", said: "〈来自乙〉" } });
    const s = await page([{ machines, problems: [], legend: LEGEND, knownTitle: "〈认得〉", rows: [a, b] }]);
    const said = (key: string) => s.element.querySelector(`.ext-row[data-key="${key}"] .ext-source`)?.textContent ?? null;
    expect([said("plugin/demo|demo@shelf"), said("plugin/demo|demo@rack")]).toEqual(["〈来自甲〉", "〈来自乙〉"]);
    open(s, "plugin/demo|demo@rack");
    expect(s.element.querySelector(".ext-drawer-head .ext-source")?.textContent).toBe("〈来自乙〉");
    invokeMock.mockClear();
    pick(s, "laptop");
    await settle();
    expect(ops()).toEqual([["<local>", "ext-hub-preview", { kind: "plugin", name: "demo", source: "demo@rack", to: ["laptop"], place: null }]]);
  });

  it("★ 两家同名 MCP ⇒ 各取各家那一条小标（默认那一家那一条不带 agent）", async () => {
    const both = cell("same", [place(user, "same")]);
    const entry = (over: Record<string, unknown>) => ({ scope: "user", name: "tracker", server: {}, sourcePath: "/h/x", status: "unknown", loginIn: [], seenAt: null, mark: null, login: null, failed: null, ...over });
    const login = { said: "〈要登录〉", tip: "〈登录小字〉", fix: [] };
    const failed = { said: "〈断了〉", tip: "〈断开小字〉", detail: null };
    const mcp = { entries: [entry({ agent: "fake2", mark: "failed", failed }), entry({ mark: "needsLogin", login })], dirs: [], problems: [] };
    const lst = {
      machines: machines.map((m) => ({ ...m, mcp })),
      problems: [],
      legend: LEGEND,
      knownTitle: "〈认得〉",
      rows: [row("mcp", "tracker", [both, both], { agentSaid: "〈家甲〉" }), row("mcp", "tracker", [both, both], { agent: "fake2", agentSaid: "〈家乙〉" })],
    };
    const s = await page([lst]);
    const mark = (key: string) => [...s.element.querySelectorAll(`.ext-row[data-key="${key}"] .ext-dot`)].map((d) => d.querySelector(".ext-dot-mark")?.className ?? null);
    expect(mark("mcp/tracker")).toEqual(["ext-dot-mark is-login", "ext-dot-mark is-login"]);
    expect(mark("fake2/mcp/tracker")).toEqual(["ext-dot-mark is-fail", "ext-dot-mark is-fail"]);
  });

  it("界面里不出现任何一家的字面名（家名只照后端给的那一格画）", () => {
    const src = stripComments(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/settings/ext-section.ts"), "utf8"), "ts");
    expect(src).not.toMatch(/claude|codex/i);
  });

  it("称呼一台只照表里的 `name`：源码里没有自判本机的那一支（columnName），也不另调 machineName", () => {
    const src = stripComments(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/settings/ext-section.ts"), "utf8"), "ts");
    expect(src).not.toMatch(/\bcolumnName\b/);
    expect(src).not.toMatch(/\bmachineName\b/);
    // 反空真：读的那一格真在。
    expect(src).toMatch(/\bm\.name\b/);
  });
});

describe("几百行 × 十几台的表：滚出视口的行不排（读数住 `tests/shots/perf/settings-bench.mjs`）", () => {
  it("表行（表头除外）content-visibility: auto；估的内容高是一行字（内边距与底线另算，估错了滚动条长短会跳）", () => {
    const css = stripComments(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/settings.css"), "utf8"), "css");
    const body = /\.ext-row:not\(\.ext-head\)\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(body, "那一条规则在").not.toBe("");
    expect(body).toMatch(/content-visibility:\s*auto;/);
    expect(body).toMatch(/contain-intrinsic-size:\s*auto 1lh;/);
  });
});
