// 机器页「别名与配置文件」栏的「账号与扩展」那一组（`设计稿/机器配置-v2.md` §5）。守的要求：
// 构造零 I/O、露出来才问 · 共用 MCP 只画那台后端答的名字与各版、只交意图（删哪一条 · 用哪一版 · 停 / 开同步）·
// 删一条先问 · 停着同步时不给删、改说「已停止同步」· 扩展那一行只数那台那一列（不判）·「去扩展页」带目的地冒泡。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
const askConfirmMock = vi.fn();
vi.mock("../../../../src/frontend/ui/kit/dialog", () => ({ confirmDialog: (s: { body?: string }) => askConfirmMock(s.body) }));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { chanArgsJson, chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { buildConfigPage } from "../../../../src/frontend/ui/settings/config-page";
import { CONFIG_SHOWN_EVENT, SETTINGS_GO_EVENT } from "../../../../src/frontend/ui/settings/events";
import { decodeAccountMcpView } from "../../../../src/frontend/ui/account-ops";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const VIEW = {
  enabled: true,
  sync: true,
  servers: ["anysearch", "cclsp"],
  conflicts: [
    {
      name: "cclsp",
      choices: [
        { from: null, holders: ["q"], gone: false },
        { from: "z", holders: ["z", "b"], gone: false },
      ],
    },
  ],
  changed: [],
  notes: ["b 号的配置读不出来，这一次不同步它。"],
};

const cell = (state: string) => ({ state, places: [{ at: { level: state === "project" ? "project" : "user", ...(state === "project" ? { dir: "/p" } : {}) }, state: state === "project" ? "same" : state, dir: null, uninstall: false, note: null }], bring: null, note: null });
const row = (kind: string, name: string, here: string, there: string) => ({ kind, name, about: null, detail: [], new: false, builtin: null, note: null, cells: [cell(here), cell(there)] });
const EXT = {
  machines: [
    { key: null, here: true, reachable: true, name: "本机", projects: [] },
    { key: "devbox", here: false, reachable: true, name: "devbox", projects: [] },
  ],
  rows: [row("skill", "cc-bus", "same", "same"), row("skill", "video-read", "missing", "same"), row("mcp", "postgres", "missing", "project")],
  problems: [],
};

const calls: Array<[string, string, unknown]> = [];
let mcp: Record<string, unknown> = VIEW;

beforeEach(() => {
  calls.length = 0;
  mcp = VIEW;
  invokeMock.mockReset();
  askConfirmMock.mockReset();
  askConfirmMock.mockResolvedValue(true);
  invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
    if (cmd !== "chan_call") return Promise.reject(new Error(`no ${cmd}`));
    calls.push([args.op, args.origin, chanArgsJson(args)]);
    if (args.op === "ext-list") return Promise.resolve(chanReply(EXT));
    if (args.op === "accounts-mcp-sync") return Promise.resolve(chanReply({ ...VIEW, sync: (chanArgsJson(args) as { on: boolean }).on, conflicts: [] }));
    if (args.op.startsWith("accounts-mcp-")) return Promise.resolve(chanReply(args.op === "accounts-mcp-read" ? mcp : { ...VIEW, conflicts: [], changed: ["q"] }));
    return Promise.reject(new Error(`no ${args.op}`));
  });
});

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
}

function page(origin = "devbox"): HTMLElement {
  const el = buildConfigPage({ platform: "posix", origin: () => origin, machine: () => origin });
  document.body.replaceChildren(el);
  return el;
}

const show = (el: HTMLElement): void => {
  el.dispatchEvent(new CustomEvent(CONFIG_SHOWN_EVENT));
};
const mcpRow = (el: HTMLElement): HTMLElement => el.querySelector<HTMLElement>("[data-role=mcp-row]")!;
const buttons = (el: HTMLElement): HTMLButtonElement[] => Array.from(el.querySelectorAll("button"));
const byText = (el: HTMLElement, t: string): HTMLButtonElement | undefined => buttons(el).find((b) => b.textContent === t);

describe("别名与配置文件 · 账号与扩展", () => {
  it("★ 构造零 I/O；露出来才问那台（共用 MCP 问 origin 那台，扩展表问本机那一张）", async () => {
    const el = page();
    await settle();
    expect(calls).toEqual([]);
    show(el);
    await settle();
    const ops = calls.map((c) => `${c[0]}@${c[1]}`).filter((o) => !o.startsWith("aliases-") && !o.startsWith("ccm-"));
    expect(ops.sort()).toEqual(["accounts-mcp-read@devbox", "ext-list@<local>"]);
  });

  it("共用 MCP 挂 shared-mcp 锚点；收着时说两边都改的那一条、给［选一版…］；点开列名字、冲突与提示、新会话生效", async () => {
    const el = page();
    show(el);
    await settle();
    const r = mcpRow(el);
    expect(r.dataset.anchor).toBe("shared-mcp");
    expect(r.querySelector(".cfg-status")!.textContent).toContain(copyText("cfgPage.mcp.conflict", { name: "cclsp" }));
    expect(byText(r, copyText("cfgPage.mcp.pickOpen"))).toBeDefined();
    byText(r, copyText("cfgPage.mcp.pickOpen"))!.click();
    const names = Array.from(r.querySelectorAll("[data-role=mcp-server] .cfg-list-name")).map((n) => n.textContent);
    expect(names).toEqual(["anysearch", "cclsp"]);
    const text = r.textContent ?? "";
    expect(text).toContain(copyText("cfgPage.mcp.conflictHead", { name: "cclsp" }));
    expect(text).toContain(VIEW.notes[0]);
    expect(text).toContain(copyText("cfgPage.mcp.newSessions"));
    expect(byText(r, copyText("cfgPage.mcp.pickOpen")), "展开后标题行那颗收掉").toBeUndefined();
  });

  it("删一条：先问，确认了才交名字", async () => {
    const el = page();
    show(el);
    await settle();
    const r = mcpRow(el);
    askConfirmMock.mockResolvedValueOnce(false);
    const del = () => buttons(r).filter((b) => b.textContent === copyText("cfgPage.mcp.remove"))[0];
    del().click();
    await settle();
    expect(calls.some((c) => c[0] === "accounts-mcp-remove")).toBe(false);
    del().click();
    await settle();
    expect(calls.find((c) => c[0] === "accounts-mcp-remove")).toEqual(["accounts-mcp-remove", "devbox", { name: "anysearch" }]);
  });

  it("挑一版：共享那一版不带 from，某个号那一版带它的名字", async () => {
    const el = page();
    show(el);
    await settle();
    const r = mcpRow(el);
    byText(r, copyText("cfgPage.mcp.useFrom", { holders: ["z", "b"].join(copyText("accountsMcp.list.sep")) }))!.click();
    await settle();
    expect(calls.find((c) => c[0] === "accounts-mcp-pick")).toEqual(["accounts-mcp-pick", "devbox", { name: "cclsp", from: "z" }]);
  });

  it("停止同步交 {on:false}；停着时现状说「已停止同步」、名字照列、不给删，按钮换成「开回同步」交 {on:true}", async () => {
    const el = page();
    show(el);
    await settle();
    const r = mcpRow(el);
    byText(r, copyText("cfgPage.mcp.stop"))!.click();
    await settle();
    expect(calls.find((c) => c[0] === "accounts-mcp-sync")).toEqual(["accounts-mcp-sync", "devbox", { on: false }]);
    expect(r.querySelector(".cfg-status")!.textContent).toBe(copyText("cfgPage.mcp.paused", { n: "2" }));
    expect(r.querySelectorAll("[data-role=mcp-server]").length).toBe(2);
    expect(byText(r, copyText("cfgPage.mcp.remove")), "停着不给删").toBeUndefined();
    byText(r, copyText("cfgPage.mcp.resume"))!.click();
    await settle();
    expect(calls.filter((c) => c[0] === "accounts-mcp-sync").map((c) => c[2])).toEqual([{ on: false }, { on: true }]);
  });

  it("扩展那一行只数那台那一列：skill 在的几条 · 只在项目里的 MCP；本机读本机那一列；［去扩展页］带 {page: ext} 冒泡", async () => {
    const remote = page("devbox");
    show(remote);
    await settle();
    const r = remote.querySelector<HTMLElement>("[data-role=ext-row]")!;
    expect(r.querySelector(".cfg-name")!.textContent).toBe(copyText("cfgPage.ext.title", { machine: "devbox" }));
    expect(r.querySelector(".cfg-status")!.textContent).toBe(copyText("cfgPage.ext.countWithMcp", { skills: "2", mcp: "1" }));
    const local = page("<local>");
    show(local);
    await settle();
    const l = local.querySelector<HTMLElement>("[data-role=ext-row]")!;
    expect(l.querySelector(".cfg-status")!.textContent).toBe(copyText("cfgPage.ext.count", { skills: "1" }));
    const got: unknown[] = [];
    document.addEventListener(SETTINGS_GO_EVENT, (e) => got.push((e as CustomEvent).detail), { once: true });
    byText(l, copyText("cfgPage.ext.go"))!.click();
    expect(got).toEqual([{ page: "ext" }]);
  });

  it("成品形状严格收：多一个键 / 缺 sync / 选项缺一格 ⇒ 读不懂", () => {
    expect(decodeAccountMcpView(VIEW)).not.toBeNull();
    expect(decodeAccountMcpView({ ...VIEW, extra: 1 })).toBeNull();
    const { sync: _s, ...noSync } = VIEW;
    expect(decodeAccountMcpView(noSync)).toBeNull();
    expect(decodeAccountMcpView({ ...VIEW, conflicts: [{ name: "x", choices: [{ from: null, holders: [] }] }] })).toBeNull();
  });
});
