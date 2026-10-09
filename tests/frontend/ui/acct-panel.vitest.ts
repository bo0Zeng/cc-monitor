/**
 * 「账号」面板的行为：勾号 · 无号可换两态 · 热切换 · 热切换不成立 · 跟随默认时只读。写都经 `quota-reads.ts`（这里桩住），
 * 数据只从 `appStore` 来（那几格由推送填）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const writeSessionRotation = vi.fn();
const checkRotation = vi.fn();
const saveRule = vi.fn();
const switchHot = vi.fn();
const switchRestart = vi.fn();
const runRemoteAttach = vi.fn();
const openLog = vi.fn();
vi.mock("../../../src/frontend/ui/quota-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/quota-reads")>()),
  writeSessionRotation: (...a: unknown[]) => writeSessionRotation(...a),
  switchHot: (...a: unknown[]) => switchHot(...a),
  switchRestart: (...a: unknown[]) => switchRestart(...a),
  readQuota: vi.fn(() => new Promise(() => {})),
  readRules: vi.fn(() => new Promise(() => {})),
  checkRotation: (...a: unknown[]) => checkRotation(...a),
  saveRule: (...a: unknown[]) => saveRule(...a),
  readSessionRotation: vi.fn(() => new Promise(() => {})),
}));

vi.mock("../../../src/frontend/ui/sessions-where", () => ({
  standingOf: vi.fn(async () => ({ kind: "running", names: ["proj-cc"], terminals: ["t1"] })),
}));
vi.mock("../../../src/frontend/ui/interrupt-reads", () => ({ askSessionInterrupts: vi.fn(async () => ({ families: [] })) }));
vi.mock("../../../src/frontend/ui/tab-batch-run", () => ({ startSettings: vi.fn(async () => ({})) }));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({ runRemoteAttach: (...a: unknown[]) => runRemoteAttach(...a) }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: { open_log_file: () => openLog() } }));

import { openAccountPanel, openAccountPanelAt, toggleAccountPanel, type AcctPanelHost } from "../../../src/frontend/ui/acct-panel.ts";
import { appStore } from "../../../src/frontend/ui/app-store.ts";
import type { QuotaRead } from "../../../src/frontend/ui/quota-lines.ts";
import type { Rotation } from "../../../src/frontend/ui/generated/Rotation.ts";
import type { SessionRotation } from "../../../src/frontend/ui/generated/SessionRotation.ts";
import type { RuleRow, RulesRead } from "../../../src/frontend/ui/quota-reads.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";

const NOW = 1_791_189_600;
const host: AcctPanelHost = {
  sessionTitle: () => "orders",
  cwdOf: () => "/w",
  agentOf: () => "claude",
  openSettings: vi.fn(),
  openDefaultMenu: vi.fn(),
  defaultOf: () => "work",
  openResume: vi.fn(),
};

const LEDGER: QuotaRead = {
  state: "present",
  reason: null,
  path: null,
  now: NOW,
  accounts: ["work", "team", "api"].map((account) => ({
    agent: "claude-code",
    account,
    seenAt: NOW - 60,
    reading: {},
    kind: account === "api" ? ("api" as const) : ("sub" as const),
    state: "ok" as const,
    stale: false,
    limiting: account === "api" ? undefined : "5h",
    slots: account === "api" ? [] : [{ slot: "5h", pct: 30, resetsAt: NOW + 3600 }],
    login: "ok" as const,
  })),
  unseen: [],
  usableNow: ["work", "team", "api"],
  earliestReturn: null,
};

/** 一张只有默认那一条的规则表（名字 `默认`）。 */
function rulesOf(rotation: Rotation, more: RuleRow[] = []): RulesRead {
  const row = (id: string, name: string, r: Rotation, isDefault: boolean): RuleRow => ({
    id,
    name,
    rotation: r,
    rev: 1,
    updatedAt: NOW,
    isDefault,
    users: { live: 1, ended: 0, follow: 1, sids: ["s1"] },
    summary: "",
    explain: "",
    missing: [],
    atLimitApplies: false,
  });
  return { state: "present", defaultRule: "r_def", rules: [row("r_def", "日常", rotation, true), ...more] };
}

function seed(p: Partial<SessionRotation>, custom: Rotation | undefined): void {
  appStore.quota.set(new Map([["<local>", LEDGER]]));
  appStore.rotationRules.set(new Map([["<local>", rulesOf({ order: [{ start: true }], enabled: [], when: "full", atLimit: "continue", wait: 40 })]]));
  appStore.sessionRotation.set(
    new Map([
      [
        "s1",
        {
          origin: "<local>",
          now: NOW,
          read: {
            state: "present",
            agent: "claude-code",
            source: custom === undefined ? "follow" : "custom",
            explain: "",
            custom,
            account: { start: "work", current: "work", since: NOW - 600, history: [], inPlace: "ok" },
            next: "team",
            atLimit: "continue",
            quota: { ...LEDGER.accounts[0] },
            ...p,
          },
        },
      ],
    ]),
  );
}

const panel = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"]')!;
const flush = async (): Promise<void> => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  writeSessionRotation.mockReset().mockResolvedValue({ s1: { state: "done" } });
  checkRotation.mockReset().mockResolvedValue([]);
  saveRule.mockReset();
  switchHot.mockReset().mockResolvedValue({ s1: { state: "done" } });
  switchRestart.mockReset();
  runRemoteAttach.mockReset().mockResolvedValue(undefined);
  openLog.mockReset().mockResolvedValue(undefined);
  vi.mocked(host.openResume).mockReset();
  if (document.querySelector('[role="dialog"]')) toggleAccountPanel("s1", "<local>", host);
  document.body.replaceChildren();
});

describe("账号面板", () => {
  it("本会话：勾上一个号 ⇒ 整份写回（顺序里没有的排末尾，挪按量号到末尾是后端的事）", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    const boxes = [...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    expect(boxes.map((b) => b.getAttribute("aria-label"))).toEqual([copyText("acct.row.checkAria", { name: "work" }), copyText("acct.row.checkAria", { name: "team" }), copyText("acct.row.checkAria", { name: "api" })]);
    expect(boxes[0].disabled, "起始 · 在用那一行锁着").toBe(true);
    boxes[2].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], { custom: { order: [{ start: true }, "team", "api"], enabled: ["team", "api"], when: "full", atLimit: "continue", wait: 40 } });
  });

  describe("勾 / 不勾只改 enabled，order 逐字不变（不凭空插「起始账号」那一格）", () => {
    const boxOf = (name: string): HTMLInputElement => panel().querySelector<HTMLInputElement>(`input[aria-label="${copyText("acct.row.checkAria", { name })}"]`)!;
    const lastWrite = (): Rotation => (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }).custom;

    it("没有占位的那份：勾上序外的号 ⇒ 只排到末尾、不插占位", async () => {
      seed({}, { order: ["team", "work"], enabled: ["team", "work"], when: "full", atLimit: "continue", wait: 40 });
      openAccountPanel("s1", "<local>", host);
      boxOf("api").click();
      await flush();
      expect(lastWrite().order).toEqual(["team", "work", "api"]);
      expect(lastWrite().enabled).toEqual(["team", "work", "api"]);
    });

    it("没有占位的那份：取消勾 ⇒ order 原样", async () => {
      seed({}, { order: ["team", "work"], enabled: ["team", "work"], when: "full", atLimit: "continue", wait: 40 });
      openAccountPanel("s1", "<local>", host);
      boxOf("team").click();
      await flush();
      expect(lastWrite().order).toEqual(["team", "work"]);
      expect(lastWrite().enabled).toEqual(["work"]);
    });

    it("占位 ＋ 起始号也具名在序里：取消勾别的号 ⇒ 具名那格不丢", async () => {
      seed({}, { order: [{ start: true }, "team", "work"], enabled: ["team", "work"], when: "full", atLimit: "continue", wait: 40 });
      openAccountPanel("s1", "<local>", host);
      boxOf("team").click();
      await flush();
      expect(lastWrite().order).toEqual([{ start: true }, "team", "work"]);
      expect(lastWrite().enabled).toEqual(["work"]);
    });

    it("没有占位的那份：Alt+↓ 移位 ⇒ 只换这两格，不插占位", async () => {
      seed({}, { order: ["team", "work"], enabled: ["team", "work"], when: "full", atLimit: "continue", wait: 40 });
      openAccountPanel("s1", "<local>", host);
      panel().querySelector<HTMLElement>('[data-acct-row="team"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", altKey: true, bubbles: true }));
      await flush();
      expect(lastWrite().order).toEqual(["work", "team"]);
    });
  });

  it("无号可换：「满」触发时两态灰着；≥N% 时点「停」⇒ 写 atLimit stop", async () => {
    seed({}, { order: [{ start: true }], enabled: [], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    const seg = (): HTMLButtonElement[] => [...panel().querySelectorAll<HTMLButtonElement>(`[aria-label="${copyText("acct.lim.aria")}"] button`)];
    expect(seg().map((b) => [b.textContent, b.disabled])).toEqual([[copyText("acct.lim.go"), true], [copyText("acct.lim.stop"), true]]);
    toggleAccountPanel("s1", "<local>", host);
    seed({}, { order: [{ start: true }], enabled: [], when: { threshold: { n: 90 } }, atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    seg()[1].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], { custom: { order: [{ start: true }], enabled: [], when: { threshold: { n: 90 } }, atLimit: "stop", wait: 40 } });
  });

  it("切换：缺省选后端给的下一个；热切换 ⇒ rotation-switch hot", async () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    const go = [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("acct.sw.go"))!;
    go.click();
    await flush();
    expect(switchHot).toHaveBeenCalledWith("<local>", ["s1"], "team");
  });

  it("热切换不成立（未实时显示）⇒ 热切换灰、写原因、主按钮变「重启切换」", () => {
    seed({ account: { start: "work", current: "work", since: NOW, history: [], inPlace: "noRelay" } }, undefined);
    openAccountPanel("s1", "<local>", host);
    const radios = [...panel().querySelectorAll<HTMLInputElement>('input[type="radio"][name^="acct-mode"]')];
    expect(radios.map((r) => [r.disabled, r.checked])).toEqual([[true, false], [false, true]]);
    expect(panel().textContent).toContain(copyText("acct.reason.noRelay"));
    expect([...panel().querySelectorAll("button")].some((b) => b.textContent === copyText("acct.sw.restart"))).toBe(true);
  });

  it("跟随默认：列表只读（无把手、勾都灰），上方写「本机默认」那一句", () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    expect([...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].every((b) => b.disabled)).toBe(true);
    expect(panel().textContent).toContain(copyText("rot.src.leadFollow", { name: "日常" }));
  });
});

describe("账号面板 · 来源下拉（19:52 那件：分段一按方向键就写回跟随默认）", () => {
  const src = (): HTMLButtonElement => panel().querySelector<HTMLButtonElement>("[data-acct-src]")!;
  const items = (): HTMLButtonElement[] => [...document.querySelectorAll<HTMLButtonElement>('body > [role="menu"] [role^="menuitem"]')];
  const toasts = (): string[] => [...document.querySelectorAll<HTMLElement>('[role="status"], [role="alert"]')].map((t) => t.textContent ?? "");

  it("★ 面板打开焦点落在来源下拉上；合着时按任何方向键 / Home / End 都不发 rotation-session-set", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    expect(document.activeElement, "焦点在来源下拉").toBe(src());
    for (const key of ["ArrowLeft", "ArrowRight", "ArrowUp", "Home", "End"]) {
      for (const mods of [{}, { altKey: true }, { ctrlKey: true }, { shiftKey: true }]) src().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...mods }));
    }
    await flush();
    expect(writeSessionRotation).not.toHaveBeenCalled();
    expect(src().dataset.value).toBe("custom");
  });

  it("★ 选一项只写一次，toast「orders · 跟随默认」带撤销；撤销 ⇒ 写回原来源", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    src().click();
    const texts = items().map((i) => i.textContent ?? "");
    expect(texts[0].startsWith(copyText("rot.src.follow")), "跟随默认在头").toBe(true);
    expect(texts.slice(-2), "末尾两个动作").toEqual([copyText("rot.src.saveAs"), copyText("rot.src.manage")]);
    items()[0].click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([["<local>", ["s1"], "follow"]]);
    const said = copyText("rot.done.src", { session: "orders", src: copyText("rot.src.follow") });
    expect(toasts().some((t) => t.includes(said))).toBe(true);
    const undo = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("kit.toast.undo"))!;
    undo.click();
    await flush();
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual(["<local>", ["s1"], "custom"]);
    expect(writeSessionRotation).toHaveBeenCalledTimes(2);
  });

  it("选当前那一项 ⇒ 不写、不弹", async () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    src().click();
    items()[0].click();
    await flush();
    expect(writeSessionRotation).not.toHaveBeenCalled();
  });
});

describe("账号面板 · 规则（来源下拉 · 用规则时只读 · 本会话可改 · 存为规则）", () => {
  const src = (): HTMLButtonElement => panel().querySelector<HTMLButtonElement>("[data-acct-src]")!;
  const items = (): HTMLButtonElement[] => [...document.querySelectorAll<HTMLButtonElement>('body > [role="menu"] [role^="menuitem"]')];
  const NIGHT: Rotation = { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40, cap: { team: { "*": [{ at: "17:00-02:00", n: 0 }] } } };
  const withNight = (rotation: Rotation = NIGHT): void => {
    const base = appStore.rotationRules.get().get("<local>")!;
    const night = { ...base.rules[0], id: "r_night", name: "夜间", rotation, isDefault: false, summary: "起始 → team · 满", users: { live: 2, ended: 0, follow: 0, sids: [] } };
    appStore.rotationRules.set(new Map([["<local>", { ...base, rules: [...base.rules, night] }]]));
  };
  const btn = (text: string): HTMLButtonElement => [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text)!;

  it("★ 下拉里每条规则一项（组名「规则」下、名字 ＋ 摘要）；选一条 ⇒ 写 {rule: id} 一次", async () => {
    seed({}, undefined);
    withNight();
    openAccountPanel("s1", "<local>", host);
    src().click();
    expect(document.querySelector('body > [role="menu"]')!.textContent).toContain(copyText("rot.src.rules"));
    const night = items().find((i) => i.textContent?.startsWith("夜间"))!;
    expect(night.textContent).toContain("起始 → team · 满");
    night.click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([["<local>", ["s1"], { rule: "r_night" }]]);
  });

  it("★ 来源是规则：列表只读、封顶写成标签；上方一行规则名 ＋ 在用；［转为本会话］⇒ 写 detach、toast 带撤销", async () => {
    seed({ source: { rule: "r_night" } }, undefined);
    withNight();
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    expect(panel().querySelector("[data-rot-cap-btn]"), "只读时没有封顶按钮").toBeNull();
    expect(panel().querySelector('[data-acct-row="team"]')!.textContent).toContain(copyText("rot.capTag.off", { at: "17:00-02:00" }));
    const lead = panel().querySelector<HTMLElement>("[data-acct-lead]")!.textContent!;
    expect(lead).toContain(copyText("rot.src.leadRule", { name: "夜间" }));
    expect(lead).toContain(copyText("rot.src.inUse", { n: 2 }));
    btn(copyText("rot.src.detach")).click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([["<local>", ["s1"], "detach"]]);
    const said = copyText("rot.done.detach", { session: "orders", name: "夜间" });
    expect([...document.querySelectorAll('[role="status"]')].some((t) => t.textContent?.includes(said))).toBe(true);
    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("kit.toast.undo"))!.click();
    await flush();
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual(["<local>", ["s1"], { rule: "r_night" }]);
  });

  it("★ 本会话：换法点「抢回」⇒ 写 preempt；点「单段预算」⇒ 写 stint {*: {*: 10}}", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    btn(copyText("rot.how.preempt")).click();
    await flush();
    expect((writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }).custom.preempt).toBe(true);
    btn(copyText("rot.how.stint")).click();
    await flush();
    const last = (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }).custom;
    expect([last.stint, last.preempt]).toEqual([{ "*": { "*": 10 } }, false]);
  });

  it("★ 本会话：封顶浮层里改的是草稿（切分段不写）；存 ⇒ 先交后端校验，有错照它标红不写；没错才写那一格", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    panel().querySelector<HTMLButtonElement>('[data-rot-cap-btn="team"]')!.click();
    const cap = (): HTMLElement => document.querySelector<HTMLElement>('[data-rot-cap="team"]')!;
    [...cap().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.cap.slots"))!.click();
    expect(writeSessionRotation, "切分段不写").not.toHaveBeenCalled();
    checkRotation.mockResolvedValueOnce([{ cell: "cap.team.*[0]", code: "same" }]);
    [...cap().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.cap.ok"))!.click();
    await flush();
    expect(cap().textContent).toContain(copyText("rot.capErr.same"));
    expect(writeSessionRotation, "有错不写").not.toHaveBeenCalled();
    checkRotation.mockResolvedValueOnce([]);
    [...cap().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.cap.ok"))!.click();
    await flush();
    expect((writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }).custom.cap).toEqual({ team: { "*": [{ at: "17:00-02:00", n: 0 }] } });
  });

  it("★ 本会话：顺序里具名的号上有「兜底」开关（开 ⇒ 实心标 · 关 ⇒ 平时藏着、悬停 / 行内有焦点才出），名字带号名、aria-pressed 对；点 ⇒ 写 fallback；来源是规则时只画开着的那个", async () => {
    seed({}, { order: [{ start: true }, "team", "personal"], enabled: ["team", "personal"], when: "full", atLimit: "continue", wait: 40, fallback: ["personal"] });
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelector('[data-rot-fallback="api"]'), "序外的号没有开关").toBeNull();
    const on = panel().querySelector<HTMLButtonElement>('[data-rot-fallback="personal"]')!;
    const off = panel().querySelector<HTMLButtonElement>('[data-rot-fallback="team"]')!;
    expect([on.getAttribute("aria-pressed"), on.getAttribute("aria-label"), on.textContent]).toEqual(["true", copyText("rot.fallback.aria", { name: "personal" }), copyText("rot.fallback.tag")]);
    expect([off.getAttribute("aria-pressed"), off.getAttribute("aria-label")]).toEqual(["false", copyText("rot.fallback.aria", { name: "team" })]);
    expect(off.tabIndex, "关着也在 Tab 序里（键盘能到）").toBe(0);
    off.click();
    await flush();
    expect((writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }).custom.fallback).toEqual(["personal", "team"]);
    toggleAccountPanel("s1", "<local>", host);
    seed({ source: { rule: "r_night" } }, undefined);
    withNight({ ...NIGHT, fallback: ["team"] });
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelector("[data-rot-fallback]"), "只读没有开关").toBeNull();
    expect(panel().querySelector('[data-acct-row="team"] [data-rot-fallback-mark]')!.textContent).toBe(copyText("rot.fallback.tag"));
    expect(panel().querySelectorAll("[data-rot-fallback-mark]").length, "只画开着的那个").toBe(1);
  });

  it("★ 存为规则：名称对错照后端（重名 ⇒ 红字、不写会话）；存成且勾着「本会话改用」⇒ 会话改用那一条", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    panel().querySelector<HTMLButtonElement>("[data-acct-save-as]")!.click();
    const pop = (): HTMLElement => document.querySelector<HTMLElement>(`[role="dialog"][aria-label="${copyText("rot.save.title")}"]`)!;
    const input = pop().querySelector<HTMLInputElement>("input[type=text]")!;
    input.value = "夜间";
    saveRule.mockResolvedValueOnce({ state: "refused", errors: [{ cell: "name", code: "dup" }] });
    [...pop().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.save.ok"))!.click();
    await flush();
    expect(pop().textContent).toContain(copyText("rot.save.dup"));
    expect(writeSessionRotation).not.toHaveBeenCalled();
    const rule = { ...appStore.rotationRules.get().get("<local>")!.rules[0], id: "r_new", name: "夜间2", isDefault: false };
    saveRule.mockResolvedValueOnce({ state: "saved", rule });
    input.value = "夜间2";
    [...pop().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.save.ok"))!.click();
    await flush();
    expect(saveRule.mock.calls.at(-1)).toEqual(["<local>", { name: "夜间2", rotation: { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 } }]);
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual(["<local>", ["s1"], { rule: "r_new" }]);
  });
});

describe("账号面板 · 额度格照后端显示态画", () => {
  const refused = (full: boolean) => ({ ...LEDGER.accounts[0], state: "refused" as const, slots: [{ slot: "5h", pct: full ? 100 : 58, resetsAt: NOW + 3600, ...(full ? { full: true } : {}) }] });
  it("★ 被拒而没用满 ⇒「58% · 被拒」（不画 ✕）；用满 ⇒ ✕", () => {
    seed({ quota: refused(false) }, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain(copyText("acct.val.refusedPct", { pct: "58" }));
    expect(panel().textContent).not.toContain("✕");
    toggleAccountPanel("s1", "<local>", host);
    seed({ quota: refused(true) }, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain("✕");
    expect(panel().textContent).not.toContain(copyText("acct.val.refusedPct", { pct: "100" }));
  });
});

describe("账号面板 · 重启切换", () => {
  /** 热切换不成立 ⇒ 主按钮是「重启切换」；点它，等那一趟回来。 */
  async function restart(reply: unknown): Promise<void> {
    switchRestart.mockResolvedValue({ s1: reply });
    seed({ account: { start: "work", current: "work", since: NOW, history: [], inPlace: "noRelay" } }, undefined);
    openAccountPanel("s1", "<local>", host);
    [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("acct.sw.restart"))!.click();
    await flush();
  }
  const toasts = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="alert"], [role="status"]')];
  const toastButtons = (t: HTMLElement): string[] => [...t.querySelectorAll("button")].map((b) => b.textContent ?? "").filter((x) => x !== "");

  it("成了 ⇒ 接回后端说的那个终端、说一句成了", async () => {
    await restart({ state: "done", terminal: "proj-cc-2" });
    expect(switchRestart).toHaveBeenCalledWith("<local>", [expect.objectContaining({ sid: "s1", compact_first: false })], "team", expect.any(Number));
    expect(runRemoteAttach).toHaveBeenCalledWith("<local>", expect.any(String), "proj-cc-2", { quiet: true });
    expect(toasts().map((t) => t.textContent)).toEqual([expect.stringContaining(copyText("acct.sw.doneRestart", { name: "team", session: "orders" }))]);
  });

  it("没成、旧会话还在 ⇒ 「原会话保留 · 原因」，只带［日志］", async () => {
    await restart({ state: "failed", code: "stop_failed", old: "kept" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain(copyText("acct.sw.failRestart", { reason: copyText("acct.reason.stopFailed") }));
    expect(toastButtons(t)).toEqual([copyText("acct.sw.log")]);
    [...t.querySelectorAll("button")].find((b) => b.textContent === copyText("acct.sw.log"))!.click();
    expect(openLog).toHaveBeenCalledTimes(1);
    expect(runRemoteAttach).not.toHaveBeenCalled();
  });

  it("没成、旧会话已停 ⇒ 「原会话已结束 · 原因」带［恢复…］［日志］，点了各做各的", async () => {
    await restart({ state: "failed", code: "start_failed", old: "ended" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain(copyText("acct.sw.failRestartEnded", { ended: copyText("sessionState.ended.name"), reason: copyText("acct.reason.startFailed") }));
    expect(toastButtons(t)).toEqual([copyText("acct.sw.resume"), copyText("acct.sw.log")]);
    [...t.querySelectorAll("button")].find((b) => b.textContent === copyText("acct.sw.resume"))!.click();
    expect(host.openResume).toHaveBeenCalledWith("s1");
    await restart({ state: "failed", code: "notArrived", old: "ended" });
    const last = toasts().at(-1)!;
    expect(last.textContent).toContain(copyText("acct.sw.failRestartEnded", { ended: copyText("sessionState.ended.name"), reason: copyText("acct.reason.notArrived") }));
    [...last.querySelectorAll("button")].find((b) => b.textContent === copyText("acct.sw.log"))!.click();
    expect(openLog).toHaveBeenCalledTimes(1);
  });

  it("那台给的码逐个说成人话，不落「原因不明」", async () => {
    const said: Record<string, string> = {
      account_unavailable: copyText("acct.reason.accountUnavailable", { name: "team" }),
      ambiguous: copyText("acct.reason.ambiguous"),
      not_in_terminal: copyText("acct.reason.notInTerminal"),
      session_already_live: copyText("acct.reason.alreadyLive"),
      shutting_down: copyText("acct.reason.shuttingDown"),
    };
    for (const [code, words] of Object.entries(said)) {
      document.body.replaceChildren();
      if (document.querySelector('[role="dialog"]')) toggleAccountPanel("s1", "<local>", host);
      await restart({ state: "failed", code, old: "kept" });
      expect(toasts().at(-1)!.textContent, code).toContain(copyText("acct.sw.failRestart", { reason: words }));
    }
  });

  describe("从设置窗那一行点进来：滚到那一节，零写入", () => {
    const DEF: Rotation = { order: [{ start: true }, "team", "api"], enabled: ["team", "api"], when: { threshold: { n: 80 } }, atLimit: "continue", wait: 40, cap: { team: { "5h": [{ at: "01:00-20:00", n: 99 }] } } };
    const seedDefault = (): void => {
      appStore.rotationRules.set(new Map([["<local>", rulesOf(DEF)]]));
    };
    const anchored = (a: string): HTMLElement | null => panel().querySelector<HTMLElement>(`[data-acct-anchor="${a}"]`);
    // jsdom 没有布局：滚动的桩就是观测口（「请求滚到谁」）。
    const scrolled = vi.fn();
    beforeEach(() => {
      scrolled.mockReset();
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled(this);
      } as Element["scrollIntoView"];
    });

    it("本会话用自己的轮换：分段照实停在「本会话」，下面摊开只读的默认轮换（顺序 · 触发 · 封顶）", async () => {
      seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue", wait: 40 });
      seedDefault();
      openAccountPanelAt("s1", "<local>", host, "default-rotation");
      await flush();
      const src = panel().querySelector<HTMLButtonElement>("[data-acct-src]");
      expect(src?.dataset.value, "来源照实显示本会话").toBe("custom");
      const box = anchored("default-rotation");
      expect(box, "默认轮换那一块摊开了").not.toBeNull();
      expect(scrolled, "滚到那一块").toHaveBeenLastCalledWith(box);
      expect(box!.textContent).toContain(copyText("acct.prev.lead"));
      expect([...box!.querySelectorAll("[data-acct-row]")].map((r) => r.getAttribute("data-acct-row"))).toEqual(["work", "team", "api"]);
      expect(box!.querySelector('[data-acct-row="team"]')?.textContent).toContain(copyText("rot.capTag.slot", { at: "01:00-20:00", n: "99" }));
      const radios = [...box!.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
      expect(radios.map((r) => [r.checked, r.disabled]), "触发照默认那份（≥N%）、全灰").toEqual([[false, true], [true, true]]);
      expect([...box!.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].every((b) => b.disabled), "勾选全灰").toBe(true);
      const own = [...panel().querySelectorAll<HTMLInputElement>(`input[type="radio"][name="acct-trigger-s1-own"]`)];
      expect(own.map((r) => r.checked), "本会话那组触发没被默认那组顶掉").toEqual([true, false]);
      expect(writeSessionRotation, "经这条打开不发任何写命令").not.toHaveBeenCalled();
    });

    it("本会话跟随默认：直接滚到轮换块，不另摊一块；时间轴那一节展开", async () => {
      seed({}, undefined);
      seedDefault();
      openAccountPanelAt("s1", "<local>", host, "default-rotation");
      await flush();
      expect(panel().querySelectorAll('[data-acct-anchor="default-rotation"]').length).toBe(1);
      expect(anchored("default-rotation")?.tagName, "就是轮换那一块").toBe("SECTION");
      expect(scrolled).toHaveBeenLastCalledWith(anchored("default-rotation"));
      openAccountPanelAt("s1", "<local>", host, "timeline");
      await flush();
      expect(anchored("timeline")?.querySelector("[aria-expanded]")?.getAttribute("aria-expanded"), "时间轴展开").toBe("true");
      expect(scrolled).toHaveBeenLastCalledWith(anchored("timeline"));
      expect(writeSessionRotation).not.toHaveBeenCalled();
      expect(switchHot).not.toHaveBeenCalled();
      expect(switchRestart).not.toHaveBeenCalled();
    });
  });
});
