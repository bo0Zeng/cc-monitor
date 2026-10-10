/**
 * 机器页「轮换」栏的判据：表（默认那条在最前带「默认」· 摘要 · 在用）· ⋯（默认那条删除灰、说为什么）· 改名（后端判重名，框红 ＋ 红字）·
 * 复制（`{name} 副本` ＋ `dedupe`，出来就在改名态）· 设为默认（toast 撤销 ＝ 设回原来那条）· 删除（没人用不问、带撤销；有人用 ⇒
 * 确认框选落到哪）· 在用展开（标题取会话清单；勾上的 / 全部 ⇒ 改用 · 转为本会话）· 批量条（含默认删除灰）· 新建浮层 ·
 * 复制到别的机器（那台回它没有的号）· 多过 12 条出筛选框 · 只有默认那条出空态一行 · 读不到 ⇒ 警告条。
 *
 * 读口 / 写口整块换成假的（`vi.mock`）：这里只量「拿到这些事实，画成什么样、交出去什么」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import type {
  RuleRow,
  RulesRead,
} from "../../../../src/frontend/ui/quota-reads";
import type { Rotation } from "../../../../src/frontend/ui/generated/Rotation";

const readRules = vi.fn();
const saveRule = vi.fn();
const renameRule = vi.fn();
const deleteRules = vi.fn();
const setDefaultRule = vi.fn();
const writeSessionRotation = vi.fn();
const fetchList = vi.fn();
const toast = vi.fn();
const failToast = vi.fn();

vi.mock("../../../../src/frontend/ui/kit/toast", () => ({
  toast: (...a: unknown[]) => toast(...a),
  failToast: (...a: unknown[]) => failToast(...a),
}));
vi.mock("../../../../src/frontend/ui/quota-reads", () => ({
  readRules: (...a: unknown[]) => readRules(...a),
  saveRule: (...a: unknown[]) => saveRule(...a),
  renameRule: (...a: unknown[]) => renameRule(...a),
  deleteRules: (...a: unknown[]) => deleteRules(...a),
  setDefaultRule: (...a: unknown[]) => setDefaultRule(...a),
  writeSessionRotation: (...a: unknown[]) => writeSessionRotation(...a),
  // 列表下那条时间轴：这里不量它（rules-timeline.vitest.ts 量），问了不回。
  readPlan: () => new Promise(() => {}),
  readQuota: () => new Promise(() => {}),
}));
vi.mock("../../../../src/frontend/ui/history-list-reads", () => ({
  fetchList: (...a: unknown[]) => fetchList(...a),
}));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({
  readRemoteConfig: () =>
    Promise.resolve({ hosts: [{ label: "devbox" }, { label: "gpu-01" }] }),
}));
vi.mock("../../../../src/frontend/ui/events", () => ({
  bindEvents: () => Promise.resolve(),
}));

import { RulesSection } from "../../../../src/frontend/ui/settings/rules-section";
import {
  __resetMachineContextForTests,
  setCurrentMachine,
} from "../../../../src/frontend/ui/settings/machine-context";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { ConfirmSpec } from "../../../../src/frontend/ui/kit/dialog";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const ROT: Rotation = {
  order: [{ start: true }, "team"],
  enabled: ["team"],
  when: "full",
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
    users: { live: 0, ended: 0, follow: 0, doing: {}, sids: [], endedSids: [] },
    summary: `${name} · team · 满`,
    explain: "",
    missing: [],
    atLimitApplies: false,
    ...p,
  };
}

const W = { state: "working", needs: null } as const;
const E = { state: "ended", needs: null } as const;
const DAILY = rule("r_daily", "日常", {
  isDefault: true,
  users: {
    live: 2,
    ended: 1,
    follow: 2,
    doing: { "s-a": W, "s-b": W, "s-c": E },
    sids: ["s-a", "s-b"],
    endedSids: ["s-c"],
  },
});
const NIGHT = rule("r_night", "夜间", {
  users: {
    live: 1,
    ended: 2,
    follow: 0,
    doing: { "s-d": W, "s-e": E, "s-f": E },
    sids: ["s-d"],
    endedSids: ["s-e", "s-f"],
  },
});
const SAVER = rule("r_saver", "省额度");

function rules(list: RuleRow[] = [DAILY, NIGHT, SAVER]): RulesRead {
  return {
    state: "present",
    defaultRule: list.find((r) => r.isDefault)?.id ?? "r_daily",
    rules: list,
  };
}

const LIST = {
  rows: [
    { sessionId: "s-a", label: "orders", status: "live" },
    { sessionId: "s-b", label: "billing", status: "live" },
    { sessionId: "s-c", label: "old-1", status: "ended" },
    { sessionId: "s-d", label: "ranker", status: "live" },
    { sessionId: "s-e", label: "old-2", status: "ended" },
  ],
  groups: [],
  total: 5,
  truncated: false,
  notice: null,
};

let confirmed: ConfirmSpec[] = [];
let answer = true;
let pickInConfirm: string | null = null;
const confirm = (spec: ConfirmSpec): Promise<boolean> => {
  confirmed.push(spec);
  const c = spec.rows?.find((r) => r.choice)?.choice;
  if (c && pickInConfirm) c.onPick(pickInConfirm);
  return Promise.resolve(answer);
};
const settle = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
};
async function mount(): Promise<HTMLElement> {
  const s = new RulesSection({ confirm });
  document.body.replaceChildren(s.element);
  s.loadNow();
  await settle();
  return s.element;
}
const rowOf = (el: HTMLElement, id: string): HTMLElement =>
  el.querySelector<HTMLElement>(`[data-rule="${id}"]`)!;
const menuItems = (): HTMLButtonElement[] => [
  ...document.querySelectorAll<HTMLButtonElement>(
    '[role="menu"] [role^="menuitem"]',
  ),
];
const menuItem = (label: string): HTMLButtonElement =>
  menuItems().find(
    (b) => b.querySelector('[data-part="label"]')?.textContent === label,
  )!;
async function openMore(el: HTMLElement, id: string): Promise<void> {
  el.querySelector<HTMLButtonElement>(`[data-rules-more="${id}"]`)!.click();
  await settle();
}
const lastToast = (): {
  title: string;
  opts: { action?: { label: string; run: () => void } };
} => {
  const c = toast.mock.calls.at(-1)!;
  return {
    title: c[0] as string,
    opts: (c[2] ?? {}) as { action?: { label: string; run: () => void } },
  };
};

beforeEach(() => {
  for (const f of [
    readRules,
    saveRule,
    renameRule,
    deleteRules,
    setDefaultRule,
    writeSessionRotation,
    fetchList,
    toast,
    failToast,
  ])
    f.mockReset();
  readRules.mockResolvedValue(rules());
  fetchList.mockResolvedValue(LIST);
  confirmed = [];
  answer = true;
  pickInConfirm = null;
  __resetMachineContextForTests();
  setCurrentMachine("devbox");
  document.body.replaceChildren();
});

describe("轮换栏 · 表", () => {
  it("段头 `devbox 的规则  默认 日常`；默认那条在最前带「默认」；摘要后端写的照抄；在用 N 会话 / 没人用写 —", async () => {
    const el = await mount();
    expect(el.querySelector("h3")!.textContent).toBe(
      copyText("rot.list.title", { machine: "devbox" }),
    );
    expect(el.textContent).toContain(
      copyText("rot.list.defaultIs", { name: "日常" }),
    );
    const ids = [...el.querySelectorAll<HTMLElement>("[data-rule]")].map(
      (r) => r.dataset.rule,
    );
    expect(ids).toEqual(["r_daily", "r_night", "r_saver"]);
    expect(rowOf(el, "r_daily").textContent).toContain(
      copyText("rot.src.tagDefault"),
    );
    expect(rowOf(el, "r_night").textContent).not.toContain(
      copyText("rot.src.tagDefault"),
    );
    expect(rowOf(el, "r_night").textContent).toContain("夜间 · team · 满");
    expect(
      rowOf(el, "r_daily").querySelector("[data-rules-use]")!.textContent,
    ).toBe(copyText("rot.list.useN", { n: 2 }));
    expect(
      rowOf(el, "r_saver").querySelector("[data-rules-use]"),
      "没人用 ⇒ 不可点",
    ).toBeNull();
    expect(rowOf(el, "r_saver").textContent).toContain(
      copyText("rot.list.useNone"),
    );
    expect(
      rowOf(el, "r_night").dataset.anchor,
      "编辑规则… 带目的地落到这一行",
    ).toBe("rule:r_night");
    expect(readRules).toHaveBeenCalledWith("devbox");
  });

  it("默认那条的 ⋯：编辑在最前 · 设为默认打勾不可点 · 删除灰、第二行「默认规则 · 先设别的为默认」", async () => {
    const el = await mount();
    await openMore(el, "r_daily");
    const labels = menuItems().map(
      (b) => b.querySelector('[data-part="label"]')?.textContent,
    );
    expect(labels).toEqual([
      copyText("rot.act.edit"),
      copyText("rot.act.copy"),
      copyText("rot.act.rename"),
      copyText("rot.act.setDefault"),
      copyText("rot.act.copyTo"),
      copyText("rot.act.delete"),
    ]);
    expect(
      menuItem(copyText("rot.act.setDefault")).getAttribute("aria-checked"),
    ).toBe("true");
    const del = menuItem(copyText("rot.act.delete"));
    expect(del.disabled).toBe(true);
    expect(del.querySelector('[data-part="why"]')!.textContent).toBe(
      copyText("rot.act.defaultNoDelete"),
    );
  });

  it("只有默认那一条 ⇒ 表下一行灰字 ＋［新建规则］；多过 12 条 ⇒ 段头下出筛选框、输入即筛", async () => {
    readRules.mockResolvedValueOnce(rules([DAILY]));
    let el = await mount();
    expect(el.querySelector("[data-rules-empty]")!.textContent).toContain(
      copyText("rot.list.emptyHint"),
    );
    expect(el.querySelector('input[type="search"]')).toBeNull();
    const many = [
      DAILY,
      ...Array.from({ length: 12 }, (_, i) => rule(`r_${i}`, `规则${i}`)),
    ];
    readRules.mockResolvedValue(rules(many));
    el = await mount();
    expect(el.querySelector("[data-rules-empty]")).toBeNull();
    const f = el.querySelector<HTMLInputElement>('input[type="search"]')!;
    expect(f.placeholder).toBe(copyText("rot.list.filter"));
    f.value = "规则1";
    f.dispatchEvent(new Event("input"));
    expect(
      [...el.querySelectorAll<HTMLElement>("[data-rule]")].map(
        (r) => r.dataset.rule,
      ),
    ).toEqual(["r_1", "r_10", "r_11"]);
  });

  it("读不到那台的规则 ⇒ 警告条 ＋［重试］，新建灰", async () => {
    readRules.mockRejectedValueOnce(new Error("offline"));
    const el = await mount();
    expect(el.textContent).toContain(
      copyText("rot.list.unreadable", { machine: "devbox" }),
    );
    expect(
      el
        .querySelector<HTMLButtonElement>("[data-rules-new]")!
        .getAttribute("aria-disabled"),
    ).toBe("true");
    expect(el.querySelector("[data-rule]")).toBeNull();
  });

  it("切机器：只认最后一趟（晚到的那台整份作废）", async () => {
    let late: (v: RulesRead) => void = () => {};
    readRules.mockImplementationOnce(() => new Promise((r) => (late = r)));
    const s = new RulesSection({ confirm });
    document.body.replaceChildren(s.element);
    s.loadNow();
    readRules.mockResolvedValueOnce(
      rules([rule("r_x", "本机那条", { isDefault: true })]),
    );
    setCurrentMachine(LOCAL_ORIGIN);
    await settle();
    late(rules());
    await settle();
    expect(
      [...s.element.querySelectorAll<HTMLElement>("[data-rule]")].map(
        (r) => r.dataset.rule,
      ),
    ).toEqual(["r_x"]);
  });
});

describe("轮换栏 · 改名 · 复制 · 设为默认", () => {
  it("改名：行内框，Enter 交那台（带读到的版本）；重名 ⇒ 框红 ＋ 红字「重名」，不退出", async () => {
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.rename")).click();
    await settle();
    const inp = el.querySelector<HTMLInputElement>(
      '[data-rules-rename="r_night"]',
    )!;
    expect(document.activeElement).toBe(inp);
    renameRule.mockResolvedValueOnce({
      state: "refused",
      errors: [{ cell: "name", code: "dup" }],
    });
    inp.value = "日常";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await settle();
    expect(renameRule).toHaveBeenCalledWith("devbox", {
      id: "r_night",
      name: "日常",
      ifRev: 3,
    });
    const again = el.querySelector<HTMLInputElement>(
      '[data-rules-rename="r_night"]',
    )!;
    expect(again.dataset.error).toBe("true");
    expect(rowOf(el, "r_night").textContent).toContain(
      copyText("rot.save.dup"),
    );
  });

  it("改名：Esc 取消，不发命令", async () => {
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.rename")).click();
    await settle();
    const inp = el.querySelector<HTMLInputElement>(
      '[data-rules-rename="r_night"]',
    )!;
    inp.value = "别的";
    inp.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await settle();
    expect(el.querySelector("[data-rules-rename]")).toBeNull();
    expect(renameRule).not.toHaveBeenCalled();
  });

  it("复制：交 `{name: 夜间 副本, from, dedupe}`（重名由那台加号），出来的那条直接在改名态", async () => {
    const copied = rule("r_copy", "夜间 副本");
    saveRule.mockResolvedValueOnce({ state: "saved", rule: copied });
    readRules
      .mockResolvedValueOnce(rules())
      .mockResolvedValue(rules([DAILY, NIGHT, SAVER, copied]));
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.copy")).click();
    await settle();
    expect(saveRule).toHaveBeenCalledWith("devbox", {
      name: copyText("rot.act.copySuffix", { name: "夜间" }),
      from: "r_night",
      dedupe: true,
    });
    expect(
      el.querySelector<HTMLInputElement>('[data-rules-rename="r_copy"]'),
    ).not.toBeNull();
  });

  it("设为默认：即写；toast `默认规则 夜间 · 跟随默认的 3 会话随之改` ＋［撤销］＝ 设回日常", async () => {
    setDefaultRule.mockResolvedValue({ defaultRule: "r_night", followers: 3 });
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.setDefault")).click();
    await settle();
    expect(setDefaultRule).toHaveBeenCalledWith("devbox", "r_night");
    const t = lastToast();
    expect(t.title).toBe(
      copyText("rot.done.setDefault", { name: "夜间", n: 3 }),
    );
    expect(t.opts.action!.label).toBe(copyText("kit.toast.undo"));
    t.opts.action!.run();
    expect(setDefaultRule).toHaveBeenLastCalledWith("devbox", "r_daily");
  });

  it("复制到 ▸：列别的机器（不含此刻这台）；那台存同名（dedupe）、回它没有的号 ⇒ toast 照实说", async () => {
    saveRule.mockResolvedValueOnce({
      state: "saved",
      rule: rule("r_far", "夜间 2", { missing: ["team", "lab"] }),
    });
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.copyTo")).click();
    await settle();
    expect(
      menuItems().map(
        (b) => b.querySelector('[data-part="label"]')?.textContent,
      ),
    ).toEqual([copyText("control.machine.local"), "gpu-01"]);
    menuItem("gpu-01").click();
    await settle();
    expect(saveRule).toHaveBeenCalledWith("gpu-01", {
      name: "夜间",
      rotation: ROT,
      dedupe: true,
    });
    expect(lastToast().title).toBe(
      copyText("rot.done.copiedToMissing", {
        machine: "gpu-01",
        name: "夜间 2",
        n: 2,
        list: ["team", "lab"].join(copyText("kit.text.sep")),
      }),
    );
  });
});

describe("轮换栏 · 删除", () => {
  it("没人在用 ⇒ 不问、直接删（then custom）；toast［撤销］＝ 照原样再存一条", async () => {
    deleteRules.mockResolvedValue({});
    saveRule.mockResolvedValue({ state: "saved", rule: SAVER });
    const el = await mount();
    await openMore(el, "r_saver");
    menuItem(copyText("rot.act.delete")).click();
    await settle();
    expect(confirmed).toEqual([]);
    expect(deleteRules).toHaveBeenCalledWith("devbox", ["r_saver"], "custom");
    const t = lastToast();
    expect(t.title).toBe(copyText("rot.done.deleted", { name: "省额度" }));
    t.opts.action!.run();
    expect(saveRule).toHaveBeenCalledWith("devbox", {
      name: "省额度",
      rotation: ROT,
    });
  });

  it("在用 ⇒ 确认框：改动 `在用 1 会话（ranker）` ＋ 单选（缺省转为本会话）· 之后 `已结束的 2 会话同样处理`；选跟随默认 ⇒ then follow", async () => {
    deleteRules.mockResolvedValue({ "s-d": "follow" });
    pickInConfirm = "follow";
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.delete")).click();
    await settle();
    const spec = confirmed[0];
    expect(spec.title).toBe(copyText("rot.del.title", { name: "夜间" }));
    expect(
      spec.title,
      "规则名是人敲的字 ⇒ 「」括起（同「结束会话「…」」）",
    ).toMatch(/「夜间」$/);
    expect(spec.danger).toBe(true);
    expect(spec.rows![0].items).toEqual([
      copyText("rot.del.inUse", { n: 1, list: "ranker" }),
    ]);
    expect(spec.rows![0].choice!.value).toBe("custom");
    expect(spec.rows![0].choice!.options.map((o) => o.label)).toEqual([
      copyText("rot.del.toCustom"),
      copyText("rot.del.toFollow", { name: "日常" }),
    ]);
    expect(spec.rows![1].items).toEqual([
      copyText("sessionState.rotDelete.ended", { n: 2 }),
    ]);
    expect(deleteRules).toHaveBeenCalledWith("devbox", ["r_night"], "follow");
    expect(
      lastToast().opts.action,
      "在用的会话已经挪了 ⇒ 不给撤销",
    ).toBeUndefined();
  });

  it("在用、点了取消 ⇒ 不删", async () => {
    answer = false;
    const el = await mount();
    await openMore(el, "r_night");
    menuItem(copyText("rot.act.delete")).click();
    await settle();
    expect(confirmed).toHaveLength(1);
    expect(deleteRules).not.toHaveBeenCalled();
  });

  it("勾了两条 ⇒ 表头换成批量条 `已选 2`；含默认那条 ⇒ 删除灰（悬停「含默认规则」）", async () => {
    const el = await mount();
    const pick = (id: string): void => {
      const c = rowOf(el, id).querySelector<HTMLInputElement>(
        'input[type="checkbox"]',
      )!;
      c.checked = true;
      c.dispatchEvent(new Event("change"));
    };
    pick("r_night");
    pick("r_saver");
    const bar = el.querySelector<HTMLElement>("[data-rules-batch]")!;
    expect(bar.textContent).toContain(copyText("rot.batch.sel", { n: 2 }));
    const del = [...bar.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === copyText("rot.batch.delete"),
    )!;
    expect(del.getAttribute("aria-disabled")).not.toBe("true");
    pick("r_daily");
    const bar2 = el.querySelector<HTMLElement>("[data-rules-batch]")!;
    const del2 = [...bar2.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === copyText("rot.batch.delete"),
    )!;
    expect(del2.getAttribute("aria-disabled")).toBe("true");
  });
});

describe("轮换栏 · 在用展开 · 新建", () => {
  it("点「2 会话」⇒ 行下名单（标题取会话清单）；已结束的折在「已结束 1」；不勾 ⇒［全部改用 ▾］选一条写全部（含已结束）", async () => {
    writeSessionRotation.mockResolvedValue({
      "s-a": { state: "done" },
      "s-b": { state: "done" },
      "s-c": { state: "done" },
    });
    const el = await mount();
    rowOf(el, "r_daily")
      .querySelector<HTMLButtonElement>("[data-rules-use]")!
      .click();
    await settle();
    const box = el.querySelector<HTMLElement>('[data-rules-users="r_daily"]')!;
    expect(
      [...box.querySelectorAll<HTMLElement>("[data-sid]")].map(
        (x) => x.textContent,
      ),
    ).toEqual([
      `orders${copyText("sessionFace.dot.running")}`,
      `billing${copyText("sessionFace.dot.running")}`,
    ]);
    expect(box.querySelector("[data-rules-ended]")!.textContent).toBe(
      copyText("sessionState.rotUsers.ended", { n: 1 }),
    );
    const move = box.querySelector<HTMLButtonElement>("[data-rules-move]")!;
    expect(move.textContent).toBe(copyText("rot.batch.moveAll"));
    move.click();
    await settle();
    menuItem("夜间").click();
    await settle();
    expect(writeSessionRotation).toHaveBeenCalledWith(
      "devbox",
      ["s-a", "s-b", "s-c"],
      { rule: "r_night" },
    );
    expect(lastToast().title).toBe(
      copyText("rot.done.batch", {
        src: copyText("rot.src.rule", { name: "夜间" }),
        n: 3,
      }),
    );
  });

  it("名单每个会话的状态照后端给的（与主窗口标签页同一套）：运行中 · 空闲 · 等批准 · 等回答 · 需手动 · 已结束；点与读屏名同一句", async () => {
    const users = {
      live: 5,
      ended: 1,
      follow: 0,
      doing: {
        "s-a": { state: "working", needs: null },
        "s-b": { state: "idle", needs: null },
        "s-d": { state: "needsYou", needs: "approve" },
        "s-x": { state: "needsYou", needs: "answer" },
        "s-y": { state: "needsYou", needs: "unknown" },
        "s-c": { state: "ended", needs: null },
      },
      sids: ["s-a", "s-b", "s-d", "s-x", "s-y"],
      endedSids: ["s-c"],
    } as const;
    readRules.mockResolvedValue(
      rules([
        {
          ...DAILY,
          users: {
            ...users,
            doing: { ...users.doing },
            sids: [...users.sids],
            endedSids: [...users.endedSids],
          },
        },
      ]),
    );
    // 会话清单说 s-d 已结束、s-c 在跑：状态以规则表那一格为准，会话清单只给标题。
    fetchList.mockResolvedValue({
      ...LIST,
      rows: LIST.rows.map((r) =>
        r.sessionId === "s-d"
          ? { ...r, status: "ended" }
          : r.sessionId === "s-c"
            ? { ...r, status: "live" }
            : r,
      ),
    });
    const el = await mount();
    rowOf(el, "r_daily")
      .querySelector<HTMLButtonElement>("[data-rules-use]")!
      .click();
    await settle();
    const box = el.querySelector<HTMLElement>('[data-rules-users="r_daily"]')!;
    box.querySelector<HTMLButtonElement>("[data-rules-ended]")!.click();
    await settle();
    const lines = [
      ...el.querySelectorAll<HTMLElement>(
        '[data-rules-users="r_daily"] [data-sid]',
      ),
    ];
    const want: Record<string, [string, string]> = {
      "s-a": ["running", copyText("sessionFace.dot.running")],
      "s-b": ["idle", copyText("sessionFace.dot.idle")],
      "s-d": ["needs-you", copyText("tabBar.needsKind.approve")],
      "s-x": ["needs-you", copyText("tabBar.needsKind.answer")],
      "s-y": ["needs-you", copyText("tabBar.needsKind.unknown")],
      "s-c": ["ended", copyText("sessionState.ended.name")],
    };
    expect(lines.map((l) => l.dataset.sid)).toEqual([
      "s-a",
      "s-b",
      "s-d",
      "s-x",
      "s-y",
      "s-c",
    ]);
    for (const l of lines) {
      const [dot, word] = want[l.dataset.sid!]!;
      const d = l.querySelector<HTMLElement>('[role="img"]')!;
      expect(
        [
          d.dataset.state,
          d.getAttribute("aria-label"),
          l.textContent!.endsWith(word),
        ],
        l.dataset.sid,
      ).toEqual([dot, word, true]);
    }
  });

  it("勾一个 ⇒ 按钮换成「改用 · 转为本会话」，只写勾上的；转为本会话 ＝ detach", async () => {
    writeSessionRotation.mockResolvedValue({ "s-d": { state: "done" } });
    const el = await mount();
    rowOf(el, "r_night")
      .querySelector<HTMLButtonElement>("[data-rules-use]")!
      .click();
    await settle();
    const c = el.querySelector<HTMLInputElement>(
      '[data-rules-users="r_night"] [data-sid="s-d"] input',
    )!;
    c.checked = true;
    c.dispatchEvent(new Event("change"));
    const box = el.querySelector<HTMLElement>('[data-rules-users="r_night"]')!;
    expect(box.textContent).toContain(copyText("rot.list.picked", { n: 1 }));
    const detach = box.querySelector<HTMLButtonElement>("[data-rules-detach]")!;
    expect(detach.textContent).toBe(copyText("rot.batch.detach"));
    detach.click();
    await settle();
    expect(writeSessionRotation).toHaveBeenCalledWith(
      "devbox",
      ["s-d"],
      "detach",
    );
  });

  it("新建：浮层 名称 ＋ 从［日常 默认 ▾］· 建 ⇒ 交 `{name, from}`；重名 ⇒ 名称下红字", async () => {
    const el = await mount();
    el.querySelector<HTMLButtonElement>("[data-rules-new]")!.click();
    await settle();
    const form = document.querySelector<HTMLElement>("[data-rules-new-form]")!;
    const inp = form.querySelector<HTMLInputElement>("input")!;
    expect(document.activeElement).toBe(inp);
    saveRule.mockResolvedValueOnce({
      state: "refused",
      errors: [{ cell: "name", code: "dup" }],
    });
    inp.value = "夜间";
    form.querySelector<HTMLButtonElement>("[data-rules-new-ok]")!.click();
    await settle();
    expect(saveRule).toHaveBeenCalledWith("devbox", {
      name: "夜间",
      from: "r_daily",
    });
    expect(form.textContent).toContain(copyText("rot.save.dup"));
  });
});
