/**
 * 规则编辑器（设置「轮换」栏里的子页）的判据：怎么进（点行 · ⋯「编辑」· 别处带目的地）与怎么回（面包屑 · Esc）· 每格改完即存一次（带版本）·
 * 撤销上一处 · 冲突条［载入最新］· 触发范围照后端红字 · 「无号可换」按后端 atLimitApplies 灰 · 封顶表一格开那一格的浮层、悬停写此刻取的值 ·
 * 预览照后端 rotation-plan 排（换号点写原因短码、泳道底纹）· 改了视窗重问 · 改名 · 在别处被删了。
 *
 * 读口 / 写口整块换成假的（`vi.mock`）：这里只量「拿到这些事实，画成什么样、交出去什么」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import type {
  PlanRead,
  RuleRow,
  RulesRead,
} from "../../../../src/frontend/ui/quota-reads";
import type { Rotation } from "../../../../src/frontend/ui/generated/Rotation";
import { TOOLTIP_DELAY_MS } from "../../../../src/frontend/ui/kit/tooltip";

const readRules = vi.fn();
const saveRule = vi.fn();
const renameRule = vi.fn();
const readPlan = vi.fn();
const readQuota = vi.fn();
const fetchList = vi.fn();

vi.mock("../../../../src/frontend/ui/kit/toast", () => ({
  toast: vi.fn(),
  failToast: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/quota-reads", async (orig) => ({
  ...(await orig<typeof import("../../../../src/frontend/ui/quota-reads")>()),
  readRules: (...a: unknown[]) => readRules(...a),
  saveRule: (...a: unknown[]) => saveRule(...a),
  renameRule: (...a: unknown[]) => renameRule(...a),
  readPlan: (...a: unknown[]) => readPlan(...a),
  readQuota: (...a: unknown[]) => readQuota(...a),
  checkRotation: vi.fn(async () => []),
  deleteRules: vi.fn(),
  setDefaultRule: vi.fn(),
  writeSessionRotation: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/history-list-reads", () => ({
  fetchList: (...a: unknown[]) => fetchList(...a),
}));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({
  readRemoteConfig: () => Promise.resolve({ hosts: [] }),
}));
vi.mock("../../../../src/frontend/ui/events", () => ({
  bindEvents: () => Promise.resolve(),
}));

import {
  openRuleEditor,
  RulesSection,
} from "../../../../src/frontend/ui/settings/rules-section";
import {
  __resetMachineContextForTests,
  setCurrentMachine,
} from "../../../../src/frontend/ui/settings/machine-context";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { accountColorSlot } from "../../../../src/frontend/ui/account-color";

const ROT: Rotation = {
  order: [{ start: true }, "team", "lab"],
  enabled: ["team", "lab"],
  atLimit: "continue",
  wait: 40,
};

function rule(id: string, name: string, p: Partial<RuleRow> = {}): RuleRow {
  return {
    id,
    name,
    rotation: ROT,
    rev: 3,
    updatedAt: 0,
    isDefault: false,
    users: {
      live: 2,
      ended: 0,
      follow: 0,
      doing: {},
      sids: ["s-a", "s-b"],
      endedSids: [],
    },
    summary: `${name} · team · 满`,
    explain: "起始账号先用 · 被拒才换",
    missing: [],
    atLimitApplies: false,
    ...p,
  };
}

const DAILY = rule("r_daily", "日常", {
  isDefault: true,
  users: { live: 0, ended: 0, follow: 0, doing: {}, sids: [], endedSids: [] },
});
const NIGHT = rule("r_night", "夜间");

function rules(list: RuleRow[] = [DAILY, NIGHT]): RulesRead {
  return { state: "present", reason: null, detail: null, defaultRule: "r_daily", rules: list };
}

const PLAN: PlanRead = {
  errors: [],
  now: 1000,
  nowText: "21:00",
  until: 1000 + 12 * 3600,
  plan: [
    {
      from: 1000,
      fromText: "21:00",
      to: 4600,
      toText: "22:00",
      account: "team",
      why: null,
    },
    {
      from: 4600,
      fromText: "22:00",
      to: 8200,
      toText: "23:00",
      account: "lab",
      why: { off: {} },
    },
    {
      from: 8200,
      fromText: "23:00",
      to: 1000 + 12 * 3600,
      toText: "09:00",
      account: null,
      why: { held: { n: 90, w: "5h" } },
    },
  ],
  lanes: [
    {
      account: "team",
      spans: [
        {
          from: 4600,
          fromText: "22:00",
          to: 8200,
          toText: "23:00",
          state: "off",
          n: null,
        },
      ],
      resets: [{ w: "5h", at: 9000, atText: "23:13" }],
    },
    { account: "lab", spans: [], resets: [] },
  ],
  effective: {
    team: {
      "5h": { v: 99, layer: "all", below: { v: null, layer: "none" } },
      "7d": { v: 99, layer: "all", below: { v: null, layer: "none" } },
      "*": { v: 99, layer: "all", below: { v: null, layer: "none" } },
    },
  },
};

const settle = async (n = 8): Promise<void> => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
/** 预览防抖 300ms：等它过去再收一拍。 */
const settlePlan = async (): Promise<void> => {
  await new Promise((r) => setTimeout(r, 320));
  await settle();
};
async function mount(): Promise<HTMLElement> {
  const s = new RulesSection();
  document.body.replaceChildren(s.element);
  s.loadNow();
  await settle();
  return s.element;
}
const editor = (): HTMLElement | null =>
  document.querySelector<HTMLElement>("[data-rule-editor]");
const menuItem = (label: string): HTMLButtonElement =>
  [
    ...document.querySelectorAll<HTMLButtonElement>(
      '[role="menu"] [role^="menuitem"]',
    ),
  ].find((b) => b.querySelector('[data-part="label"]')?.textContent === label)!;
async function openNight(el: HTMLElement): Promise<HTMLElement> {
  el.querySelector<HTMLElement>('[data-rule="r_night"] [role="cell"]')!.click();
  await settle();
  return editor()!;
}
const saved = (rot: Rotation, p: Partial<RuleRow> = {}) => ({
  state: "saved",
  rule: { ...NIGHT, rotation: rot, rev: NIGHT.rev + 1, ...p },
});

beforeEach(() => {
  for (const f of [
    readRules,
    saveRule,
    renameRule,
    readPlan,
    readQuota,
    fetchList,
  ])
    f.mockReset();
  readRules.mockResolvedValue(rules());
  readPlan.mockResolvedValue(PLAN);
  readQuota.mockResolvedValue({
    state: "present",
    now: 0,
    accounts: [],
    unseen: [],
    usableNow: [],
  });
  fetchList.mockResolvedValue({
    rows: [],
    groups: [],
    total: 0,
    truncated: false,
    notice: null,
  });
  __resetMachineContextForTests();
  setCurrentMachine("devbox");
  document.body.replaceChildren();
});

describe("规则编辑器 · 进与回", () => {
  it("点行 ⇒ 同一栏里的子页（段头收起）：面包屑 `轮换 / 夜间`、后端那句说明、在用 2 会话；面包屑 / Esc ⇒ 回列表", async () => {
    const el = await mount();
    const ed = await openNight(el);
    expect(ed.dataset.ruleEditor).toBe("r_night");
    expect(el.dataset.editing).toBe("true");
    expect(el.querySelector("[data-rule]"), "列表收起").toBeNull();
    expect(ed.querySelector("[data-ed-name]")!.textContent).toBe("夜间");
    expect(ed.querySelector("[data-ed-explain]")!.textContent).toBe(
      "起始账号先用 · 被拒才换",
    );
    expect(ed.querySelector("[data-ed-users]")!.textContent).toBe(
      copyText("rot.src.inUse", { n: 2 }),
    );
    ed.querySelector<HTMLButtonElement>("[data-ed-back]")!.click();
    await settle();
    expect(editor()).toBeNull();
    expect(el.querySelector('[data-rule="r_night"]')).not.toBeNull();
    await openNight(el);
    editor()!.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await settle();
    expect(editor(), "Esc 回列表").toBeNull();
  });

  it("⋯「编辑」⇒ 同一个编辑器；别处带目的地（面板「编辑规则…」）⇒ 这一栏读到那台的表时开那一条", async () => {
    const el = await mount();
    el.querySelector<HTMLButtonElement>('[data-rules-more="r_night"]')!.click();
    await settle();
    menuItem(copyText("rot.act.edit")).click();
    await settle();
    expect(editor()!.dataset.ruleEditor).toBe("r_night");
    editor()!.querySelector<HTMLButtonElement>("[data-ed-back]")!.click();
    openRuleEditor("devbox", "r_daily");
    await settle();
    expect(editor()!.dataset.ruleEditor).toBe("r_daily");
  });
});

describe("规则编辑器 · 每格改完即存", () => {
  it("换法点「抢回」⇒ 存一次（整份 ＋ 读到的版本）；顶上 `已存 HH:MM ·［撤销上一处］`，撤销 ⇒ 把上一份存回去；头一次改动出影响条", async () => {
    const el = await mount();
    const ed = await openNight(el);
    saveRule.mockResolvedValueOnce(saved({ ...ROT, preempt: true }));
    ed.querySelector<HTMLElement>('[data-ed-how="preempt"]')!.click();
    await settle();
    expect(saveRule.mock.calls).toEqual([
      [
        "devbox",
        {
          id: "r_night",
          name: "夜间",
          rotation: { ...ROT, preempt: true },
          ifRev: 3,
        },
      ],
    ]);
    expect(editor()!.querySelector("[data-ed-saved]")!.textContent).toMatch(
      /^已存 \d\d:\d\d/,
    );
    expect(editor()!.querySelector("[data-ed-impact]")!.textContent).toBe(
      copyText("rot.ed.impact", { n: 2 }),
    );
    expect(
      editor()!
        .querySelector('[data-ed-how="preempt"]')!
        .getAttribute("aria-checked"),
    ).toBe("true");
    saveRule.mockResolvedValueOnce(saved(ROT, { rev: 5 }));
    editor()!.querySelector<HTMLButtonElement>("[data-ed-undo]")!.click();
    await settle();
    expect(saveRule.mock.calls[1]).toEqual([
      "devbox",
      { id: "r_night", name: "夜间", rotation: ROT, ifRev: 4 },
    ]);
    expect(
      editor()!.querySelector("[data-ed-undo]"),
      "撤销之后不再有撤销",
    ).toBeNull();
  });

  it("别处先改过了（conflict）⇒ 顶上错误条 `规则已在别处改动 · 未保存 ［载入最新］`，点了重读规则表", async () => {
    const el = await mount();
    const ed = await openNight(el);
    saveRule.mockResolvedValueOnce({ state: "conflict", rev: 9 });
    ed.querySelector<HTMLElement>('[data-ed-how="preempt"]')!.click();
    await settle();
    const bar = editor()!.querySelector<HTMLElement>("[data-ed-conflict]")!;
    expect(bar.textContent).toContain(copyText("rot.fail.conflict"));
    const before = readRules.mock.calls.length;
    [...bar.querySelectorAll("button")]
      .find((b) => b.textContent === copyText("rot.ed.reload"))!
      .click();
    await settle();
    expect(readRules.mock.calls.length).toBe(before + 1);
  });

  it("★ 触发两格：满时两格空着（灰「—」）；在 5h 格填 90 ⇒ 写触发那一行的 5h 格、单选随之落到到线；7d 空 ＝ 7d 满才换（悬停那一句）", async () => {
    const el = await mount();
    const ed = await openNight(el);
    const radio = (k: string) =>
      ed.querySelector<HTMLInputElement>(`[data-rot-trig="${k}"]`)!;
    const cell = (w: string) =>
      editor()!.querySelector<HTMLInputElement>(`[data-rot-line-num="${w}"]`)!;
    expect([radio("full").checked, radio("line").checked]).toEqual([true, false]);
    expect([cell("5h").value, cell("7d").value]).toEqual(["", ""]);
    expect(cell("5h").placeholder).toBe(copyText("rot.list.useNone"));
    expect(cell("7d").getAttribute("aria-label")).toBe(
      copyText("acct.rot.lineAria", { w: "7d" }),
    );
    expect(cell("7d").getAttribute("aria-description")).toBe(
      copyText("acct.rot.lineEmpty", { w: "7d" }),
    );
    const next: Rotation = { ...ROT, cap: { "*": { "5h": 90 } } };
    saveRule.mockResolvedValueOnce(saved(next));
    cell("5h").value = "90";
    cell("5h").dispatchEvent(new Event("change"));
    await settle();
    expect(saveRule.mock.calls[0][1].rotation.cap).toEqual({ "*": { "5h": 90 } });
    expect(
      editor()!.querySelector<HTMLInputElement>('[data-rot-trig="line"]')!.checked,
    ).toBe(true);
    expect(cell("5h").value).toBe("90");
    expect(cell("7d").value, "7d 那一格照旧空着").toBe("");
  });

  it("★ 点「到线」且两格都空 ⇒ 5h 预填 90 写盘；点「满」⇒ 两格都清掉（`cap` 里不留触发那一行，号自己的封顶不动）", async () => {
    const el = await mount();
    let ed = await openNight(el);
    saveRule.mockResolvedValueOnce(saved({ ...ROT, cap: { "*": { "5h": 90 } } }));
    ed.querySelector<HTMLInputElement>('[data-rot-trig="line"]')!.dispatchEvent(
      new Event("change"),
    );
    await settle();
    expect(saveRule.mock.calls[0][1].rotation.cap).toEqual({ "*": { "5h": 90 } });
    ed.querySelector<HTMLElement>("[data-ed-back]")!.click();
    const both: Rotation = {
      ...ROT,
      cap: { "*": { "5h": 90, "7d": 95 }, team: { "7d": 80 } },
    };
    readRules.mockResolvedValue(rules([DAILY, { ...NIGHT, rotation: both }]));
    ed = await openNight(await mount());
    saveRule.mockClear();
    saveRule.mockResolvedValueOnce(saved({ ...ROT, cap: { team: { "7d": 80 } } }));
    ed.querySelector<HTMLInputElement>('[data-rot-trig="full"]')!.dispatchEvent(
      new Event("change"),
    );
    await settle();
    expect(saveRule.mock.calls[0][1].rotation.cap).toEqual({ team: { "7d": 80 } });
  });

  it("★ 填错：写错的数也交后端，它拒（`cap.*.5h` 那一格）⇒ 那一格框红、照原样写着填的字 ＋ 行尾红字 1–99；另一格不红；界面不另判范围", async () => {
    const el = await mount();
    await openNight(el);
    saveRule.mockResolvedValueOnce({
      state: "refused",
      errors: [{ cell: "cap.*.5h", code: "range" }],
    });
    const cell = (w: string) =>
      editor()!.querySelector<HTMLInputElement>(`[data-rot-line-num="${w}"]`)!;
    cell("5h").value = "120";
    cell("5h").dispatchEvent(new Event("change"));
    await settle();
    expect(saveRule.mock.calls[0][1].rotation.cap).toEqual({ "*": { "5h": 120 } });
    expect(cell("5h").dataset.error).toBe("true");
    expect(cell("5h").value).toBe("120");
    expect(cell("7d").dataset.error).toBeUndefined();
    expect(editor()!.querySelector("[data-rot-line-err]")!.textContent).toBe(
      copyText("rot.ed.pctErr"),
    );
  });

  it("★ 盘上 5h 是按时段（命令行 / AI 写的）⇒ 那一格画成按钮「时段 N」，点开即封顶浮层", async () => {
    readRules.mockResolvedValue(
      rules([
        DAILY,
        {
          ...NIGHT,
          rotation: {
            ...ROT,
            cap: { "*": { "5h": [{ at: "17:00-02:00", n: 80 }, { at: "02:00-17:00", n: 95 }] } },
          },
        },
      ]),
    );
    const ed = await openNight(await mount());
    const b = ed.querySelector<HTMLButtonElement>('[data-rot-line-slots="5h"]')!;
    expect(b.textContent).toBe(copyText("rot.cap.slotsN", { n: 2 }));
    expect(ed.querySelector<HTMLInputElement>('[data-rot-trig="line"]')!.checked).toBe(true);
    b.click();
    await settle();
    expect(document.querySelector('[data-rot-cap="*"]')).not.toBeNull();
  });

  it("★ 封顶表没设的格写落下来的值（后端 effective，灰）：来自触发 ⇒「≤90」；全部窗口那一列 · 不封顶 ⇒「—」；设了的格照旧", async () => {
    readPlan.mockResolvedValue({
      ...PLAN,
      effective: {
        team: {
          "5h": { v: 90, layer: "trigger", w: "5h", below: { v: 90, layer: "trigger", w: "5h" } },
          "7d": { v: null, layer: "none", below: { v: null, layer: "none" } },
          "*": { v: null, layer: "none", below: { v: null, layer: "none" }, list: "5h ≤90 · 7d 不封顶", belowList: "5h ≤90 · 7d 不封顶" },
        },
      },
    });
    const el = await mount();
    await openNight(el);
    await settlePlan();
    const cap = (k: string) =>
      editor()!.querySelector<HTMLButtonElement>(`[data-ed-cap="${k}"]`)!;
    expect(cap("team.5h").textContent).toBe(copyText("rot.cap.fixedShort", { n: 90 }));
    expect(cap("team.5h").dataset.fallen).toBe("true");
    expect(cap("team.7d").textContent).toBe(copyText("rot.list.useNone"));
    expect(cap("team.*").textContent).toBe(copyText("rot.list.useNone"));
  });

  it("无号可换：后端说这条规则没有上限（atLimitApplies false）⇒ 两态灰；有上限 ⇒ 可点、点「停」写 atLimit stop", async () => {
    const el = await mount();
    let ed = await openNight(el);
    expect(
      [
        ...ed.querySelectorAll<HTMLButtonElement>("[data-ed-at-limit] button"),
      ].every((b) => b.disabled),
    ).toBe(true);
    ed.querySelector<HTMLButtonElement>("[data-ed-back]")!.click();
    readRules.mockResolvedValue(
      rules([DAILY, { ...NIGHT, atLimitApplies: true }]),
    );
    const el2 = await mount();
    ed = await openNight(el2);
    saveRule.mockResolvedValueOnce(saved({ ...ROT, atLimit: "stop" }));
    [...ed.querySelectorAll<HTMLButtonElement>("[data-ed-at-limit] button")]
      .find((b) => b.textContent === copyText("acct.lim.stop"))!
      .click();
    await settle();
    expect(saveRule.mock.calls[0][1].rotation.atLimit).toBe("stop");
  });

  it("名称点即就地改：Enter ⇒ 改名（带版本）；重名 ⇒ 框红 ＋ 红字、不退出", async () => {
    const el = await mount();
    const ed = await openNight(el);
    ed.querySelector<HTMLButtonElement>("[data-ed-name]")!.click();
    renameRule.mockResolvedValueOnce({
      state: "refused",
      errors: [{ cell: "name", code: "dup" }],
    });
    const inp = editor()!.querySelector<HTMLInputElement>("[data-ed-rename]")!;
    inp.value = "日常";
    inp.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
    await settle();
    expect(renameRule).toHaveBeenCalledWith("devbox", {
      id: "r_night",
      name: "日常",
      ifRev: 3,
    });
    expect(
      editor()!.querySelector<HTMLInputElement>("[data-ed-rename]")!.dataset
        .error,
    ).toBe("true");
    expect(editor()!.textContent).toContain(copyText("rot.save.dup"));
  });

  it("这条规则在别处被删了（重读表里没它）⇒ 整块换成 `规则 夜间 已删除［回到列表］`", async () => {
    const el = await mount();
    const ed = await openNight(el);
    saveRule.mockResolvedValueOnce(saved({ ...ROT, preempt: true }));
    readRules.mockResolvedValue(rules([DAILY]));
    ed.querySelector<HTMLElement>('[data-ed-how="preempt"]')!.click();
    await settle(16);
    expect(editor()!.querySelector("[data-ed-gone]")!.textContent).toContain(
      copyText("rot.ed.gone", { name: "夜间" }),
    );
  });
});

describe("规则编辑器 · 封顶表与预览", () => {
  it("预览「这条规则」不只靠颜色：两号同色（team · lab）时每段段里写号名，读屏名 ＝ 悬停那句 `起止 · 号 · 为什么换`", async () => {
    expect(accountColorSlot("team"), "夹具前提：两号同一个号色").toBe(
      accountColorSlot("lab"),
    );
    const el = await mount();
    await openNight(el);
    await settlePlan();
    const segs = [
      ...editor()!.querySelectorAll<HTMLElement>(
        '[data-tl-row="track"] [data-tl-seg]',
      ),
    ];
    expect(
      segs.map((x) => x.querySelector("[data-tl-seg-name]")?.textContent),
    ).toEqual(["team", "lab", copyText("rot.pv.held")]);
    expect(segs.map((x) => x.getAttribute("aria-label"))).toEqual([
      copyText("rot.pv.seg", { from: "21:00", to: "22:00", acct: "team" }),
      copyText("rot.pv.segWhy", {
        from: "22:00",
        to: "23:00",
        acct: "lab",
        why: copyText("rot.pv.whyOff", { acct: "team" }),
      }),
      copyText("rot.pv.segWhy", {
        from: "23:00",
        to: "09:00",
        acct: copyText("rot.pv.held"),
        why: copyText("acct.hist.held", { name: "lab", w: "5h", n: 90 }),
      }),
    ]);
  });

  it("封顶表：行 ＝ 勾上的号，列 5h · 7d · 全部窗口；点一格开那一格的浮层", async () => {
    const el = await mount();
    await openNight(el);
    await settlePlan();
    const rows = [
      ...editor()!.querySelectorAll<HTMLElement>("[data-ed-cap-row]"),
    ].map((r) => r.dataset.edCapRow);
    expect(rows).toEqual(["team", "lab"]);
    const cell = editor()!.querySelector<HTMLButtonElement>(
      '[data-ed-cap="team.5h"]',
    )!;
    cell.click();
    await settle();
    expect(
      document.querySelector('[data-rot-cap="team"]'),
      "开那一格的封顶浮层",
    ).not.toBeNull();
  });

  it("封顶格悬停 ＝ 此刻实际取的值与来自哪一层（后端 effective）：`此刻 ≤99 · 来自 全部窗口`；后端没给那一格 ⇒ 不出", async () => {
    const el = await mount();
    await openNight(el);
    await settlePlan();
    // 悬停延时走假计时器推过去（原先真睡 560 ＋ 600 ms、两格共 2.3 s，负载高时整格撞 5 s 默认期限）。
    // 只假 setTimeout：提示的「同组」宽限读 `Date.now()`，假了它会把收起时刻记到真钟的未来、串到后面的用例。
    const tipOf = (sel: string): string | null => {
      const b = editor()!.querySelector<HTMLElement>(sel)!;
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      try {
        b.dispatchEvent(new Event("mouseenter"));
        vi.advanceTimersByTime(TOOLTIP_DELAY_MS + 60);
        const t = document.querySelector('[role="tooltip"]')?.textContent ?? null;
        b.dispatchEvent(new Event("mouseleave"));
        vi.advanceTimersByTime(600);
        return t;
      } finally {
        vi.useRealTimers();
      }
    };
    expect(tipOf('[data-ed-cap="team.5h"]')).toBe(
      copyText("rot.capT.effective", {
        v: copyText("rot.cap.fixedShort", { n: 99 }),
        layer: copyText("rot.capT.layerAll"),
      }),
    );
    expect(tipOf('[data-ed-cap="lab.5h"]')).toBeNull();
  });

  it("预览：问 `{rule, span: 12h}`；「这条规则」按段排、换号点写原因短码（时段停用 · 停发）；泳道里不能用的段带它的样子；换视窗 ⇒ 重问", async () => {
    const el = await mount();
    await openNight(el);
    await settlePlan();
    expect(readPlan).toHaveBeenLastCalledWith("devbox", {
      rule: "r_night",
      span: "12h",
    });
    const pv = editor()!.querySelector<HTMLElement>(
      '[data-ed-preview="r_night"]',
    )!;
    const segs = [
      ...pv.querySelectorAll<HTMLElement>(
        '[data-tl-row="track"] [data-tl-seg]',
      ),
    ];
    expect(segs.map((x) => x.dataset.tlSeg)).toEqual(["team", "lab", ""]);
    // 换号点在段上面那一层（不与段里的号名叠）：头一段没有，其余各一个、摆在那一段的起点。
    const whys = [...pv.querySelectorAll<HTMLElement>("[data-tl-why]")];
    expect(whys.map((x) => x.textContent)).toEqual([
      copyText("rot.why.off"),
      copyText("rot.why.held"),
    ]);
    expect(whys.map((x) => x.style.left)).toEqual([
      segs[1].style.left,
      segs[2].style.left,
    ]);
    expect(segs[2].dataset.held).toBe("true");
    expect(segs[1].style.left).toBe(
      `${((3600 / (12 * 3600)) * 100).toFixed(3)}%`,
    );
    const off = pv.querySelector<HTMLElement>(
      '[data-tl-row="team"] [data-state]',
    )!;
    expect(off.dataset.state).toBe("off");
    pv.querySelector<HTMLButtonElement>("[data-ed-span]")!.click();
    menuItem(copyText("rot.pv.span", { span: "6h" })).click();
    await settlePlan();
    expect(readPlan).toHaveBeenLastCalledWith("devbox", {
      rule: "r_night",
      span: "6h",
    });
  });
});
