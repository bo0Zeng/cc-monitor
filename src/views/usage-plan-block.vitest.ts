/**
 * S10：用量视图里的「账号 plan 窗口」块。
 *
 * 钉两条主计划点名的性质：**打开页面不自动探**、**失败必须可见**。
 * 后者是 F10 建立的路径 —— 探针的「静止 3s」是预算不是实测值，卡顿会抓早，
 * 那时必须让用户看见「认不出」和原始屏，**而不是给一个错的数字**。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../remote-config", () => ({
  readRemoteConfig: vi.fn().mockResolvedValue({ enabled: true, hosts: [] }),
}));
vi.mock("../account-chip", () => ({ pickPrimaryOrigin: () => "aya" }));
vi.mock("../accounts", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({}),
  currentWorkingAccount: () => ({ name: "z", configDir: "/c" }),
}));
const { outcome } = vi.hoisted(() => ({ outcome: { value: {} as unknown } }));
vi.mock("../account-usage", () => ({
  fetchAccountUsage: vi.fn(async () => outcome.value),
  OK_USAGE_UNVERIFIED_CAVEAT: "CAVEAT",
}));

import { UsageView } from "./usage-view";

/** 只建视图（不 open —— open 会去拉会话数据，本测只关心 plan 块）。 */
function mountBlock(): HTMLElement {
  const v = new UsageView();
  // 视图的 root 是私有的；plan 块通过 class 找。
  const root = (v as unknown as { root: HTMLElement }).root;
  return root.querySelector<HTMLElement>(".usage-plan")!;
}

const clickLoad = async (block: HTMLElement) => {
  block.querySelector<HTMLButtonElement>(".usage-plan-load")!.click();
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  outcome.value = { status: "ok", buckets: [] };
});

describe("S10 plan 窗口块", () => {
  it("★ 建视图时只有一个按钮，**不自动探**（一次探测要在远端起 tmux 跑 /usage）", () => {
    const block = mountBlock();
    expect(block.querySelector(".usage-plan-load")).not.toBeNull();
    expect(block.querySelector(".usage-plan-result")).toBeNull();
  });

  it("ok → 列出每个窗口，并挂上「这数字有多可信」的说明", async () => {
    outcome.value = {
      status: "ok",
      buckets: [{ label: "会话窗口", usedPercent: 12, resetIn: "2:20am" }],
    };
    const block = mountBlock();
    await clickLoad(block);
    const row = block.querySelector<HTMLElement>(".usage-plan-row")!;
    expect(row.textContent).toContain("会话窗口");
    expect(row.textContent).toContain("12%");
    expect(row.title).toBe("CAVEAT");
  });

  it("★ 认不出格式 → 说「认不出」+ **把原始屏带回来** + 给复制按钮", async () => {
    // 这条就是主计划要求合并后必须保住的可见失败路径。
    outcome.value = { status: "unrecognized", reason: "无百分比", raw: "RAW-SCREEN" };
    const block = mountBlock();
    await clickLoad(block);
    const box = block.querySelector<HTMLElement>(".usage-plan-result")!;
    expect(box.dataset.status).toBe("unrecognized");
    expect(box.querySelector(".usage-plan-fail")!.textContent).toContain("认不出");
    expect(box.querySelector<HTMLTextAreaElement>(".usage-plan-raw")!.value).toBe(
      "RAW-SCREEN",
    );
    expect(
      [...box.querySelectorAll("button")].some((b) => b.textContent === "复制诊断文本"),
    ).toBe(true);
  });

  it("★ 失败时**不显示任何百分比**（宁可说不知道，也不给错数字）", async () => {
    outcome.value = { status: "unrecognized", reason: "x", raw: "RAW" };
    const block = mountBlock();
    await clickLoad(block);
    expect(block.querySelector(".usage-plan-row")).toBeNull();
  });

  it("探测失败 / 未登录 / 没有 claude —— 各自说清是哪种，不混成一句", async () => {
    for (const [o, expected] of [
      [{ status: "probe-failed", error: "ssh 挂了" }, "ssh 挂了"],
      [{ status: "not-logged-in" }, "还没登录"],
      [{ status: "cli-missing" }, "找不到 claude"],
    ] as const) {
      outcome.value = o;
      const block = mountBlock();
      await clickLoad(block);
      expect(block.querySelector(".usage-plan-fail")!.textContent).toContain(expected);
    }
  });
});
