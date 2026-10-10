/**
 * 「账号」面板的行为：勾号 · 无号可换两态 · 热切换 · 热切换不成立 · 跟随默认时只读。写都经 `quota-reads.ts`（这里桩住），
 * 数据只从 `appStore` 来（那几格由推送填）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const writeSessionRotation = vi.fn();
const checkRotation = vi.fn();
const readPlan = vi.fn();
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
  readPlan: (...a: unknown[]) => readPlan(...a),
  saveRule: (...a: unknown[]) => saveRule(...a),
  readSessionRotation: vi.fn(() => new Promise(() => {})),
}));

vi.mock("../../../src/frontend/ui/sessions-where", () => ({
  standingOf: vi.fn(async () => ({
    kind: "running",
    names: ["proj-cc"],
    terminals: ["t1"],
  })),
}));
vi.mock("../../../src/frontend/ui/interrupt-reads", () => ({
  askSessionInterrupts: vi.fn(async () => ({ families: [] })),
}));
vi.mock("../../../src/frontend/ui/tab-batch-run", () => ({
  startSettings: vi.fn(async () => ({})),
}));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteAttach: (...a: unknown[]) => runRemoteAttach(...a),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: { open_log_file: () => openLog() },
}));

import {
  openAccountPanel,
  toggleAccountPanel,
  type AcctPanelHost,
} from "../../../src/frontend/ui/acct-panel.ts";
import { appStore } from "../../../src/frontend/ui/app-store.ts";
import type { QuotaRead } from "../../../src/frontend/ui/acct-words.ts";
import type { Rotation } from "../../../src/frontend/ui/generated/Rotation.ts";
import type { SessionRotation } from "../../../src/frontend/ui/generated/SessionRotation.ts";
import type {
  RuleRow,
  RulesRead,
} from "../../../src/frontend/ui/quota-reads.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";

const NOW = 1_791_189_600;
const host: AcctPanelHost = {
  sessionTitle: () => "orders",
  cwdOf: () => "/w",
  agentOf: () => "claude",
  openSettings: vi.fn(),
  openRules: vi.fn(),
  openDefaultMenu: vi.fn(),
  defaultOf: () => "work",
  openResume: vi.fn(),
  openSession: vi.fn(),
  canOpenSession: (sid) => sid === "p1",
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
    slots:
      account === "api" ? [] : [{ slot: "5h", pct: 30, resetsAt: NOW + 3600, text: "30%", tone: "plain" }],
    login: "ok" as const,
    usage: account === "api" ? { value: copyText("acct.kind.api"), text: copyText("acct.kind.api"), tone: "plain" as const } : { slot: "5h", window: "5h", value: "30%", text: "5h 30%", tone: "plain" as const },
    rows: [],
    warm: { act: "send" as const, text: "" },
    fiveHour: account === "api" ? null : "5h 30%",
  })),
  unseen: [],
  usableNow: ["work", "team", "api"],
  earliestReturn: null,
  fiveHour: null,
};

/** 一张只有默认那一条的规则表（名字 `默认`）。 */
function rulesOf(rotation: Rotation, more: RuleRow[] = []): RulesRead {
  const row = (
    id: string,
    name: string,
    r: Rotation,
    isDefault: boolean,
  ): RuleRow => ({
    id,
    name,
    rotation: r,
    rev: 1,
    updatedAt: NOW,
    isDefault,
    users: {
      live: 1,
      ended: 0,
      follow: 1,
      doing: { s1: { state: "working", needs: null, text: "核心·运行中", tone: "now" } },
      sids: ["s1"],
      endedSids: [],
    },
    summary: "",
    explain: "",
    missing: [],
    atLimitApplies: false,
  });
  return {
    state: "present",
    reason: null,
    detail: null,
    defaultRule: "r_def",
    followText: "核心·跟随默认",
    rules: [row("r_def", "日常", rotation, true), ...more],
  };
}

function seed(p: Partial<SessionRotation>, custom: Rotation | undefined): void {
  appStore.quota.set(new Map([["<local>", LEDGER]]));
  appStore.rotationRules.set(
    new Map([
      [
        "<local>",
        rulesOf({
          order: [{ start: true }],
          enabled: [],
          atLimit: "continue",
          wait: 40,
        }),
      ],
    ]),
  );
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
            account: {
              start: "work",
              current: "work",
              since: NOW - 600,
              history: [],
              inPlace: "ok",
            },
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

const panel = (): HTMLElement =>
  document.querySelector<HTMLElement>('[role="dialog"]')!;
const flush = async (): Promise<void> => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  writeSessionRotation.mockReset().mockResolvedValue({ s1: { state: "done" } });
  checkRotation.mockReset().mockResolvedValue([]);
  readPlan.mockReset().mockReturnValue(new Promise(() => {}));
  saveRule.mockReset();
  switchHot.mockReset().mockResolvedValue({ s1: { state: "done" } });
  switchRestart.mockReset();
  runRemoteAttach.mockReset().mockResolvedValue(undefined);
  openLog.mockReset().mockResolvedValue(undefined);
  vi.mocked(host.openResume).mockReset();
  if (document.querySelector('[role="dialog"]'))
    toggleAccountPanel("s1", "<local>", host);
  document.body.replaceChildren();
});

describe("账号面板", () => {
  it("本会话：勾上一个号 ⇒ 整份写回（顺序里没有的排末尾，挪按量号到末尾是后端的事）", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    const boxes = [
      ...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ];
    expect(boxes.map((b) => b.getAttribute("aria-label"))).toEqual([
      copyText("acct.row.checkAria", { name: "work" }),
      copyText("acct.row.checkAria", { name: "team" }),
      copyText("acct.row.checkAria", { name: "api" }),
    ]);
    expect(boxes[0].disabled, "起始 · 在用那一行锁着").toBe(true);
    boxes[2].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], {
      custom: {
        order: [{ start: true }, "team", "api"],
        enabled: ["team", "api"],
        atLimit: "continue",
        wait: 40,
      },
    });
  });

  describe("勾 / 不勾只改 enabled，order 逐字不变（不凭空插「起始账号」那一格）", () => {
    const boxOf = (name: string): HTMLInputElement =>
      panel().querySelector<HTMLInputElement>(
        `input[aria-label="${copyText("acct.row.checkAria", { name })}"]`,
      )!;
    const lastWrite = (): Rotation =>
      (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation })
        .custom;

    it("没有占位的那份：勾上序外的号 ⇒ 只排到末尾、不插占位", async () => {
      seed(
        {},
        {
          order: ["team", "work"],
          enabled: ["team", "work"],
          atLimit: "continue",
          wait: 40,
        },
      );
      openAccountPanel("s1", "<local>", host);
      boxOf("api").click();
      await flush();
      expect(lastWrite().order).toEqual(["team", "work", "api"]);
      expect(lastWrite().enabled).toEqual(["team", "work", "api"]);
    });

    it("没有占位的那份：取消勾 ⇒ order 原样", async () => {
      seed(
        {},
        {
          order: ["team", "work"],
          enabled: ["team", "work"],
          atLimit: "continue",
          wait: 40,
        },
      );
      openAccountPanel("s1", "<local>", host);
      boxOf("team").click();
      await flush();
      expect(lastWrite().order).toEqual(["team", "work"]);
      expect(lastWrite().enabled).toEqual(["work"]);
    });

    it("占位 ＋ 起始号也具名在序里：取消勾别的号 ⇒ 具名那格不丢", async () => {
      seed(
        {},
        {
          order: [{ start: true }, "team", "work"],
          enabled: ["team", "work"],
          atLimit: "continue",
          wait: 40,
        },
      );
      openAccountPanel("s1", "<local>", host);
      boxOf("team").click();
      await flush();
      expect(lastWrite().order).toEqual([{ start: true }, "team", "work"]);
      expect(lastWrite().enabled).toEqual(["work"]);
    });

    it("没有占位的那份：Alt+↓ 移位 ⇒ 只换这两格，不插占位", async () => {
      seed(
        {},
        {
          order: ["team", "work"],
          enabled: ["team", "work"],
          atLimit: "continue",
          wait: 40,
        },
      );
      openAccountPanel("s1", "<local>", host);
      panel()
        .querySelector<HTMLElement>('[data-acct-row="team"]')!
        .dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "ArrowDown",
            altKey: true,
            bubbles: true,
          }),
        );
      await flush();
      expect(lastWrite().order).toEqual(["work", "team"]);
    });
  });

  it("无号可换：「满」触发时两态灰着；≥N% 时点「停」⇒ 写 atLimit stop", async () => {
    seed(
      {},
      {
        order: [{ start: true }],
        enabled: [],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    const seg = (): HTMLButtonElement[] => [
      ...panel().querySelectorAll<HTMLButtonElement>(
        `[aria-label="${copyText("acct.lim.aria")}"] button`,
      ),
    ];
    expect(seg().map((b) => [b.textContent, b.disabled])).toEqual([
      [copyText("acct.lim.go"), true],
      [copyText("acct.lim.stop"), true],
    ]);
    toggleAccountPanel("s1", "<local>", host);
    seed(
      {},
      {
        order: [{ start: true }],
        enabled: [],
        cap: { "*": { "5h": 90 } },
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    seg()[1].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], {
      custom: {
        order: [{ start: true }],
        enabled: [],
        cap: { "*": { "5h": 90 } },
        atLimit: "stop",
        wait: 40,
      },
    });
  });

  it("额度账读不出 ⇒「当前」那一块一条警告条：后端写好的那一句（读的哪份 · 原因词）＋［复制详情］", () => {
    seed({}, undefined);
    const why = "读取 /h/.cc-monitor/quota.json 失败 · 内容无法解析";
    appStore.quota.set(
      new Map([["<local>", { ...LEDGER, state: "unreadable", reason: why, detail: "时刻 …\n原话 zz", text: why, accounts: [] }]]),
    );
    openAccountPanel("s1", "<local>", host);
    const bar = [...panel().querySelectorAll<HTMLElement>("[role=status]")].find((b) => b.textContent?.includes(why));
    expect(bar, "读不出那一条没说原因").toBeTruthy();
    expect(bar!.querySelector('[data-part="copy-detail"]'), "没有［复制详情］").not.toBeNull();
    toggleAccountPanel("s1", "<local>", host);
  });

  it("切换：缺省选后端给的下一个；热切换 ⇒ rotation-switch hot", async () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    const go = [...panel().querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === copyText("acct.sw.go"),
    )!;
    go.click();
    await flush();
    expect(switchHot).toHaveBeenCalledWith("<local>", ["s1"], "team");
  });

  it("热切换不成立（未实时显示）⇒ 热切换灰、写原因、主按钮变「重启切换」", () => {
    seed(
      {
        account: {
          start: "work",
          current: "work",
          since: NOW,
          history: [],
          inPlace: "noRelay",
        },
      },
      undefined,
    );
    openAccountPanel("s1", "<local>", host);
    const radios = [
      ...panel().querySelectorAll<HTMLInputElement>(
        'input[type="radio"][name^="acct-mode"]',
      ),
    ];
    expect(radios.map((r) => [r.disabled, r.checked])).toEqual([
      [true, false],
      [false, true],
    ]);
    expect(panel().textContent).toContain(copyText("acct.reason.noRelay"));
    expect(
      [...panel().querySelectorAll("button")].some(
        (b) => b.textContent === copyText("acct.sw.restart"),
      ),
    ).toBe(true);
  });

  it("跟随默认：列表只读（无把手、勾都灰），上方写「本机默认」那一句", () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    expect(
      [
        ...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
      ].every((b) => b.disabled),
    ).toBe(true);
    expect(panel().textContent).toContain(
      copyText("rot.src.leadFollow", { name: "日常" }),
    );
  });
});

describe("账号面板 · 来源下拉（19:52 那件：分段一按方向键就写回跟随默认）", () => {
  const src = (): HTMLButtonElement =>
    panel().querySelector<HTMLButtonElement>("[data-acct-src]")!;
  const items = (): HTMLButtonElement[] => [
    ...document.querySelectorAll<HTMLButtonElement>(
      'body > [role="menu"] [role^="menuitem"]',
    ),
  ];
  const toasts = (): string[] =>
    [
      ...document.querySelectorAll<HTMLElement>(
        '[role="status"], [role="alert"]',
      ),
    ].map((t) => t.textContent ?? "");

  it("★ 面板打开焦点落在来源下拉上；合着时按任何方向键 / Home / End 都不发 rotation-session-set", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    expect(document.activeElement, "焦点在来源下拉").toBe(src());
    for (const key of ["ArrowLeft", "ArrowRight", "ArrowUp", "Home", "End"]) {
      for (const mods of [
        {},
        { altKey: true },
        { ctrlKey: true },
        { shiftKey: true },
      ])
        src().dispatchEvent(
          new KeyboardEvent("keydown", { key, bubbles: true, ...mods }),
        );
    }
    await flush();
    expect(writeSessionRotation).not.toHaveBeenCalled();
    expect(src().dataset.value).toBe("custom");
  });

  it("★ 选一项只写一次，toast「orders · 跟随默认」带撤销；撤销 ⇒ 写回原来源", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    src().click();
    const texts = items().map((i) => i.textContent ?? "");
    expect(
      texts[0].startsWith(copyText("rot.src.follow")),
      "跟随默认在头",
    ).toBe(true);
    expect(texts.slice(-2), "末尾两个动作").toEqual([
      copyText("rot.src.saveAs"),
      copyText("rot.src.manage"),
    ]);
    items()[0].click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([
      ["<local>", ["s1"], "follow"],
    ]);
    const said = copyText("rot.done.src", {
      session: "orders",
      src: copyText("rot.src.follow"),
    });
    expect(toasts().some((t) => t.includes(said))).toBe(true);
    const undo = [
      ...document.querySelectorAll<HTMLButtonElement>("button"),
    ].find((b) => b.textContent === copyText("kit.toast.undo"))!;
    undo.click();
    await flush();
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual([
      "<local>",
      ["s1"],
      "custom",
    ]);
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
  const src = (): HTMLButtonElement =>
    panel().querySelector<HTMLButtonElement>("[data-acct-src]")!;
  const items = (): HTMLButtonElement[] => [
    ...document.querySelectorAll<HTMLButtonElement>(
      'body > [role="menu"] [role^="menuitem"]',
    ),
  ];
  const NIGHT: Rotation = {
    order: [{ start: true }, "team"],
    enabled: ["team"],
    atLimit: "continue",
    wait: 40,
    cap: { team: { "*": [{ at: "17:00-02:00", n: 0 }] } },
  };
  const withNight = (rotation: Rotation = NIGHT): void => {
    const base = appStore.rotationRules.get().get("<local>")!;
    const night = {
      ...base.rules[0],
      id: "r_night",
      name: "夜间",
      rotation,
      isDefault: false,
      summary: "起始 → team · 满",
      users: {
        live: 2,
        ended: 0,
        follow: 0,
        doing: {},
        sids: [],
        endedSids: [],
      },
    };
    appStore.rotationRules.set(
      new Map([["<local>", { ...base, rules: [...base.rules, night] }]]),
    );
  };
  const btn = (text: string): HTMLButtonElement =>
    [...panel().querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === text,
    )!;

  it("★ 下拉里每条规则一项（组名「规则」下、名字 ＋ 摘要）；选一条 ⇒ 写 {rule: id} 一次", async () => {
    seed({}, undefined);
    withNight();
    openAccountPanel("s1", "<local>", host);
    src().click();
    expect(
      document.querySelector('body > [role="menu"]')!.textContent,
    ).toContain(copyText("rot.src.rules"));
    const night = items().find((i) => i.textContent?.startsWith("夜间"))!;
    expect(night.textContent).toContain("起始 → team · 满");
    night.click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([
      ["<local>", ["s1"], { rule: "r_night" }],
    ]);
  });

  it("★ 下拉里规则那一项悬停 300ms ⇒ 右侧只读小卡：顺序（勾上的号 · 封顶标签 · 兜底）＋ 后端那句说明；看完不用切、不写", async () => {
    seed({}, undefined);
    withNight({ ...NIGHT, fallback: ["team"] });
    const night = appStore.rotationRules
      .get()
      .get("<local>")!
      .rules.find((r) => r.id === "r_night")!;
    night.explain = "起始账号先用 · 被拒才换";
    openAccountPanel("s1", "<local>", host);
    src().click();
    const item = items().find((i) => i.textContent?.startsWith("夜间"))!;
    vi.useFakeTimers();
    item.dispatchEvent(new MouseEvent("mouseenter"));
    vi.advanceTimersByTime(300);
    vi.useRealTimers();
    const card = document.querySelector<HTMLElement>("[data-menu-peek]")!;
    expect(card, "悬停出小卡").not.toBeNull();
    expect(
      [...card.querySelectorAll<HTMLElement>("[data-peek-row]")].map(
        (r) => r.dataset.peekRow,
      ),
    ).toEqual(["start", "team"]);
    expect(card.querySelector('[data-peek-row="team"]')!.textContent).toContain(
      copyText("rot.capTag.off", { at: "17:00-02:00" }),
    );
    expect(
      card.querySelector('[data-peek-row="team"] [data-rot-fallback-mark]'),
      "兜底照只读那一形",
    ).not.toBeNull();
    expect(card.textContent).toContain("起始账号先用 · 被拒才换");
    expect(writeSessionRotation).not.toHaveBeenCalled();
  });

  it("规则多过 10 条 ⇒ 下拉顶上出筛选框（自动聚焦）；10 条以内不出", async () => {
    seed({}, undefined);
    const base = appStore.rotationRules.get().get("<local>")!;
    const many = (n: number) =>
      Array.from({ length: n }, (_, i) => ({
        ...base.rules[0],
        id: `r_${i}`,
        name: `规则${i}`,
        isDefault: i === 0,
      }));
    appStore.rotationRules.set(
      new Map([["<local>", { ...base, rules: many(10) }]]),
    );
    openAccountPanel("s1", "<local>", host);
    src().click();
    expect(document.querySelector("input[data-menu-filter]")).toBeNull();
    src().click();
    appStore.rotationRules.set(
      new Map([["<local>", { ...base, rules: many(11) }]]),
    );
    openAccountPanel("s1", "<local>", host);
    src().click();
    const box = document.querySelector<HTMLInputElement>(
      "input[data-menu-filter]",
    )!;
    expect(box.getAttribute("aria-label")).toBe(copyText("rot.src.filter"));
    expect(document.activeElement).toBe(box);
  });

  it("［编辑规则…］⇒ 设置窗那台的「轮换」栏、带这条规则；下拉「管理规则…」⇒ 同一栏、不带规则（不再开账号栏）", async () => {
    seed({ source: { rule: "r_night" } }, undefined);
    withNight();
    openAccountPanel("s1", "<local>", host);
    btn(copyText("rot.src.edit")).click();
    expect(host.openRules).toHaveBeenLastCalledWith("<local>", "r_night");
    src().click();
    items()
      .find((i) => i.textContent === copyText("rot.src.manage"))!
      .click();
    expect(host.openRules).toHaveBeenLastCalledWith("<local>");
    expect(host.openSettings).not.toHaveBeenCalled();
  });

  it("★ 来源是规则：列表只读、封顶写成标签；上方一行规则名 ＋ 在用；［转为本会话］⇒ 写 detach、toast 带撤销", async () => {
    seed({ source: { rule: "r_night" } }, undefined);
    withNight();
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    expect(
      panel().querySelector("[data-rot-cap-btn]"),
      "只读时没有封顶按钮",
    ).toBeNull();
    expect(
      panel().querySelector('[data-acct-row="team"]')!.textContent,
    ).toContain(copyText("rot.capTag.off", { at: "17:00-02:00" }));
    const lead =
      panel().querySelector<HTMLElement>("[data-acct-lead]")!.textContent!;
    expect(lead).toContain(copyText("rot.src.leadRule", { name: "夜间" }));
    expect(lead).toContain(copyText("rot.src.inUse", { n: 2 }));
    btn(copyText("rot.src.detach")).click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([
      ["<local>", ["s1"], "detach"],
    ]);
    const said = copyText("rot.done.detach", {
      session: "orders",
      name: "夜间",
    });
    expect(
      [...document.querySelectorAll('[role="status"]')].some((t) =>
        t.textContent?.includes(said),
      ),
    ).toBe(true);
    [...document.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("kit.toast.undo"))!
      .click();
    await flush();
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual([
      "<local>",
      ["s1"],
      { rule: "r_night" },
    ]);
  });

  it("★ 本会话：换法点「抢回」⇒ 写 preempt；点「单段预算」⇒ 写 stint {*: {*: 10}}", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    btn(copyText("rot.how.preempt")).click();
    await flush();
    expect(
      (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation })
        .custom.preempt,
    ).toBe(true);
    btn(copyText("rot.how.stint")).click();
    await flush();
    const last = (
      writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation }
    ).custom;
    expect([last.stint, last.preempt]).toEqual([{ "*": { "*": 10 } }, false]);
  });

  it("★ 本会话：封顶浮层里改的是草稿（切分段不写）；存 ⇒ 先交后端校验，有错照它标红不写；没错才写那一格", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    panel()
      .querySelector<HTMLButtonElement>('[data-rot-cap-btn="team"]')!
      .click();
    const cap = (): HTMLElement =>
      document.querySelector<HTMLElement>('[data-rot-cap="team"]')!;
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.slots"))!
      .click();
    expect(writeSessionRotation, "切分段不写").not.toHaveBeenCalled();
    checkRotation.mockResolvedValueOnce([
      { cell: "cap.team.*[0]", code: "same", said: "那台写的起止那一句" },
    ]);
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.ok"))!
      .click();
    await flush();
    expect(cap().textContent, "照抄核心写好的 `said`").toContain("那台写的起止那一句");
    expect(writeSessionRotation, "有错不写").not.toHaveBeenCalled();
    checkRotation.mockResolvedValueOnce([]);
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.ok"))!
      .click();
    await flush();
    expect(
      (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation })
        .custom.cap,
    ).toEqual({ team: { "*": [{ at: "17:00-02:00", n: 0 }] } });
  });

  it("★ 封顶浮层「按时段」：色带下一行「其余时段 ＝ …」照后端 effective 那一格的下一层写（全部窗口 ≤N · 触发 ≥N% · 不封顶）；问的是这一份草稿", async () => {
    const r: Rotation = {
      order: [{ start: true }, "team"],
      enabled: ["team"],
      cap: { "*": { "5h": 90 } },
      atLimit: "continue",
      wait: 40,
    };
    seed({}, r);
    const below = (b: { v: number | null; layer: string; w?: string }) => ({
      errors: [],
      now: 0,
      nowText: "",
      until: 0,
      plan: [],
      lanes: [],
      effective: { team: { "*": { v: 90, layer: "trigger", below: b } } },
    });
    readPlan.mockResolvedValue(below({ v: 90, layer: "trigger", w: "5h" }));
    openAccountPanel("s1", "<local>", host);
    panel()
      .querySelector<HTMLButtonElement>('[data-rot-cap-btn="team"]')!
      .click();
    const cap = (): HTMLElement =>
      document.querySelector<HTMLElement>('[data-rot-cap="team"]')!;
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.slots"))!
      .click();
    await flush();
    expect(readPlan).toHaveBeenCalledWith("<local>", { rotation: r });
    expect(cap().querySelector("[data-rot-rest]")!.textContent).toBe(
      copyText("rot.cap.rest", {
        what: copyText("rot.cap.restTrig", { w: "5h", n: 90 }),
      }),
    );
    readPlan.mockResolvedValue(below({ v: null, layer: "none" }));
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.fixed"))!
      .click();
    expect(
      cap().querySelector("[data-rot-rest]"),
      "固定那一形没有「其余时段」",
    ).toBeNull();
    [...cap().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.cap.slots"))!
      .click();
    await flush();
    expect(
      cap().querySelector("[data-rot-rest]")!.textContent,
      "开浮层时问一次（下一层不归这一格管，切分段不重问）",
    ).toBe(
      copyText("rot.cap.rest", {
        what: copyText("rot.cap.restTrig", { w: "5h", n: 90 }),
      }),
    );
  });

  it("★ 本会话：顺序里具名的号上有「兜底」开关（开 ⇒ 实心标 · 关 ⇒ 平时藏着、悬停 / 行内有焦点才出），名字带号名、aria-pressed 对；点 ⇒ 写 fallback；来源是规则时只画开着的那个", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team", "personal"],
        enabled: ["team", "personal"],
        atLimit: "continue",
        wait: 40,
        fallback: ["personal"],
      },
    );
    openAccountPanel("s1", "<local>", host);
    expect(
      panel().querySelector('[data-rot-fallback="api"]'),
      "序外的号没有开关",
    ).toBeNull();
    const on = panel().querySelector<HTMLButtonElement>(
      '[data-rot-fallback="personal"]',
    )!;
    const off = panel().querySelector<HTMLButtonElement>(
      '[data-rot-fallback="team"]',
    )!;
    expect([
      on.getAttribute("aria-pressed"),
      on.getAttribute("aria-label"),
      on.textContent,
    ]).toEqual([
      "true",
      copyText("rot.fallback.aria", { name: "personal" }),
      copyText("rot.fallback.tag"),
    ]);
    expect([
      off.getAttribute("aria-pressed"),
      off.getAttribute("aria-label"),
    ]).toEqual(["false", copyText("rot.fallback.aria", { name: "team" })]);
    expect(off.tabIndex, "关着也在 Tab 序里（键盘能到）").toBe(0);
    off.click();
    await flush();
    expect(
      (writeSessionRotation.mock.calls.at(-1)![2] as { custom: Rotation })
        .custom.fallback,
    ).toEqual(["personal", "team"]);
    toggleAccountPanel("s1", "<local>", host);
    seed({ source: { rule: "r_night" } }, undefined);
    withNight({ ...NIGHT, fallback: ["team"] });
    openAccountPanel("s1", "<local>", host);
    expect(
      panel().querySelector("[data-rot-fallback]"),
      "只读没有开关",
    ).toBeNull();
    expect(
      panel().querySelector('[data-acct-row="team"] [data-rot-fallback-mark]')!
        .textContent,
    ).toBe(copyText("rot.fallback.tag"));
    expect(
      panel().querySelectorAll("[data-rot-fallback-mark]").length,
      "只画开着的那个",
    ).toBe(1);
  });

  it("★ 存为规则：名称对错照后端（重名 ⇒ 红字、不写会话）；存成且勾着「本会话改用」⇒ 会话改用那一条", async () => {
    seed(
      {},
      {
        order: [{ start: true }, "team"],
        enabled: ["team"],
        atLimit: "continue",
        wait: 40,
      },
    );
    openAccountPanel("s1", "<local>", host);
    panel().querySelector<HTMLButtonElement>("[data-acct-save-as]")!.click();
    const pop = (): HTMLElement =>
      document.querySelector<HTMLElement>(
        `[role="dialog"][aria-label="${copyText("rot.save.title")}"]`,
      )!;
    const input = pop().querySelector<HTMLInputElement>("input[type=text]")!;
    input.value = "夜间";
    saveRule.mockResolvedValueOnce({
      state: "refused",
      errors: [{ cell: "name", code: "dup", said: copyText("rot.save.dup") }],
    });
    [...pop().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.save.ok"))!
      .click();
    await flush();
    expect(pop().textContent).toContain(copyText("rot.save.dup"));
    expect(writeSessionRotation).not.toHaveBeenCalled();
    const rule = {
      ...appStore.rotationRules.get().get("<local>")!.rules[0],
      id: "r_new",
      name: "夜间2",
      isDefault: false,
    };
    saveRule.mockResolvedValueOnce({ state: "saved", rule, savedAtText: "04:30" });
    input.value = "夜间2";
    [...pop().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("rot.save.ok"))!
      .click();
    await flush();
    expect(saveRule.mock.calls.at(-1)).toEqual([
      "<local>",
      {
        name: "夜间2",
        rotation: {
          order: [{ start: true }, "team"],
          enabled: ["team"],
          atLimit: "continue",
          wait: 40,
        },
      },
    ]);
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual([
      "<local>",
      ["s1"],
      { rule: "r_new" },
    ]);
  });
});

describe("账号面板 · 额度格照抄核心写好的字", () => {
  const refused = (full: boolean) => ({
    ...LEDGER.accounts[0],
    state: "refused" as const,
    slots: [
      {
        slot: "5h",
        pct: full ? 100 : 58,
        resetsAt: NOW + 3600,
        // 字由核心写好（`show.rs::slot_words`），界面照抄。
        text: full ? "✕" : copyText("acct.val.refusedPct", { pct: "58" }),
        tone: "fail" as const,
        ...(full ? { full: true } : {}),
      },
    ],
  });
  it("★ 被拒而没用满 ⇒「58% · 被拒」（不画 ✕）；用满 ⇒ ✕", () => {
    seed({ quota: refused(false) }, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain(
      copyText("acct.val.refusedPct", { pct: "58" }),
    );
    expect(panel().textContent).not.toContain("✕");
    toggleAccountPanel("s1", "<local>", host);
    seed({ quota: refused(true) }, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain("✕");
    expect(panel().textContent).not.toContain(
      copyText("acct.val.refusedPct", { pct: "100" }),
    );
  });
});

describe("账号面板 · 重启切换", () => {
  /** 热切换不成立 ⇒ 主按钮是「重启切换」；点它，等那一趟回来。 */
  async function restart(reply: unknown): Promise<void> {
    switchRestart.mockResolvedValue({ s1: reply });
    seed(
      {
        account: {
          start: "work",
          current: "work",
          since: NOW,
          history: [],
          inPlace: "noRelay",
        },
      },
      undefined,
    );
    openAccountPanel("s1", "<local>", host);
    [...panel().querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === copyText("acct.sw.restart"))!
      .click();
    await flush();
  }
  const toasts = (): HTMLElement[] => [
    ...document.querySelectorAll<HTMLElement>(
      '[role="alert"], [role="status"]',
    ),
  ];
  const toastButtons = (t: HTMLElement): string[] =>
    [...t.querySelectorAll("button")]
      .map((b) => b.textContent ?? "")
      .filter((x) => x !== "");

  it("成了 ⇒ 接回后端说的那个终端、说一句成了", async () => {
    await restart({ state: "done", terminal: "proj-cc-2" });
    expect(switchRestart).toHaveBeenCalledWith(
      "<local>",
      [expect.objectContaining({ sid: "s1", compact_first: false })],
      "team",
      expect.any(Number),
    );
    expect(runRemoteAttach).toHaveBeenCalledWith(
      "<local>",
      expect.any(String),
      "proj-cc-2",
      { quiet: true },
    );
    expect(toasts().map((t) => t.textContent)).toEqual([
      expect.stringContaining(
        copyText("acct.sw.doneRestart", { name: "team", session: "orders" }),
      ),
    ]);
  });

  it("没成、旧会话还在 ⇒ 「原会话保留 · 原因」，只带［日志］", async () => {
    await restart({ state: "failed", code: "stop_failed", old: "kept" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain(
      copyText("acct.sw.failRestart", {
        reason: copyText("acct.reason.stopFailed"),
      }),
    );
    expect(toastButtons(t)).toEqual([copyText("acct.sw.log")]);
    [...t.querySelectorAll("button")]
      .find((b) => b.textContent === copyText("acct.sw.log"))!
      .click();
    expect(openLog).toHaveBeenCalledTimes(1);
    expect(runRemoteAttach).not.toHaveBeenCalled();
  });

  it("没成、旧会话已停 ⇒ 「原会话已结束 · 原因」带［恢复…］［日志］，点了各做各的", async () => {
    await restart({ state: "failed", code: "start_failed", old: "ended" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain(
      copyText("acct.sw.failRestartEnded", {
        ended: copyText("sessionState.ended.name"),
        reason: copyText("acct.reason.startFailed"),
      }),
    );
    expect(toastButtons(t)).toEqual([
      copyText("acct.sw.resume"),
      copyText("acct.sw.log"),
    ]);
    [...t.querySelectorAll("button")]
      .find((b) => b.textContent === copyText("acct.sw.resume"))!
      .click();
    expect(host.openResume).toHaveBeenCalledWith("s1");
    await restart({ state: "failed", code: "notArrived", old: "ended" });
    const last = toasts().at(-1)!;
    expect(last.textContent).toContain(
      copyText("acct.sw.failRestartEnded", {
        ended: copyText("sessionState.ended.name"),
        reason: copyText("acct.reason.notArrived"),
      }),
    );
    [...last.querySelectorAll("button")]
      .find((b) => b.textContent === copyText("acct.sw.log"))!
      .click();
    expect(openLog).toHaveBeenCalledTimes(1);
  });

  it("那台给的码逐个说成人话，不落「原因不明」", async () => {
    const said: Record<string, string> = {
      account_unavailable: copyText("acct.reason.accountUnavailable", {
        name: "team",
      }),
      ambiguous: copyText("acct.reason.ambiguous"),
      not_in_terminal: copyText("acct.reason.notInTerminal"),
      session_already_live: copyText("acct.reason.alreadyLive"),
      shutting_down: copyText("acct.reason.shuttingDown"),
    };
    for (const [code, words] of Object.entries(said)) {
      document.body.replaceChildren();
      if (document.querySelector('[role="dialog"]'))
        toggleAccountPanel("s1", "<local>", host);
      await restart({ state: "failed", code, old: "kept" });
      expect(toasts().at(-1)!.textContent, code).toContain(
        copyText("acct.sw.failRestart", { reason: words }),
      );
    }
  });
});

describe("账号面板 · 时间轴", () => {
  const T = 50_000;
  const tlPlan = (head: unknown) => ({
    errors: [],
    now: T,
    nowText: "02:00",
    from: T - 6 * 3600,
    fromText: "20:00",
    until: T + 18 * 3600,
    past: [],
    plan: [
      { from: T, fromText: "02:00", to: T + 3600, toText: "03:00", account: "team", why: null },
    ],
    lanes: [],
    effective: {},
    grid: [{ at: T, atText: "02:00", label: "02:00" }],
    head,
  });
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
  };
  const foldHead = (): HTMLElement =>
    [...panel().querySelectorAll<HTMLElement>("[data-fold-head]")].find((h) =>
      h.textContent?.includes(copyText("acct.tl.title")),
    )!;

  it("★ 轮换列表：后端预览里此刻过线的号 ⇒ 行尾「7d 到线」、用量那格改显那一窗；判定照后端泳道，界面不比数", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40 });
    const p = tlPlan({ account: "work", w: "5h", pct: 63 });
    readPlan.mockReset().mockResolvedValue({
      ...p,
      lanes: [
        { account: "team", spans: [{ from: T - 60, fromText: "01:59", to: T + 3600, toText: "03:00", state: "capped", n: 90, w: "7d" }], resets: [] },
        { account: "work", spans: [], resets: [] },
      ],
    });
    openAccountPanel("s1", "<local>", host);
    await settle();
    const team = panel().querySelector<HTMLElement>('[data-acct-row="team"]')!;
    expect(team.textContent).toContain(copyText("acct.tag.atLine", { w: "7d" }));
    expect(team.textContent).toContain("7d");
    const work = panel().querySelector<HTMLElement>('[data-acct-row="work"]')!;
    expect(work.textContent).not.toContain(copyText("acct.tag.atLine", { w: "5h" }));
  });

  it("★ 触发两格（本会话）：先交后端逐格校验，错的格框红、不写；界面不另判范围（面板那份前端自判的 50–99 删了：40 也交后端）", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40 });
    openAccountPanel("s1", "<local>", host);
    await settle();
    const cell = (w: string) =>
      panel().querySelector<HTMLInputElement>(`[data-rot-line-num="${w}"]`)!;
    checkRotation.mockResolvedValueOnce([{ cell: "cap.*.7d", code: "range", said: "0–99" }]);
    cell("7d").focus();
    cell("7d").value = "120";
    cell("7d").dispatchEvent(new Event("change"));
    cell("7d").blur();
    await settle();
    expect(checkRotation.mock.calls[0][1].cap).toEqual({ "*": { "7d": 120 } });
    expect(writeSessionRotation, "后端说错 ⇒ 不写").not.toHaveBeenCalled();
    expect(cell("7d").dataset.error).toBe("true");
    expect(cell("7d").value).toBe("120");
    cell("5h").focus();
    cell("5h").value = "40";
    cell("5h").dispatchEvent(new Event("change"));
    cell("5h").blur();
    await settle();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], {
      custom: { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40, cap: { "*": { "5h": 40 } } },
    });
  });

  it("问 `{sid, view: 24h}`；折着时摘要 ＝ 后端那一句顶行；换视窗 ⇒ 按新视窗重问", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40 });
    readPlan.mockReset().mockResolvedValue(tlPlan({ account: "team", w: "5h", pct: 63, toLine: { w: "5h", n: 27 } }));
    openAccountPanel("s1", "<local>", host);
    await settle();
    expect(readPlan).toHaveBeenLastCalledWith("<local>", { sid: "s1", view: "24h" });
    expect(foldHead().getAttribute("aria-expanded")).toBe("false");
    expect(foldHead().textContent).toContain(
      [copyText("rot.tl.now", { acct: "team", w: "5h", pct: 63 }), copyText("rot.tl.toLine", { w: "5h", n: 27 })].join(copyText("kit.text.sep")),
    );
    const calls = readPlan.mock.calls.length;
    foldHead().click();
    await settle();
    expect(readPlan.mock.calls.length, "摊开不重问（同一把钥匙）").toBe(calls);
    panel().querySelector<HTMLButtonElement>('[data-tl-view] button[data-key="6h"]')!.click();
    await settle();
    expect(readPlan).toHaveBeenLastCalledWith("<local>", { sid: "s1", view: "6h" });
  });

  it("卡住（后端 head.blocked）⇒ 自动摊开、顶行写成琥珀条那一形", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40 });
    readPlan.mockReset().mockResolvedValue(tlPlan({ blocked: { account: "team", at: T + 600, atText: "02:10", atRelText: "+10m", w: "5h" } }));
    openAccountPanel("s1", "<local>", host);
    await settle();
    expect(foldHead().getAttribute("aria-expanded")).toBe("true");
    const line = panel().querySelector<HTMLElement>("[data-tl-head]")!;
    expect(line.dataset.tlHead).toBe("blocked");
    expect(line.textContent).toContain(copyText("rot.tl.blocked", { acct: "team", at: "02:10", rel: "+10m" }));
    expect(panel().querySelector("[data-tl]"), "轴画出来了").not.toBeNull();
  });

  it("摊开时顶行按段排（窄了在段与段之间换行）：估那一段整段在一个框里", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], atLimit: "continue", wait: 40 });
    const est = { at: T + 2400, atText: "02:40", pct: 90, w: "5h" };
    readPlan.mockReset().mockResolvedValue(tlPlan({ account: "team", w: "5h", pct: 63, toLine: { w: "5h", n: 27 }, est }));
    openAccountPanel("s1", "<local>", host);
    await settle();
    foldHead().click();
    await settle();
    const line = panel().querySelector<HTMLElement>("[data-tl-head]")!;
    expect([...line.children].map((c) => c.textContent)).toEqual([
      copyText("rot.tl.now", { acct: "team", w: "5h", pct: 63 }),
      copyText("rot.tl.toLine", { w: "5h", n: 27 }),
      copyText("rot.tl.est", { at: "02:40", pct: 90 }),
    ]);
    expect(line.textContent).toBe(
      [...line.children].map((c) => c.textContent).join(copyText("kit.text.sep")),
    );
  });
});

describe("账号面板 · 跟随父会话（会话血缘）", () => {
  const src = (): HTMLButtonElement =>
    panel().querySelector<HTMLButtonElement>("[data-acct-src]")!;
  const items = (): HTMLButtonElement[] => [
    ...document.querySelectorAll<HTMLButtonElement>(
      'body > [role="menu"] [role^="menuitem"]',
    ),
  ];
  const btn = (text: string): HTMLButtonElement | undefined =>
    [...panel().querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === text,
    );
  const titled: AcctPanelHost = {
    ...host,
    sessionTitle: (sid) => (sid === "p1" ? "orders" : "orders-worker"),
  };
  const closeAccountPanel = (): void => {
    if (document.querySelector('[role="dialog"]'))
      toggleAccountPanel("s1", "<local>", titled);
    document.body.replaceChildren();
  };

  it("★ 没有父 ⇒ 下拉里没有「跟随父会话」；有父 ⇒ 紧跟「跟随默认」出这一项、右侧父标题 ＋ 父此刻那条规则；选它 ⇒ 写 \"parent\" 一次", async () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", titled);
    src().click();
    expect(
      items().some((i) => i.textContent?.startsWith(copyText("rot.src.parent"))),
    ).toBe(false);
    closeAccountPanel();
    seed({ parent: "p1", ruleName: "日常" }, undefined);
    openAccountPanel("s1", "<local>", titled);
    src().click();
    const labels = items().map((i) => i.textContent ?? "");
    const at = labels.findIndex((t) => t.startsWith(copyText("rot.src.parent")));
    expect(at, labels.join(" | ")).toBeGreaterThan(-1);
    expect(labels[at - 1]).toContain(copyText("rot.src.follow"));
    expect(labels[at]).toContain("orders");
    items()[at]!.click();
    await flush();
    expect(writeSessionRotation.mock.calls).toEqual([["<local>", ["s1"], "parent"]]);
  });

  it("★ 来源 ＝ 跟随父会话：下一行「跟随父会话 orders · 规则 日常」＋［打开父会话］［转为本会话］；列表只读；打开 ⇒ 交给宿主切过去", async () => {
    seed({ source: { parent: "p1" }, parent: "p1", ruleName: "日常" }, undefined);
    openAccountPanel("s1", "<local>", titled);
    const lead = panel().querySelector<HTMLElement>("[data-acct-lead]")!.textContent!;
    expect(lead).toContain(copyText("rot.src.leadParent", { name: "orders", rule: "日常" }));
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    btn(copyText("rot.src.openParent"))!.click();
    expect(titled.openSession).toHaveBeenCalledWith("p1");
    btn(copyText("rot.src.detach"))!.click();
    await flush();
    expect(writeSessionRotation.mock.calls.at(-1)).toEqual(["<local>", ["s1"], "detach"]);
  });

  it("★ 父是本会话那份 ⇒「· 本会话」；父没经过中转 ⇒「· 父会话未经中转 · 按默认 日常」；父不在会话列表里 ⇒［打开父会话］灰、悬停说原因", () => {
    seed({ source: { parent: "p1" }, parent: "p1" }, undefined);
    openAccountPanel("s1", "<local>", titled);
    expect(panel().querySelector("[data-acct-lead]")!.textContent).toContain(
      copyText("rot.src.leadParentOwn", { name: "orders" }),
    );
    closeAccountPanel();
    seed(
      { source: { parent: "p9" }, parent: "p9", parentMissing: true, ruleName: "日常" },
      undefined,
    );
    openAccountPanel("s1", "<local>", titled);
    expect(panel().querySelector("[data-acct-lead]")!.textContent).toContain(
      copyText("rot.src.leadParentMissing", { name: "orders-worker", rule: "日常" }),
    );
    const open = btn(copyText("rot.src.openParent"))!;
    expect(open.getAttribute("aria-disabled")).toBe("true");
    expect(open.title).toBe(copyText("rot.src.openParentOff"));
    open.click();
    expect(titled.openSession).not.toHaveBeenCalledWith("p9");
  });
});
