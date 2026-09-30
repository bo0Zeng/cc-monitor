/**
 * 要求：设置「扩展」页 —— 一张表（搜索 · 种类筛选 · 一行一个条目 · 每台一个点 ● ◐ ○ ◎，悬停是机器名 · 新见到的行上一个「新」）
 * ＋ 一个抽屉（每台一行：态 ＋ 唯一那个按钮）＋ 一张确认卡（一个确认按钮；后端答 stale 就在卡上说一句、给「重看」）；
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
const machines = [
  { key: null, here: true, reachable: true, name: "me@h", projects: ["/h/p"] },
  { key: "laptop", here: false, reachable: true, name: "laptop", projects: ["/g/p"] },
];
const cell = (state: string, action: unknown = null, note: string | null = null) => ({ state, places: state === "missing" ? [] : [user], dir: null, action, note });
const install = { verb: "install", from: null, fromName: "me@h", scope: { from: user, to: user } };
const listWith = (there: unknown) => ({
  machines,
  problems: [],
  rows: [
    { kind: "skill", name: "demo", about: "does demo", detail: [{ label: "文件", value: "1 个" }], new: true, cells: [cell("same", { verb: "uninstall", at: user }), there] },
    { kind: "mcp", name: "fs", about: "npx fs", detail: [], new: false, cells: [cell("same", null, "只读"), cell("missing", null, "没项目")] },
  ],
});
const card = { kind: "skill", name: "demo", path: "/g/.claude/skills/demo", writes: ["SKILL.md"], unchanged: false, suspects: [], stop: null, config: null, slots: [], tokens: { source: "s", target: "t" } };
const synced = { self: "h", synced: [], reach: [{ origin: "laptop", machine: "g" }] };

/** 按命令名答（`ext-list` 依次答给定的几份）。 */
function backend(lists: unknown[], apply: () => unknown = () => chanReply({ path: "/g", changed: ["SKILL.md"], note: null })) {
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

describe("扩展页：表 · 抽屉 · 确认卡", () => {
  beforeEach(() => invokeMock.mockReset());

  it("表：一行一个条目，每台一个点（悬停是机器名 ＋ 态），新见到的行上一个「新」；搜索与种类筛选只看后端给的字", async () => {
    const s = await page([listWith(cell("missing", install))]);
    expect(dots(s, "demo")).toEqual([
      ["●", `${copyText("extPage.machine.here")}：${copyText("extPage.state.same")}`],
      ["○", `laptop：${copyText("extPage.state.missing")}`],
    ]);
    const rows = () => [...s.element.querySelectorAll(".ext-row")].map((r) => r.getAttribute("data-key"));
    expect(rows()).toEqual(["skill/demo", "mcp/fs"]);
    expect(s.element.querySelector('.ext-row[data-key="skill/demo"] .ext-new')?.textContent).toBe("新");
    expect(s.element.querySelector('.ext-row[data-key="mcp/fs"] .ext-new')).toBeNull();
    const search = s.element.querySelector(".ext-search") as HTMLInputElement;
    search.value = "npx";
    search.dispatchEvent(new Event("input"));
    expect(rows()).toEqual(["mcp/fs"]);
    search.value = "";
    search.dispatchEvent(new Event("input"));
    const [, skillOnly] = [...s.element.querySelectorAll(".ext-filter button")] as HTMLButtonElement[];
    skillOnly.click();
    expect(rows()).toEqual(["skill/demo"]);
    // 进页那一问算「来看了一次」，随后同步一趟、再读（不算来看）。
    expect(ops().map(([o, op, a]) => [o, op, a])).toEqual([
      ["<local>", "ext-list", { visit: true }],
      ["<local>", "assets-sync", {}],
      ["<local>", "ext-list", { visit: false }],
    ]);
  });

  it("抽屉：每台一行、态 ＋ 唯一一个按钮；没有按钮的那一格说后端给的那一句", async () => {
    const s = await page([listWith(cell("missing", install))]);
    (s.element.querySelector('.ext-row[data-key="mcp/fs"]') as HTMLButtonElement).click();
    const lines = [...s.element.querySelectorAll(".ext-drawer .ext-machine")];
    expect(lines.map((l) => l.querySelectorAll(".ext-machine-top button").length)).toEqual([0, 0]);
    expect(lines.map((l) => l.querySelector(".settings-hint")?.textContent)).toEqual(["只读", "没项目"]);
    (s.element.querySelector('.ext-row[data-key="skill/demo"]') as HTMLButtonElement).click();
    const buttons = [...s.element.querySelectorAll(".ext-drawer .ext-machine-top button")].map((b) => b.textContent);
    expect(buttons).toEqual([copyText("extPage.button.uninstall"), copyText("extPage.button.install")]);
  });

  it("skill 在一台远端上有目录 ⇒ 那一行多一颗「在文件窗口里打开」（本机那一行不给：这一版文件窗口只开远端）", async () => {
    const there = { ...cell("same", { verb: "uninstall", at: user }), dir: "/g/.claude/skills/demo" };
    const here = { ...cell("same", { verb: "uninstall", at: user }), dir: "/h/.claude/skills/demo" };
    const list = listWith(there);
    list.rows[0].cells[0] = here;
    const s = await page([list]);
    (s.element.querySelector('.ext-row[data-key="skill/demo"]') as HTMLButtonElement).click();
    const per = [...s.element.querySelectorAll(".ext-drawer .ext-machine")].map((l) =>
      [...l.querySelectorAll(".ext-machine-top button")].map((b) => b.textContent),
    );
    expect(per).toEqual([[copyText("extPage.button.uninstall")], [copyText("extPage.button.uninstall"), copyText("extPage.button.openFiles")]]);
  });

  it("装：点「装上」⇒ 卡（从哪装到哪 · 写哪几个）⇒ 确认 ⇒ 交回卡上的记号 ⇒ 那台同步一趟、重读，那一格的点变 ●", async () => {
    const s = await page([listWith(cell("missing", install)), listWith(cell("missing", install)), listWith(cell("same", { verb: "uninstall", at: user }))]);
    (s.element.querySelector('.ext-row[data-key="skill/demo"]') as HTMLButtonElement).click();
    const bring = [...s.element.querySelectorAll(".ext-drawer .ext-machine")][1];
    (bring.querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    const box = s.element.querySelector(".ext-card")!;
    expect(box.querySelector(".ext-card-title")?.textContent).toBe(copyText("extPage.card.bringTitle", { from: copyText("extPage.machine.here"), to: "laptop" }));
    expect([...box.querySelectorAll(".ext-card-files li")].map((l) => l.textContent)).toEqual(["SKILL.md"]);
    invokeMock.mockClear();
    (box.querySelector(".settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    expect(ops()).toEqual([
      ["<local>", "ext-hub-apply", { kind: "skill", name: "demo", from: null, to: "laptop", scope: { from: user, to: user }, tokens: { source: "s", target: "t" }, fill: {} }],
      ["<local>", "assets-sync", { origin: "laptop" }],
      ["<local>", "ext-list", { visit: false }],
    ]);
    expect(dots(s, "demo").map(([d]) => d)).toEqual(["●", "●"]);
    expect(s.element.querySelector(".ext-card")).toBeNull();
  });

  it("看过之后变了（后端答 stale）⇒ 在那台那一行说一句、给「重看」，一个字节不写", async () => {
    const s = await page([listWith(cell("missing", install))], () => {
      throw refusedReply("stale", "看过之后又变了，一个字节都没写。重看一次再装。");
    });
    (s.element.querySelector('.ext-row[data-key="skill/demo"]') as HTMLButtonElement).click();
    const bring = () => [...s.element.querySelectorAll(".ext-drawer .ext-machine")][1];
    (bring().querySelector(".ext-machine-top button") as HTMLButtonElement).click();
    await settle();
    (s.element.querySelector(".ext-card .settings-btn-primary") as HTMLButtonElement).click();
    await settle();
    const err = bring().querySelector(".ext-error")!;
    expect(err.textContent).toContain("看过之后又变了");
    expect([...err.querySelectorAll("button")].map((b) => b.textContent)).toEqual([copyText("extPage.card.again")]);
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

describe("界面层零判定：扩展页不比较指纹", () => {
  it("扩展页与它的读口两份源码里零处（正控：同一把尺子认得出一处现造的比较）", () => {
    expect(comparesFingerprints("if (a.digest === b.digest) x();")).toHaveLength(1);
    expect(comparesFingerprints("if (card.tokens.source !== t) x();")).toHaveLength(1);
    for (const f of ["src/frontend/ui/settings/ext-section.ts", "src/frontend/ui/ext-reads.ts"]) {
      expect(comparesFingerprints(readFileSync(resolve(REPO_ROOT, f), "utf8")), f).toEqual([]);
    }
  });
});
