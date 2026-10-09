/**
 * 设置「轮换」栏规则列表下那条时间轴：问 `{machine: true, view}`（这台全部号）· 顶行照额度账的此刻可用 / 最早恢复 ·
 * 没有本会话轨 · 行头带「在用 N 会话」· 号多过 8 个才有「只看在用的」· 换视窗重问 · 读不到整块不画。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const readPlan = vi.fn();
const readQuota = vi.fn();
vi.mock("../../../../src/frontend/ui/quota-reads", async (orig) => ({
  ...(await orig<typeof import("../../../../src/frontend/ui/quota-reads")>()),
  readPlan: (...a: unknown[]) => readPlan(...a),
  readQuota: (...a: unknown[]) => readQuota(...a),
}));

import { MachineTimeline } from "../../../../src/frontend/ui/settings/rules-timeline";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const T = 70_000;
const lane = (account: string, usedBy: number) => ({ account, pct: 10, usedBy, spans: [], resets: [] });
const plan = (n = 3) => ({
  errors: [],
  now: T,
  nowText: "02:00",
  from: T - 6 * 3600,
  fromText: "20:00",
  until: T + 18 * 3600,
  plan: [],
  lanes: Array.from({ length: n }, (_, i) => lane(`a${i}`, i === 0 ? 2 : 0)),
  effective: {},
  grid: [{ at: T, atText: "02:00", label: "02:00" }],
});
const settle = async (): Promise<void> => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  readPlan.mockReset().mockResolvedValue(plan());
  readQuota.mockReset().mockResolvedValue({
    state: "present",
    now: T,
    accounts: [{}],
    unseen: [],
    usableNow: ["a1", "a2"],
    earliestReturn: { account: "a0", at: T + 3600, atText: "03:00" },
  });
  document.body.replaceChildren();
});

describe("设置 · 轮换 · 时间轴", () => {
  it("问这台全部号（24h）；顶行 可用 … · 最早恢复 …；没有本会话轨；行头 在用 N 会话；换视窗 ⇒ 重问", async () => {
    const tl = new MachineTimeline();
    document.body.appendChild(tl.element);
    tl.load("devbox");
    await settle();
    expect(readPlan).toHaveBeenLastCalledWith("devbox", { machine: true, view: "24h" });
    expect(tl.element.querySelector("[data-tl-head]")!.textContent).toBe(
      [copyText("rot.tl.usable", { list: "a1, a2" }), copyText("rot.tl.earliest", { acct: "a0", at: "03:00" })].join(
        copyText("kit.text.sep"),
      ),
    );
    expect(tl.element.querySelector('[data-tl-row="track"]')).toBeNull();
    expect(tl.element.querySelector('[data-tl-row="a0"] [data-tl-used-by]')!.textContent).toBe(
      copyText("rot.tl.usedBy", { n: 2 }),
    );
    expect(tl.element.querySelector("[data-tl-only-used]"), "3 个号不出").toBeNull();
    tl.element.querySelector<HTMLButtonElement>('[data-tl-view] button[data-key="7d"]')!.click();
    await settle();
    expect(readPlan).toHaveBeenLastCalledWith("devbox", { machine: true, view: "7d" });
  });

  it("号多过 8 个 ⇒「只看在用的」，勾上只剩在用的那几条；读不到 ⇒ 整块不画", async () => {
    readPlan.mockResolvedValue(plan(9));
    const tl = new MachineTimeline();
    document.body.appendChild(tl.element);
    tl.load("devbox");
    await settle();
    const only = tl.element.querySelector<HTMLInputElement>("[data-tl-only-used] input")!;
    only.click();
    const rows = [...tl.element.querySelectorAll<HTMLElement>("[data-tl-row]")].map((r) => r.dataset.tlRow);
    expect(rows).toEqual(["ticks", "a0"]);
    readPlan.mockRejectedValue(new Error("offline"));
    tl.load("devbox");
    await settle();
    expect(tl.element.childElementCount).toBe(0);
  });
});
