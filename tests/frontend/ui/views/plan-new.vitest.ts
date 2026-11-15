/**
 * 新计划框：领域照后端问来的那几档 · 那一行照后端写的 · 撞上已有的一片 ⇒ 领域锁住、主按钮变［切到这一片］·
 * pb 拒 ⇒ 路径下出它那一句、框不关 · 成了 ⇒ 交回落在哪一片；
 * 后端成品的那几格照摆：pb 不在 / 太旧 ⇒ `said` 进红线、`hint` 在它下面一行（后端分好的两格，不拼）·
 * 起 pb 没成 ⇒ `failed` 进红线 · ［新建］灰着时说的就是红线那一句（[refusal] 一处算）· 路径底下那一句是 `note`。
 * 期望手写，夹具是合成的中性例子。
 */
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { openNewPlan, refusal, type NewPlanHost } from "../../../../src/frontend/ui/views/plan-new.ts";
import type { PlanNewForm } from "../../../../src/frontend/ui/plan-reads.ts";
import { ControlError } from "../../../../src/frontend/ui/control-said.ts";

type Pb = PlanNewForm["pb"];

const form = (o: Partial<Omit<PlanNewForm, "pb">> & { pb?: Pb } = {}): PlanNewForm =>
  ({
    pb: { state: "ok", said: null },
    workspace: null,
    root: "/home/u/work",
    slice: null,
    domains: [
      { name: "研究", error: null, kinds: [] },
      { name: "软件", error: null, kinds: [] },
    ],
    existing: null,
    repoDir: null,
    note: null,
    line: null,
    failed: null,
    ...o,
  }) as PlanNewForm;

interface Asked {
  origin: string;
  dir: string;
  path: string;
  domain: string | null;
}

function host(answer: (a: Asked) => PlanNewForm, create?: NewPlanHost["create"]): { h: NewPlanHost; asked: Asked[]; done: unknown[] } {
  const asked: Asked[] = [];
  const done: unknown[] = [];
  const h: NewPlanHost = {
    machines: () => ["<local>", "devbox"],
    dirs: (o) => (o === "<local>" ? { known: ["/home/u/work"], cwds: ["/home/u/src/site"] } : { known: [], cwds: ["/srv/app"] }),
    ask: async (origin, dir, path, domain) => {
      const a = { origin, dir, path, domain };
      asked.push(a);
      return answer(a);
    },
    create: create ?? (async () => ({ rc: 0, said: "登记了", path: null, adopted: null })),
    done: (origin, root, slice, said) => done.push({ origin, root, slice, said }),
  };
  return { h, asked, done };
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};

const dlg = (): HTMLElement => document.querySelector<HTMLElement>("[role=dialog]")!;
const okBtn = (): HTMLButtonElement => [...dlg().querySelectorAll<HTMLButtonElement>("button")].at(-1)!;
const pathInput = (): HTMLInputElement => dlg().querySelector<HTMLInputElement>("input")!;
const typePath = async (v: string): Promise<void> => {
  pathInput().value = v;
  pathInput().dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
};
const segs = (): HTMLButtonElement[] => [...dlg().querySelectorAll<HTMLButtonElement>("[role=radiogroup] button")];
const alert = (): HTMLElement | null => dlg().querySelector<HTMLElement>("[role=alert]");
/** 红线紧跟着的那一行（下一步）。 */
const underAlert = (): string | null => (alert()?.nextElementSibling?.textContent ?? null) || null;
/** ［新建］灰着时说的那一句（悬停提示接管 title 时挪进 `data-kit-title`）。 */
const blockedSaid = (): string => okBtn().dataset.kitTitle ?? okBtn().title;
const greyed = (): boolean => okBtn().getAttribute("aria-disabled") === "true" || okBtn().disabled;

const opened = async (f: PlanNewForm): Promise<void> => {
  openNewPlan(host(() => f).h, { origin: "<local>", dir: "/home/u/work" });
  await flush();
};

afterEach(() => {
  document.body.replaceChildren();
});

describe("新计划框", () => {
  it("开框就问一次：几档领域照后端给的；没写路径 ⇒ 没有那一行、［新建］灰着", async () => {
    const { h, asked } = host(() => form());
    openNewPlan(h, { origin: "<local>", dir: "/home/u/work" });
    await flush();
    expect(asked[0]).toEqual({ origin: "<local>", dir: "/home/u/work", path: "", domain: null });
    expect(segs().map((b) => b.textContent)).toEqual(["研究", "软件"]);
    expect(dlg().querySelector("code")?.textContent ?? "").toBe("");
    expect(greyed()).toBe(true);
  });

  it("写了路径 ⇒ 重问，那一行照后端写的；选了领域带上它", async () => {
    const { h, asked } = host((a) => form({ line: a.path ? `pb repo ${a.path}${a.domain ? ` ${a.domain}` : ""}` : null, note: a.path ? `仓库 /home/u/work/${a.path}` : null }));
    openNewPlan(h, { origin: "<local>", dir: "/home/u/work" });
    await flush();
    await typePath("invoice");
    segs()[1].click();
    await flush();
    expect(asked.at(-1)).toEqual({ origin: "<local>", dir: "/home/u/work", path: "invoice", domain: "软件" });
    expect(dlg().querySelector("code")?.textContent).toBe("pb repo invoice 软件");
    expect(dlg().textContent).toContain("仓库 /home/u/work/invoice");
  });

  it("撞上已有的一片 ⇒ 领域锁成它的、主按钮［切到这一片］", async () => {
    const { h } = host((a) => (a.path === "ledger" ? form({ existing: { slice: "ledger", domain: "软件" }, line: "pb repo ledger", note: "已有此片 · 软件 · 只切换" }) : form()));
    openNewPlan(h, { origin: "<local>", dir: "/home/u/work" });
    await flush();
    await typePath("ledger");
    expect(okBtn().textContent).toContain("plan.new.switch");
    const group = dlg().querySelector<HTMLElement>("[role=radiogroup]")!;
    expect(group.closest<HTMLElement>("[data-locked]")?.dataset.locked).toBe("true");
    expect(group.inert).toBe(true);
    expect(segs().find((b) => b.getAttribute("aria-checked") === "true")?.textContent).toBe("软件");
    expect(dlg().textContent).toContain("已有此片 · 软件 · 只切换");
  });

  it("pb 拒 ⇒ 路径下出 pb 那一句、框不关；成了 ⇒ 交回落在哪一片", async () => {
    let refuse = true;
    const { h, done } = host(
      (a) => form({ line: `pb repo ${a.path} 软件`, repoDir: `/home/u/work/${a.path}`, slice: a.path.split("/").at(-1) ?? null }),
      async () => {
        if (refuse) throw new ControlError("名字只用字母、数字、- 和 _");
        return { rc: 0, said: "登记了 invoice", path: null, adopted: null };
      },
    );
    openNewPlan(h, { origin: "<local>", dir: "/home/u/work" });
    await flush();
    await typePath("my invoice");
    segs()[1].click();
    await flush();
    okBtn().click();
    await flush();
    expect(dlg()).toBeTruthy();
    expect(dlg().textContent).toContain("名字只用字母、数字、- 和 _");
    expect(done).toEqual([]);
    refuse = false;
    await typePath("notes/invoice");
    okBtn().click();
    await flush();
    expect(done).toEqual([{ origin: "<local>", root: "/home/u/work", slice: "invoice", said: "登记了 invoice" }]);
  });
});

describe("新计划框照后端那一份摆", () => {
  it("找不到 pb ⇒ 那一句进红线、下一步在它下面一行；［新建］灰着说那一句", async () => {
    await opened(form({ pb: { state: "missing", said: "后端·未找到", hint: "后端·改填路径" }, domains: [] }));
    expect(alert()?.textContent).toContain("后端·未找到");
    expect(alert()?.textContent).not.toContain("后端·改填路径");
    expect(underAlert()).toBe("后端·改填路径");
    expect(greyed()).toBe(true);
    expect(blockedSaid()).toBe("后端·未找到");
  });

  it("pb 太旧 ⇒ 「要更新」进红线、pb 那一半在下一行", async () => {
    await opened(form({ pb: { state: "unsupported", said: "后端·要更新", hint: "后端·缺 dump" }, domains: [] }));
    expect(alert()?.textContent).toContain("后端·要更新");
    expect(underAlert()).toBe("后端·缺 dump");
  });

  it("起 pb 没成 ⇒ 那一句进红线，没有下一步那一行；［新建］灰着说它", async () => {
    await opened(form({ failed: "后端·读计划失败" }));
    expect(alert()?.textContent).toContain("后端·读计划失败");
    expect(underAlert()).toBeNull();
    expect(blockedSaid()).toBe("后端·读计划失败");
  });

  it("都好 ⇒ 没有红线；路径底下照 `note` 那一句", async () => {
    await opened(form({ note: "后端·仓库 /w/x", line: "后端·pb repo x" }));
    expect(alert()).toBeNull();
    expect(dlg().textContent).toContain("后端·仓库 /w/x");
    expect(dlg().querySelector("code")?.textContent).toBe("后端·pb repo x");
  });
});

describe("拦下的那一句（红线与［新建］灰着说的同一处算）", () => {
  it("pb 那一句在前；pb 好着 ⇒ 起 pb 没成那一句；都没有 ⇒ 没有", () => {
    expect(refusal(form({ pb: { state: "missing", said: "后端·未找到" }, failed: "后端·读计划失败" }))).toBe("后端·未找到");
    expect(refusal(form({ failed: "后端·读计划失败" }))).toBe("后端·读计划失败");
    expect(refusal(form())).toBeNull();
    expect(refusal(null)).toBeNull();
  });

  it("两样都有 ⇒ 红线与［新建］灰着说的都是 pb 那一句", async () => {
    await opened(form({ pb: { state: "missing", said: "后端·未找到", hint: "后端·改填路径" }, failed: "后端·读计划失败", domains: [] }));
    expect(alert()?.textContent).toContain("后端·未找到");
    expect(alert()?.textContent).not.toContain("后端·读计划失败");
    expect(blockedSaid()).toBe("后端·未找到");
  });
});
