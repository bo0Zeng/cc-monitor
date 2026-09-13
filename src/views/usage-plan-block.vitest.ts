/**
 * S10：用量视图里的「账号 plan 窗口」块。
 *
 * 钉两条主计划点名的性质：**打开页面不自动探**、**失败必须可见**。
 *
 * 🔴 **`K-R101`/`R59`（09-13）改了「可见」的含义**：以前是「解析失败时让用户看见『认不出』
 * ＋ 原始屏」；今天生产路上**没有解析**了 ⇒ **成功那一条路本身就是给用户看那一屏原文**
 * （`R58` 逐字「即点击用量后把屏幕预览给我看」）。`captured=false` 才是失败态。
 * ⇒ 下面 `unrecognized` / `not-logged-in` / `cli-missing` 三个态**不再存在**，
 * 相应的用例改成断「原文到得了界面」那一条（`KR101D1`）。
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
// 🔴 **只 mock `fetchAccountUsage` 这一个口，其余原样用真的** ——
// `usageScreenEl` 是 `KR101D1` 的被测对象（「原文到得了界面」），
// 把它也 mock 掉这条判据就在断一个假货。
vi.mock("../account-usage", async (orig) => ({
  ...(await orig<typeof import("../account-usage")>()),
  fetchAccountUsage: vi.fn(async () => outcome.value),
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
  outcome.value = { status: "screen", raw: "" };
});

describe("S10 plan 窗口块", () => {
  it("★ 建视图时只有一个按钮，**不自动探**（一次探测要在远端起 tmux 跑 /usage）", () => {
    const block = mountBlock();
    expect(block.querySelector(".usage-plan-load")).not.toBeNull();
    expect(block.querySelector(".usage-plan-result")).toBeNull();
  });

  /**
   * 🔴 **`KR101D1`（`R58` 裁定一）在用量页这一面的判据 —— 本条是本文件的重心。**
   *
   * ⚠ 失效方向（件文件点名）：判「`raw` 字段还在类型里」。这里断的是**渲染出来的 DOM 文本
   * 逐字等于那一屏** —— 中途被谁 `trim()`、被谁截断 500 字、被谁塞进 `title` 而不是正文，
   * 本条都会红。
   */
  it("★ KR101D1：screen → 那一屏**原文逐字**落在页面上（等宽 · 保留空白）", async () => {
    const screen = "Current session\n  12% used\nResets 2:20am (America/Los_Angeles)\n\n  尾部空白  ";
    outcome.value = { status: "screen", raw: screen };
    const block = mountBlock();
    await clickLoad(block);
    const box = block.querySelector<HTMLElement>(".usage-plan-result")!;
    expect(box.dataset.status).toBe("screen");
    const pre = box.querySelector<HTMLElement>(".usage-screen-raw")!;
    expect(pre, "页面上没有那一屏原文的容器 —— 原文在中途被丢了").not.toBeNull();
    expect(pre.tagName).toBe("PRE"); // 等宽 + 保留空白（R59 逐字）
    expect(pre.style.whiteSpace).toBe("pre-wrap");
    expect(pre.textContent, "原文被改过（trim / 截断 / 转义？）").toBe(screen);
    expect([...box.querySelectorAll("button")].some((b) => b.textContent === "复制这一屏")).toBe(
      true,
    );
  });

  it("★ KR101D1 ③：抓到空屏也是成功 —— 照样出结果块，并明说它是空屏", async () => {
    outcome.value = { status: "screen", raw: "" };
    const block = mountBlock();
    await clickLoad(block);
    const box = block.querySelector<HTMLElement>(".usage-plan-result")!;
    expect(box.dataset.status).toBe("screen");
    expect(box.querySelector(".usage-plan-fail"), "空屏被当成失败了").toBeNull();
    expect(box.querySelector(".usage-screen-empty")!.textContent).toContain("空屏");
  });

  it("★ 生产路上**没有解析**了：屏上有百分比也不许被拆成 `.usage-plan-row`", async () => {
    outcome.value = { status: "screen", raw: "Current session\n  38% used\nResets in 2h" };
    const block = mountBlock();
    await clickLoad(block);
    expect(
      block.querySelector(".usage-plan-row"),
      "有人在展示这一层就地把那一屏解析了一遍 —— 那就是把退役掉的那层原地复活",
    ).toBeNull();
  });

  it("探测失败（captured=false）才是错误态，原因原样带给用户", async () => {
    outcome.value = { status: "probe-failed", error: "远端未安装 tmux" };
    const block = mountBlock();
    await clickLoad(block);
    expect(block.querySelector(".usage-plan-fail")!.textContent).toContain("远端未安装 tmux");
    expect(block.querySelector(".usage-screen-raw")).toBeNull();
  });

  it("★ 连读两次：只留最新那份（陈旧读数叠在上面比没有更糟）", async () => {
    // Phase G 审计逮到的：`renderPlanIdle` 是唯一做 replaceChildren 的地方，
    // 而它只在 build() 里跑过一次 ⇒ 结果只 append 不清。两块标签一模一样、
    // 都没有时间戳，而阅读顺序把**过期**那条排在最前面。
    const block = mountBlock();
    outcome.value = { status: "screen", raw: "SCREEN-FIRST" };
    await clickLoad(block);
    outcome.value = { status: "screen", raw: "SCREEN-SECOND" };
    await clickLoad(block);
    const results = block.querySelectorAll(".usage-plan-result");
    expect(results, "只该有一块结果").toHaveLength(1);
    expect(block.textContent).toContain("SCREEN-SECOND");
    expect(block.textContent, "过期那一屏必须消失").not.toContain("SCREEN-FIRST");
  });

  it("★ 失败之后再成功：上一次的失败消息不许留在屏上", async () => {
    const block = mountBlock();
    outcome.value = { status: "probe-failed", error: "ssh 挂了" };
    await clickLoad(block);
    expect(block.querySelector(".usage-plan-fail")).not.toBeNull();
    outcome.value = { status: "screen", raw: "SCREEN-AFTER-FAIL" };
    await clickLoad(block);
    expect(block.querySelector(".usage-plan-fail"), "旧失败要清掉").toBeNull();
    expect(block.querySelector(".usage-screen-raw")!.textContent).toBe("SCREEN-AFTER-FAIL");
  });
});
