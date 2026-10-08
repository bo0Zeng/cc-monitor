/**
 * 要求：设置「扩展」页 —— 一张表（搜索 · 种类筛选 · 一行一个条目 · 每台一个点 ● ◐ ○ ◎，悬停是机器名 · 新见到的行上一个「新」·
 * 有备注的行带一个记号）＋ 一个抽屉（顶部备注；每台一行展开成它的各处，各自的态与「卸载」；机器那一行一个「装到…」）
 * ＋ 一张确认卡（「装到哪」由用户选：后端给的各处，不能选的显示但置灰、旁注为什么；一个确认按钮；后端答 stale 就在卡上说一句、给「重看」）；
 * cc-bus 那一行：内置备注下面每台一行钩子状态与要加的内容，都由那台后端给（界面不读那份配置文件）。
 * 做完之后重读那张表，那一格的点自己变。界面只画、只问：不比较指纹、不做判定（扫描判据在本文件末尾）。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { revealed } = vi.hoisted(() => ({ revealed: [] as unknown[] }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (p: unknown) => {
    revealed.push(p);
    return Promise.resolve();
  },
}));

import { invoke } from "@tauri-apps/api/core";
import { ExtSection } from "../../../../src/frontend/ui/settings/ext-section";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { REPO_ROOT } from "../../../test-support/repo-root";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const ops = () => invokeMock.mock.calls.filter(([c]) => c === "chan_call").map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 8; i++) await tick();
};

const user = { level: "user" } as const;
const proj = { level: "project", dir: "/g/p" } as const;
const machines = [
  { key: null, here: true, reachable: true, name: "me@h", projects: ["/h/p"] },
  { key: "laptop", here: false, reachable: true, name: "laptop", projects: ["/g/p"] },
];
const place = (at: object, state: string, uninstall = false, note: string | null = null, dir: string | null = null) => ({ at, state, dir, uninstall, note });
const cell = (state: string, places: object[], bring: unknown = null, note: string | null = null) => ({ state, places, bring, note });
const targets = (globalOk = true, globalNote: string | null = null) => [
  { at: user, ok: globalOk, note: globalNote },
  { at: proj, ok: true, note: null },
];
const bring = (over: Record<string, unknown> = {}) => ({ from: null, fromName: "本机", scope: { from: user, to: user }, targets: targets(), ...over });
const row = (kind: string, name: string, cells: unknown[], over: Record<string, unknown> = {}) => ({ kind, name, about: null, detail: [], new: false, builtin: null, note: null, cells, ...over });
const here = cell("same", [place(user, "same", true)]);
const demoMissing = cell("missing", [place(user, "missing")], bring());
const listWith = (there: unknown, extra: ReturnType<typeof row>[] = []) => ({
  machines,
  problems: [],
  rows: [
    row("skill", "demo", [here, there], { about: "does demo", detail: [{ label: copyText("diagnostics.files.title"), value: copyText("accounts.status.count", { n: "1" }) }], new: true }),
    row("mcp", "fs", [cell("same", [place(user, "same", false, copyText("agentWindow.status.plain"))], bring({ scope: { from: user, to: proj }, targets: targets(false, "全局的只读") })), cell("missing", [place(user, "missing")], null, "没项目")], {
      about: "npx fs",
    }),
    ...extra,
  ],
});
const card = { kind: "skill", name: "demo", path: "/g/.claude/skills/demo", writes: ["SKILL.md"], unchanged: false, suspects: [], stop: null, config: null, slots: [], tokens: { source: "s", target: "t" } };
const synced = { self: "h", synced: [], reach: [{ origin: "laptop", machine: "g" }] };
const diag = (snippet: string | null, supported = true) => ({
  supported,
  diagnosis: { session_start: { kind: "not-installed" }, stop: { kind: "installed-at-path", command: "x", path: "$HOME/.claude/skills/cc-bus/scripts/cc-bus-stop-hook" }, note: "" },
  snippet,
  source: "/h/.claude/settings.json",
});

/** 按命令名答（`ext-list` 依次答给定的几份）。 */
function backend(lists: unknown[], apply: () => unknown = () => chanReply({ path: "/g", changed: ["SKILL.md"], note: null }), hooks: (origin: string) => unknown = () => diag(null)) {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call") return undefined;
    switch (a.op) {
      case "ext-list":
        return chanReply(lists.length > 1 ? lists.shift() : lists[0]);
      case "assets-sync":
        return chanReply(synced);
      case "ext-hub-preview":
        return chanReply(card);
      case "ext-hub-apply":
        return apply();
      case "ext-note-set":
        return chanReply({ note: (chanArgsJson(a) as { text: string }).text || null });
      case "hooks-diag":
        return chanReply(hooks(a.origin));
    }
    throw new Error(`没料到这一问：${a.op}`);
  });
}

async function page(lists: unknown[], apply?: () => unknown): Promise<ExtSection> {
  backend(lists, apply);
  const s = new ExtSection();
  document.body.replaceChildren(s.element);
  s.loadNow();
  await settle();
  return s;
}

const dots = (s: ExtSection, name: string) =>
  [...s.element.querySelectorAll(`.ext-row[data-key$="/${name}"] .ext-dot`)].map((d) => [d.textContent, (d as HTMLElement).title]);
const open = (s: ExtSection, key: string) => (s.element.querySelector(`.ext-row[data-key="${key}"]`) as HTMLButtonElement).click();
const machineLines = (s: ExtSection) => [...s.element.querySelectorAll(".ext-drawer .ext-machine")];
/** 抽屉里勾那一台（键 ＝ 机器名；本机是空串）。 */
const pick = (s: ExtSection, key: string) => (s.element.querySelector(`.ext-drawer .ext-pick[data-machine="${key}"]`) as HTMLInputElement).click();
const installBox = (s: ExtSection) => s.element.querySelector(".ext-install")!;
const installBtn = (s: ExtSection) => installBox(s).querySelector<HTMLButtonElement>('[data-action="install-many"]')!;

describe("扩展页：表 · 抽屉 · 确认卡", () => {
  beforeEach(() => invokeMock.mockReset());

  it("表：一行一个条目，每台一个点（悬停是机器名 ＋ 态），新见到的行上一个「新」、有备注的带一个记号；搜索与种类筛选只看后端给的字", async () => {
    const noted = row("skill", "noted", [here, demoMissing], { note: "我写的" });
    const s = await page([listWith(demoMissing, [noted])]);
    expect(dots(s, "demo")).toEqual([
      ["●", `${copyText("extPage.machine.here")}：${copyText("extPage.state.same")}`],
      ["○", `laptop：${copyText("extPage.state.missing")}`],
    ]);
    const rows = () => [...s.element.querySelectorAll(".ext-row[data-key]")].map((r) => r.getAttribute("data-key"));
    expect(rows()).toEqual(["skill/demo", "mcp/fs", "skill/noted"]);
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"] .ext-new')?.textContent).toBe(copyText("extPage.row.new"));
    expect(s.element.querySelector('.ext-row[data-key="mcp/fs"] .ext-new')).toBeNull();
    const marked = [...s.element.querySelectorAll(".ext-row[data-key]")].filter((r) => r.querySelector(".ext-note-mark")).map((r) => r.getAttribute("data-key"));
    expect(marked).toEqual(["skill/noted"]);
    const search = s.element.querySelector(".ext-search") as HTMLInputElement;
    search.value = "npx";
    search.dispatchEvent(new Event("input"));
    expect(rows()).toEqual(["mcp/fs"]);
    search.value = "";
    search.dispatchEvent(new Event("input"));
    const [, skillOnly] = [...s.element.querySelectorAll(".ext-filter button")] as HTMLButtonElement[];
    skillOnly.click();
    expect(rows()).toEqual(["skill/demo", "skill/noted"]);
    // 进页那一问算「来看了一次」，随后同步一趟、再读（不算来看）。
    expect(ops()).toEqual([
      ["<local>", "ext-list", { visit: true }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
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
      [copyText("extPage.machine.here"), false],
      ["laptop", true],
    ]);
    expect(s.element.querySelector(".ext-offline")?.textContent).toBe(copyText("extPage.offline.bar", { machine: "laptop" }));
    expect([...s.element.querySelectorAll('.ext-row[data-key="skill/demo"] .ext-dot')].map((d) => d.classList.contains("is-offline"))).toEqual([false, true]);
    expect([...s.element.querySelectorAll(".ext-legend-item")].map((i) => i.textContent)).toEqual(
      [
        `${copyText("extPage.dot.same")}${copyText("extPage.legend.same")}`,
        `${copyText("extPage.dot.differs")}${copyText("extPage.legend.differs")}`,
        `${copyText("extPage.dot.missing")}${copyText("extPage.state.missing")}`,
        `${copyText("extPage.dot.project")}${copyText("extPage.legend.project")}`,
      ],
    );
    open(s, "skill/demo");
    expect((machineLines(s)[1].querySelector(".ext-pick") as HTMLInputElement).disabled, "连不上的那台勾不了").toBe(true);
  });

  it("skill 某一处有目录 ⇒ 本机那一处多一颗「在文件夹中显示」（系统文件管理器选中那个目录）、远端那一处多一颗「在文件窗口里打开」", async () => {
    const there = cell("same", [place(user, "same", true, null, "/g/.claude/skills/demo")]);
    const list = listWith(there);
    list.rows[0].cells[0] = cell("same", [place(user, "same", true, null, "/h/.claude/skills/demo")]);
    const s = await page([list]);
    open(s, "skill/demo");
    const per = machineLines(s).map((l) => [...l.querySelectorAll(".ext-place button")].map((b) => b.textContent));
    // 只有全局一处 ⇒「卸载…」跟在机器那一行后面，那一处下面只剩看目录的那一颗。
    expect(per).toEqual([[copyText("extPage.button.reveal")], [copyText("extPage.button.openFiles")]]);
    expect(machineLines(s).map((l) => [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent))).toEqual([
      [copyText("extPage.button.uninstall")],
      [copyText("extPage.button.uninstall")],
    ]);
    (machineLines(s)[0].querySelector(".ext-place button[data-reveal]") as HTMLButtonElement).click();
    await settle();
    expect(revealed).toEqual(["/h/.claude/skills/demo"]);
  });

  it("勾上一台 ⇒ 照建议的那一处问它一张卡；装到哪由用户选（改选 ⇒ 勾上的几台按那一处重看）；［装到 N 台］⇒ 交回各台卡上的记号与选的那一处 ⇒ 各台同步一趟、重读，点变 ◎", async () => {
    const after = cell("project", [place(user, "missing"), place(proj, "same", true)], bring());
    const s = await page([listWith(demoMissing), listWith(demoMissing), listWith(after)]);
    open(s, "skill/demo");
    expect(installBtn(s).disabled, "没勾不给装").toBe(true);
    invokeMock.mockClear();
    pick(s, "laptop");
    await settle();
    const ask0 = { kind: "skill", name: "demo", from: null, to: "laptop", scope: { from: user, to: user } };
    expect(ops()).toEqual([["<local>", "ext-hub-preview", ask0]]);
    const radios = () => [...installBox(s).querySelectorAll<HTMLInputElement>(".ext-targets input[type=radio]")];
    expect(radios().map((r) => [r.checked, r.disabled])).toEqual([
      [true, false],
      [false, false],
    ]);
    expect([...installBox(s).querySelectorAll(".ext-install-files")].map((l) => l.textContent)).toEqual([
      copyText("extPage.install.filesLine", { machine: "laptop", path: "/g/.claude/skills/demo", files: "SKILL.md" }),
    ]);
    invokeMock.mockClear();
    radios()[1].checked = true;
    radios()[1].dispatchEvent(new Event("change"));
    await settle();
    const ask = { ...ask0, scope: { from: user, to: proj } };
    expect(ops()).toEqual([["<local>", "ext-hub-preview", ask]]);
    expect(radios().map((r) => r.checked)).toEqual([false, true]);
    expect(installBtn(s).textContent).toBe(copyText("extPage.install.confirm", { n: 1 }));
    invokeMock.mockClear();
    installBtn(s).click();
    await settle();
    expect(ops()).toEqual([
      ["<local>", "ext-hub-apply", { ...ask, tokens: { source: "s", target: "t" }, fill: {} }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(dots(s, "demo").map(([d]) => d)).toEqual(["●", "◎"]);
    expect(installBox(s).querySelector('[data-result="laptop"]')?.textContent).toBe(copyText("extPage.install.doneOne", { machine: "laptop", said: copyText("extPage.done.written", { n: "1" }) }));
  });

  it("不能选的那一处（MCP 的全局）照列、置灰、旁注后端给的那一句；建议的那一处是能选的", async () => {
    const s = await page([listWith(demoMissing)]);
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

  it("看过之后变了（后端答 stale）⇒ 那台一行说一句、一个字节不写，那台自己重看一张卡（再点就是重试这台）", async () => {
    const s = await page([listWith(demoMissing)], () => {
      throw refusedReply("stale", "看过之后又变了，一个字节都没写。重看一次再装。");
    });
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    invokeMock.mockClear();
    installBtn(s).click();
    await settle();
    expect(installBox(s).querySelector('[data-result="laptop"]')!.textContent).toContain("看过之后又变了");
    expect(ops().filter(([, op]) => op === "ext-hub-preview").length, "没成的那台重看一张卡").toBe(1);
    expect(installBtn(s).disabled).toBe(false);
  });

  it("后端答了一个错误 ⇒ 那台那一行就是「装到 <那台> 失败：」＋ 它那一句本身，码不上屏；「装到哪」留着可以换一处", async () => {
    const said = copyText("beMcpEdit.path.notAbsolute", { dir: "w/x" });
    backend([listWith(demoMissing)]);
    const answer = invokeMock.getMockImplementation() as (cmd: string, a: ChanCallArgs) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd === "chan_call" && a.op === "ext-hub-preview") throw refusedReply("bad_path", said);
      return answer(cmd, a);
    });
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "skill/demo");
    pick(s, "laptop");
    await settle();
    expect(installBox(s).querySelector('.ext-install-files[data-machine="laptop"]')!.textContent).toBe(copyText("extPage.error.install", { machine: "laptop", said }));
    expect(installBox(s).querySelectorAll(".ext-targets input").length).toBe(2);
    expect(installBtn(s).disabled).toBe(true);
  });
});

describe("备注 · cc-bus 那一行", () => {
  beforeEach(() => invokeMock.mockReset());
  const ccBus = (cells: unknown[], note: string | null = null) => row("skill", "cc-bus", cells, { builtin: { note: "内置：要加两条钩子", hooks: true }, note });

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
    (s.element.querySelector(".ext-notes .settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    expect(ops().filter(([, op]) => op !== "hooks-diag")).toEqual([
      ["<local>", "ext-note-set", { kind: "skill", name: "cc-bus", text: "我的话" }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(s.element.querySelector(".ext-drawer .ext-note-user")?.textContent).toBe("我的话");
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
      ["not-installed", "installed-at-path"],
      ["not-installed", "installed-at-path"],
    ]);
    expect(lines[0].querySelector(".ext-hook-state")?.className).toBe("ext-hook-state is-bad");
    expect((lines[0].querySelector(".paste-block-out") as HTMLTextAreaElement).value).toBe(snippet);
    expect(lines[0].querySelector(".paste-block-target")?.textContent).toContain("/h/.claude/settings.json");
    expect(lines[1].querySelector(".paste-block-out")).toBeNull();
    expect(lines[1].querySelector(".ext-hook-install-first")?.textContent).toBe(copyText("extPage.hooks.installFirst"));
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

  it("cc-bus 装到本机之后：顺手问一次本机的命令够不够新，不够就在那一句后面说", async () => {
    const list = listWith(demoMissing, [ccBus([cell("missing", [place(user, "missing")], bring({ fromName: "自带" })), demoMissing])]);
    backend([list]);
    const answer = invokeMock.getMockImplementation() as (cmd: string, a: ChanCallArgs) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => (cmd === "cc_bus_ccm_precheck" ? "本机 ccm 太旧" : answer(cmd, a)));
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "skill/cc-bus");
    await settle();
    pick(s, "");
    await settle();
    installBtn(s).click();
    await settle();
    expect(invokeMock.mock.calls.filter(([c]) => c === "cc_bus_ccm_precheck")).toHaveLength(1);
    expect(installBox(s).querySelector('[data-result=""]')?.textContent).toBe(
      `${copyText("extPage.install.doneOne", { machine: copyText("extPage.machine.here"), said: copyText("extPage.done.written", { n: "1" }) })} ${copyText("extPage.done.warn", { said: "本机 ccm 太旧" })}`,
    );
  });
});

/** 一份源码里「比较指纹」的痕迹：出现摘要这个词，或拿记号去比。注释先剥掉。 */
export function comparesFingerprints(src: string): string[] {
  const code = src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
  const hits: string[] = [];
  for (const m of code.matchAll(/[^\n]*\bdigest\b[^\n]*/gi)) hits.push(m[0].trim());
  for (const m of code.matchAll(/[^\n]*\btokens?\b[^\n]*(===|!==|==|!=)[^\n]*/g)) hits.push(m[0].trim());
  return hits;
}

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

  it("★ 勾两台 ⇒ 一张卡、那一格只出一次（哪台已有照它那张卡说）；填一次 ⇒ 两台交上去的都是它；一台没成不挡另一台", async () => {
    const m3 = [...machines, { key: "nano", here: false, reachable: true, name: "nano", projects: [] }];
    const mcpMissing = cell("missing", [place(user, "missing")], bring());
    const lst = { machines: m3, problems: [], rows: [row("mcp", "srv", [cell("same", [place(user, "same", true)]), mcpMissing, mcpMissing])] };
    const applied: Array<{ to: unknown; fill: unknown }> = [];
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd !== "chan_call") return undefined;
      const body = chanArgsJson(a) as { to?: string; fill?: unknown };
      switch (a.op) {
        case "ext-list":
          return chanReply(lst);
        case "assets-sync":
          return chanReply(synced);
        case "ext-hub-preview":
          return chanReply({ ...card, kind: "mcp", name: "srv", slots: [{ field: "env", key: "API_KEY", kept: body.to === "nano" }] });
        case "ext-hub-apply":
          applied.push({ to: body.to, fill: body.fill });
          if (body.to === "nano") throw refusedReply("unreachable", "nano 断开了");
          return chanReply({ path: "/g", changed: ["x"], note: null });
      }
      throw new Error("unexpected " + a.op);
    });
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "mcp/srv");
    pick(s, "laptop");
    pick(s, "nano");
    await settle();
    const secrets = [...installBox(s).querySelectorAll<HTMLInputElement>(".ext-card-secret")];
    expect(secrets.length, "那一格只出一次").toBe(1);
    expect(installBox(s).textContent).toContain(copyText("extPage.install.kept", { machines: "nano" }));
    secrets[0].value = "sk-123";
    secrets[0].dispatchEvent(new Event("input"));
    expect(installBtn(s).textContent).toBe(copyText("extPage.install.confirm", { n: 2 }));
    installBtn(s).click();
    await settle();
    expect(applied.map((x) => [x.to, x.fill]).sort()).toEqual([
      ["laptop", { env: { API_KEY: "sk-123" } }],
      ["nano", { env: { API_KEY: "sk-123" } }],
    ]);
    expect(installBox(s).querySelector('[data-result="laptop"]')?.classList.contains("ext-done")).toBe(true);
    expect(installBox(s).querySelector('[data-result="nano"]')?.textContent).toContain("nano 断开了");
    expect(installBtn(s).textContent, "再点只重试没成的那台").toBe(copyText("extPage.install.confirm", { n: 1 }));
  });
});
