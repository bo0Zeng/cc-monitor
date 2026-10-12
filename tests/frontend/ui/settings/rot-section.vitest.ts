/**
 * 设置 → 机器 → 轮换：配置列表 · 配置编辑器（同一个文档做法：草稿 ＋ 保存）· 策略列表 · 试跑 · 源码。
 * 读写都经 `rot-reads.ts`（这里桩住）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const readConfigs = vi.fn();
const readStrategies = vi.fn();
const saveConfig = vi.fn();
const setDefaultConfig = vi.fn();
const deleteConfig = vi.fn();
const tryStrategy = vi.fn();
const readPlan = vi.fn();
vi.mock("../../../../src/frontend/ui/rot-reads", async (orig) => ({
  ...(await orig<typeof import("../../../../src/frontend/ui/rot-reads")>()),
  readConfigs: (...a: unknown[]) => readConfigs(...a),
  readStrategies: (...a: unknown[]) => readStrategies(...a),
  saveConfig: (...a: unknown[]) => saveConfig(...a),
  setDefaultConfig: (...a: unknown[]) => setDefaultConfig(...a),
  deleteConfig: (...a: unknown[]) => deleteConfig(...a),
  tryStrategy: (...a: unknown[]) => tryStrategy(...a),
  readPlan: (...a: unknown[]) => readPlan(...a),
  renameConfig: vi.fn(),
  addStrategy: vi.fn(),
  deleteStrategy: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/quota-reads", async (orig) => ({
  ...(await orig<typeof import("../../../../src/frontend/ui/quota-reads")>()),
  readQuota: vi.fn(async () => ({
    accounts: [{ account: "team", kind: "sub" }, { account: "api", kind: "api" }],
    unseen: [{ account: "lab", kind: "sub" }],
    candidates: [{ agent: "claude-code", account: "team", kind: "sub" }, { agent: "claude-code", account: "api", kind: "api" }, { agent: "claude-code", account: "lab", kind: "sub" }],
  })),
}));
vi.mock("../../../../src/frontend/ui/events", () => ({ bindEvents: vi.fn(async () => {}) }));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({ readRemoteConfig: vi.fn(async () => ({ hosts: [] })) }));

import { RotationSection } from "../../../../src/frontend/ui/settings/rot-section";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { ConfigItem } from "../../../../src/frontend/ui/generated/ConfigItem";
import type { DeclShown } from "../../../../src/frontend/ui/generated/DeclShown";
import type { StrategiesRead } from "../../../../src/frontend/ui/generated/StrategiesRead";

const ACCT: DeclShown[] = [
  { k: "line", type: "line", init: 90, label: "L-line" },
  { k: "mode", type: "choice", init: "normal", of: ["normal", "standby"], label: "L-mode", options: [{ v: "normal", label: "O-normal" }, { v: "standby", label: "O-standby" }] },
];
const WHOLE: DeclShown[] = [{ k: "delay", type: "minutes", init: 0, min: 0, max: 120, label: "L-delay" }];
const STRATS: StrategiesRead = {
  lib: { source: "// L1\n// L2" },
  strategies: [
    { id: "1", name: "S-one", builtin: true, ordered: true, text: "D-s1", params: WHOLE, acctParams: ACCT, source: "fn a() {}", usedBy: ["c1"] },
    { id: "night", name: "S-night", builtin: false, path: "/p/night.rhai", ordered: false, text: "", params: [], acctParams: [], source: "x\ny\nz", error: { kind: "failed", line: 2, msg: "m", text: "E-night" }, usedBy: [] },
  ],
};
const cfg = (id: string, name: string, over: Partial<ConfigItem> = {}): ConfigItem => ({
  id,
  name,
  text: `D-${id}`,
  strategyName: "S-one",
  inUse: [],
  isDefault: false,
  rev: 3,
  updatedAt: 0,
  savedBy: "settings",
  strategy: "1",
  accounts: [
    { start: true, params: { line: 90, mode: "normal" } },
    { id: "team", params: { line: 99, mode: "normal" } },
    { id: "api", params: { line: 90, mode: "standby" } },
  ],
  params: { delay: 0 },
  acctInit: { line: 90, mode: "normal" },
  parked: {},
  ...over,
});
const CONFIGS = { state: "present" as const, reason: null, detail: null, defaultConfig: "c1", configs: [cfg("c1", "C-one", { isDefault: true, inUse: ["s1"] }), cfg("c2", "C-two")] };

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
};
const confirm = vi.fn(async () => true);

async function mount(): Promise<HTMLElement> {
  const sec = new RotationSection({ confirm });
  document.body.replaceChildren(sec.element);
  sec.loadNow();
  await flush();
  return sec.element;
}
const q = <T extends HTMLElement = HTMLElement>(root: ParentNode, sel: string): T => root.querySelector<T>(sel)!;
const type = (i: HTMLInputElement, t: string): void => {
  i.value = t;
  i.dispatchEvent(new Event("change", { bubbles: true }));
};
const dialog = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"][aria-modal="true"]')!;
const btn = (root: ParentNode, text: string): HTMLButtonElement => [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text)!;
const menuItem = (text: string): HTMLElement => [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((m) => m.textContent?.startsWith(text))!;

beforeEach(() => {
  for (const f of [readConfigs, readStrategies, saveConfig, setDefaultConfig, deleteConfig, tryStrategy, confirm]) f.mockReset();
  readConfigs.mockResolvedValue(CONFIGS);
  readStrategies.mockResolvedValue(STRATS);
  saveConfig.mockResolvedValue({ state: "saved", id: "c1", rev: 4, savedAtText: "21:40" });
  setDefaultConfig.mockResolvedValue(undefined);
  deleteConfig.mockResolvedValue({ state: "done" });
  confirm.mockResolvedValue(true);
  readPlan.mockReset().mockReturnValue(new Promise(() => {}));
  document.body.replaceChildren();
});

describe("设置 · 轮换 · 配置", () => {
  it("配置列表：名 · 本机默认 · 描述 · 策略名 · N 会话；编辑器先开本机默认那份：号表一行一个号、列是每号参数、格写这个号自己的数", async () => {
    const root = await mount();
    const rows = [...root.querySelectorAll<HTMLElement>("[data-rot-config]")];
    expect(rows.map((r) => r.dataset.rotConfig)).toEqual(["c1", "c2"]);
    expect(rows[0].textContent).toContain(copyText("rot.cfg.tagDefault"));
    expect(rows[0].textContent).toContain("D-c1");
    expect(rows[0].textContent).toContain(copyText("rot.cfg.inUse", { n: 1 }));
    expect(q(root, "[data-rot-editor]").dataset.rotEditor).toBe("c1");
    const team = q(root, '[data-rot-row="team"]');
    expect(team.textContent).toContain(copyText("rot.ed.pct", { n: 99 }));
    expect(q(root, '[data-rot-row="api"]').textContent).toContain("O-standby");
    expect(q(root, '[data-rot-row="@start"]').textContent).toContain(copyText("rot.ed.rowStart"));
    expect(q(root, '[data-rot-row="@init"]')).not.toBeNull();
    expect(root.querySelectorAll("[data-rot-handle]").length, "ordered 的策略：行首有把手").toBe(3);
  });

  it("改一格只改草稿（出 已改），保存 ⇒ 带打开时的版本写回整份草稿", async () => {
    const root = await mount();
    q(root, '[data-rot-row="team"]').click();
    type(q<HTMLInputElement>(root, '[data-rot-row-form="team"] input[data-line="all"]'), "95");
    expect(saveConfig).not.toHaveBeenCalled();
    expect(q(root, "[data-rot-editor]").textContent).toContain(copyText("rot.doc.dirty"));
    q<HTMLButtonElement>(root, "[data-rot-save]").click();
    await flush();
    const [, args] = saveConfig.mock.calls[0] as [string, { id: string; ifRev: number; config: { accounts: { id?: string; params: Record<string, unknown> }[] } }];
    expect(args.id).toBe("c1");
    expect(args.ifRev).toBe(3);
    expect(args.config.accounts.find((r) => r.id === "team")?.params.line).toBe(95);
  });

  it("保存被拒 ⇒ 错落在那一格；撞上别处的新版 ⇒ 问，覆盖 ⇒ 带 force 再存", async () => {
    const root = await mount();
    q<HTMLInputElement>(root, "[data-rot-name]").value = "";
    q<HTMLInputElement>(root, "[data-rot-name]").dispatchEvent(new Event("change"));
    saveConfig.mockResolvedValueOnce({ state: "refused", errors: [{ cell: "name", code: "empty", said: "S-empty" }, { cell: "accounts[1].line", code: "range", said: "S-range" }] });
    q<HTMLButtonElement>(root, "[data-rot-save]").click();
    await flush();
    expect(q(root, "[data-rot-editor]").textContent).toContain("S-empty");
    expect(q(root, '[data-rot-row-form="team"]').textContent).toContain("S-range");
    saveConfig.mockResolvedValueOnce({ state: "conflict", rev: 5, theirs: [{ k: "delay", from: 0, to: 5, text: "T-theirs" }] });
    q<HTMLButtonElement>(root, "[data-rot-save]").click();
    await flush();
    expect(dialog().textContent).toContain("T-theirs");
    btn(dialog(), copyText("rot.doc.overwrite")).click();
    await flush();
    expect((saveConfig.mock.calls.at(-1) as [string, { force?: boolean }])[1].force).toBe(true);
  });

  it("勾两行 ⇒ 已选 2 个号 ＋ 全部改成（只改填了的格，进草稿）", async () => {
    const root = await mount();
    for (const id of ["team", "api"]) q<HTMLInputElement>(root, `[data-rot-row="${id}"] input[type=checkbox]`).click();
    const bar = q(root, "[data-rot-bulk]");
    expect(bar.textContent).toContain(copyText("rot.bulk.picked", { n: 2 }));
    type(q<HTMLInputElement>(bar, 'input[data-bulk="line"]'), "80");
    q<HTMLButtonElement>(bar, "[data-rot-bulk-apply]").click();
    expect(q(root, '[data-rot-row="team"]').textContent).toContain(copyText("rot.ed.pct", { n: 80 }));
    expect(q(root, '[data-rot-row="api"]').textContent).toContain(copyText("rot.ed.pct", { n: 80 }));
    expect(q(root, '[data-rot-row="@start"]').textContent).toContain(copyText("rot.ed.pct", { n: 90 }));
  });

  it("草稿改过时换开另一份 ⇒ 先问；⋯：设为本机默认 · 删除（先确认）", async () => {
    const root = await mount();
    type(q<HTMLInputElement>(root, '[data-rot-form$="whole"] [data-rot-field="delay"] input'), "5");
    q(root, '[data-rot-config="c2"]').click();
    await flush();
    expect(dialog().textContent).toContain(copyText("rot.doc.askTitle", { name: "C-two" }));
    btn(dialog(), copyText("rot.doc.discard")).click();
    await flush();
    expect(q(root, "[data-rot-editor]").dataset.rotEditor).toBe("c2");
    q<HTMLButtonElement>(root, '[data-rot-config="c2"] button').click();
    menuItem(copyText("rot.set.makeDefault")).click();
    await flush();
    expect(setDefaultConfig).toHaveBeenCalledWith("<local>", "c2");
    q<HTMLButtonElement>(root, '[data-rot-config="c2"] button').click();
    menuItem(copyText("rot.set.delete")).click();
    await flush();
    expect(confirm).toHaveBeenCalled();
    expect(deleteConfig).toHaveBeenCalledWith("<local>", "c2");
  });
});

describe("设置 · 轮换 · 策略", () => {
  it("顶上一行 lib · 共用小函数（引擎）；内置 / 本机标；读不懂的照实写那一句；详情写声明的参数与用它的配置；试跑列出排出的先后", async () => {
    const root = await mount();
    q<HTMLButtonElement>(root, '[data-rot-tabs] [data-key="strategies"]').click();
    const rows = [...root.querySelectorAll<HTMLElement>("[data-rot-strategy]")];
    expect(rows.map((r) => r.dataset.rotStrategy)).toEqual(["@lib", "1", "night"]);
    expect(rows[0].textContent).toContain(copyText("rot.st.libKind"));
    expect(rows[1].textContent).toContain(copyText("rot.st.builtin"));
    expect(rows[2].textContent).toContain(copyText("rot.st.local"));
    expect(rows[2].textContent).toContain("E-night");
    const det = q(root, "[data-rot-strategy-detail]");
    expect(det.textContent).toContain(copyText("rot.st.decl", { label: "L-delay", init: "0" }));
    expect(det.textContent).toContain("C-one");
    tryStrategy.mockResolvedValueOnce({ rows: [{ account: "team", p: 1000, why: "W-team" }, { account: "api", p: null, why: "W-api" }], pick: "team", ops: 1200, maxOps: 200000 });
    q<HTMLButtonElement>(root, "[data-rot-try]").click();
    await flush();
    expect(tryStrategy).toHaveBeenCalledWith("<local>", { config: "c1", id: "1" });
    const tried = q(root, "[data-rot-tried]");
    expect([...tried.querySelectorAll<HTMLElement>("[data-rot-try-row]")].map((r) => r.dataset.rotTryRow)).toEqual(["team", "api"]);
    expect(tried.textContent).toContain(copyText("rot.row.skip", { why: "W-api" }));
    expect(tried.textContent).toContain(copyText("rot.st.tryPick", { acct: "team", ops: 1200, max: 200000 }));
  });

  it("读不懂的那份：源码里出错那一行标出来；lib 那一行只看源码", async () => {
    const root = await mount();
    q<HTMLButtonElement>(root, '[data-rot-tabs] [data-key="strategies"]').click();
    q(root, '[data-rot-strategy="night"]').click();
    const lines = [...root.querySelectorAll<HTMLElement>("[data-rot-source] > span")];
    expect(lines.map((l) => l.dataset.error ?? "")).toEqual(["", "true", ""]);
    expect(q<HTMLButtonElement>(root, "[data-rot-try]").disabled).toBe(true);
    q(root, '[data-rot-strategy="@lib"]').click();
    expect(q(root, "[data-rot-strategy-detail]").dataset.rotStrategyDetail).toBe("@lib");
    expect(root.querySelectorAll("[data-rot-source] > span").length).toBe(2);
  });
});
