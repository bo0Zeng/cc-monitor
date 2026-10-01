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
  { key: "gpd", here: false, reachable: true, name: "gpd", projects: ["/g/p"] },
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
    row("skill", "demo", [here, there], { about: "does demo", detail: [{ label: "文件", value: "1 个" }], new: true }),
    row("mcp", "fs", [cell("same", [place(user, "same", false, "只读")], bring({ scope: { from: user, to: proj }, targets: targets(false, "全局的只读") })), cell("missing", [place(user, "missing")], null, "没项目")], {
      about: "npx fs",
    }),
    ...extra,
  ],
});
const card = { kind: "skill", name: "demo", path: "/g/.claude/skills/demo", writes: ["SKILL.md"], unchanged: false, suspects: [], stop: null, config: null, slots: [], tokens: { source: "s", target: "t" } };
const synced = { self: "h", synced: [], reach: [{ origin: "gpd", machine: "g" }] };
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

describe("扩展页：表 · 抽屉 · 确认卡", () => {
  beforeEach(() => invokeMock.mockReset());

  it("表：一行一个条目，每台一个点（悬停是机器名 ＋ 态），新见到的行上一个「新」、有备注的带一个记号；搜索与种类筛选只看后端给的字", async () => {
    const noted = row("skill", "noted", [here, demoMissing], { note: "我写的" });
    const s = await page([listWith(demoMissing, [noted])]);
    expect(dots(s, "demo")).toEqual([
      ["●", `${copyText("extPage.machine.here")}：${copyText("extPage.state.same")}`],
      ["○", `gpd：${copyText("extPage.state.missing")}`],
    ]);
    const rows = () => [...s.element.querySelectorAll(".ext-row")].map((r) => r.getAttribute("data-key"));
    expect(rows()).toEqual(["skill/demo", "mcp/fs", "skill/noted"]);
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"] .ext-new')?.textContent).toBe("新");
    expect(s.element.querySelector('.ext-row[data-key="mcp/fs"] .ext-new')).toBeNull();
    const marked = [...s.element.querySelectorAll(".ext-row")].filter((r) => r.querySelector(".ext-note-mark")).map((r) => r.getAttribute("data-key"));
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

  it("抽屉：每台一行展开成它的各处（全局 ＋ 每个装着它的项目），各自的态与「卸载」；机器那一行一个「装到…」，没有它的说后端给的那一句", async () => {
    const both = cell("project", [place(user, "missing"), place(proj, "same", true)], bring());
    const s = await page([listWith(both)]);
    open(s, "mcp/fs");
    const lines = machineLines(s);
    expect(lines.map((l) => [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent))).toEqual([[copyText("extPage.button.bring")], []]);
    expect(lines[1].querySelector(":scope > .settings-hint")?.textContent).toBe("没项目");
    expect([...lines[0].querySelectorAll(".ext-place .settings-hint")].map((h) => h.textContent)).toEqual(["只读"]);
    open(s, "skill/demo");
    const places = machineLines(s).map((l) =>
      [...l.querySelectorAll(".ext-place")].map((p) => [p.querySelector(".ext-place-at")?.textContent, [...p.querySelectorAll("button")].map((b) => b.textContent)]),
    );
    expect(places).toEqual([
      [[copyText("extPage.loc.user"), [copyText("extPage.button.uninstall")]]],
      [
        [copyText("extPage.loc.user"), []],
        [copyText("extPage.loc.project", { dir: "/g/p" }), [copyText("extPage.button.uninstall")]],
      ],
    ]);
  });

  it("skill 在一台远端上某一处有目录 ⇒ 那一处多一颗「在文件窗口里打开」（本机那一处不给：这一版文件窗口只开远端）", async () => {
    const there = cell("same", [place(user, "same", true, null, "/g/.claude/skills/demo")]);
    const list = listWith(there);
    list.rows[0].cells[0] = cell("same", [place(user, "same", true, null, "/h/.claude/skills/demo")]);
    const s = await page([list]);
    open(s, "skill/demo");
    const per = machineLines(s).map((l) => [...l.querySelectorAll(".ext-place button")].map((b) => b.textContent));
    expect(per).toEqual([[copyText("extPage.button.uninstall")], [copyText("extPage.button.uninstall"), copyText("extPage.button.openFiles")]]);
  });

  it("装到哪由用户选：卡上列后端给的各处（建议的那一处选中）；改选项目 ⇒ 按那一处重看一张卡；确认 ⇒ 交回卡上的记号与选的那一处 ⇒ 那台同步一趟、重读，点变 ◎", async () => {
    const after = cell("project", [place(user, "missing"), place(proj, "same", true)], bring());
    const s = await page([listWith(demoMissing), listWith(demoMissing), listWith(after)]);
    open(s, "skill/demo");
    (machineLines(s)[1].querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    const box = () => s.element.querySelector(".ext-card")!;
    expect(box().querySelector(".ext-card-title")?.textContent).toBe(copyText("extPage.card.bringTitle", { from: "本机", to: "gpd" }));
    const radios = () => [...box().querySelectorAll<HTMLInputElement>(".ext-targets input[type=radio]")];
    expect(radios().map((r) => [r.checked, r.disabled])).toEqual([
      [true, false],
      [false, false],
    ]);
    expect([...box().querySelectorAll(".ext-card-files li")].map((l) => l.textContent)).toEqual(["SKILL.md"]);
    invokeMock.mockClear();
    radios()[1].checked = true;
    radios()[1].dispatchEvent(new Event("change"));
    await settle();
    const ask = { kind: "skill", name: "demo", from: null, to: "gpd", scope: { from: user, to: proj } };
    expect(ops()).toEqual([["<local>", "ext-hub-preview", ask]]);
    expect(radios().map((r) => r.checked)).toEqual([false, true]);
    invokeMock.mockClear();
    (box().querySelector(".settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    expect(ops()).toEqual([
      ["<local>", "ext-hub-apply", { ...ask, tokens: { source: "s", target: "t" }, fill: {} }],
      ["<local>", "assets-sync", { origin: "gpd" }],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(dots(s, "demo").map(([d]) => d)).toEqual(["●", "◎"]);
    expect(s.element.querySelector(".ext-card")).toBeNull();
  });

  it("不能选的那一处（MCP 的全局）照列、置灰、旁注后端给的那一句；建议的那一处是能选的", async () => {
    const s = await page([listWith(demoMissing)]);
    open(s, "mcp/fs");
    (machineLines(s)[0].querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    const rows = [...s.element.querySelectorAll(".ext-card .ext-target")];
    expect(
      rows.map((r) => [r.classList.contains("is-off"), (r.querySelector("input") as HTMLInputElement).disabled, (r.querySelector("input") as HTMLInputElement).checked, r.querySelector(".ext-target-note")?.textContent ?? null]),
    ).toEqual([
      [true, true, false, "全局的只读"],
      [false, false, true, null],
    ]);
  });

  it("看过之后变了（后端答 stale）⇒ 在那台那一行说一句、给「重看」，一个字节不写", async () => {
    const s = await page([listWith(demoMissing)], () => {
      throw refusedReply("stale", "看过之后又变了，一个字节都没写。重看一次再装。");
    });
    open(s, "skill/demo");
    (machineLines(s)[1].querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    (s.element.querySelector(".ext-card .settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    const err = machineLines(s)[1].querySelector(".ext-error")!;
    expect(err.textContent).toContain("看过之后又变了");
    expect([...err.querySelectorAll("button")].map((b) => b.textContent)).toEqual([copyText("extPage.card.again")]);
  });

  it("后端答了一个错误 ⇒ 那一行就是「装到 <那台> 失败：」＋ 它那一句本身，码不上屏；「装到哪」留着可以换一处", async () => {
    const said = "项目目录「w/x」不是绝对路径。要从根目录或盘符写起，中间不能有 ..";
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
    (machineLines(s)[1].querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    expect(machineLines(s)[1].querySelector(".ext-error")!.textContent).toBe(copyText("extPage.error.install", { machine: "gpd", said }));
    expect(s.element.querySelectorAll(".ext-card .ext-targets input").length).toBe(2);
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
      ["gpd", "hooks-diag", {}],
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
    backend([list], undefined, (o) => diag(o === "gpd" ? null : '{"hooks":{}}', o !== "gpd"));
    const s = new ExtSection();
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    open(s, "skill/cc-bus");
    await settle();
    const gpd = [...s.element.querySelectorAll(".ext-hooks .ext-hook")][1];
    expect(gpd.querySelector(".ext-hook-unsupported")?.textContent).toBe(copyText("extPage.hooks.unsupported"));
    expect(gpd.querySelectorAll(".ext-hook-state, .paste-block-out")).toHaveLength(0);
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
    (machineLines(s)[0].querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    (s.element.querySelector(".ext-card .settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    expect(invokeMock.mock.calls.filter(([c]) => c === "cc_bus_ccm_precheck")).toHaveLength(1);
    expect(machineLines(s)[0].querySelector(".ext-done")?.textContent).toBe(
      `${copyText("extPage.done.written", { n: "1" })} ${copyText("extPage.done.warn", { said: "本机 ccm 太旧" })}`,
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
