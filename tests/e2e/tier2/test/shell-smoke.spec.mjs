// F-E5a —— 裸壳 DOM 冒烟（无 fixture）。
//
// 经 session-1 hop 驱真 WebView2，用 WebDriver 断言 cc-monitor 的裸壳（无会话时）：
//   1. 壳元素存在：#app / #tab-bar / #message-stream / #status-bar
//   2. 无会话时：状态栏里没有这个会话的几枚（任务 · agent · 上下文），.empty-state 可见含「暂无活跃会话」
//   3. 栏顶每颗入口按钮（照页面取）+ .status-cmdk 存在且可点（isClickable，不实际点——避免开窗/弹层副作用）
//   4. overlay 快捷键（物理码，dispatcher 按 KeyboardEvent.code 归一）：
//        KeyH → 历史 overlay 出现；Escape → 关；
//        Ctrl+KeyK → 命令栏出现；Escape → 关。
//
// 键盘用 browser.keys（真 WebDriver Actions），比 execute 合成事件更强的活证。
// 单键快捷键在可编辑焦点里被 dispatcher 抑制（registry.ts isEditableTarget），
// 故每个 overlay 测试结束都归位（Esc + blur），保证下一个单键能触发。
import { browser, $, $$, expect } from "@wdio/globals";
import { Key } from "webdriverio";

const SHELL = ["#app", "#tab-bar", "#message-stream", "#status-bar"];
// 栏顶那排全局入口不在这里手写：跑的那台没有仓、读不到 `main.ts`，手写的清单腐过一次
// （入口删了、表里还列着 ⇒ 必然失败的断言）。照页面取：栏顶入口区里的每颗图标按钮。
const TOPBAR_BUTTONS = ".tab-bar-head-acts .tab-bar-icon";
const CMDK = ".status-cmdk";

// 把所有 overlay 关掉并释放焦点，隔离各快捷键用例（Esc 关栈顶；blur 解除可编辑焦点抑制）。
async function resetOverlays() {
  for (let i = 0; i < 3; i++) {
    await browser.keys([Key.Escape]);
    await browser.pause(120);
  }
  await browser.execute(() => {
    const el = document.activeElement;
    if (el && typeof el.blur === "function") el.blur();
  });
  await browser.pause(120);
}

describe("F-E5a cc-monitor 裸壳 DOM 冒烟", () => {
  before(async () => {
    // 等 DOM 渲染完
    await browser.waitUntil(
      async () => (await browser.execute(() => document.readyState)) === "complete",
      { timeout: 30000, timeoutMsg: "document 未到 complete" },
    );
    // 桩掉 window.confirm/prompt（本套件不做破坏性动作，纯防御——若某点击意外触发也不阻塞）
    await browser.execute(() => {
      window.confirm = () => true;
      window.prompt = () => "";
    });
    // 壳元素出现即可开测
    await $("#app").waitForExist({ timeout: 30000 });
    await $(".status-bar, #status-bar").waitForExist({ timeout: 30000 });
    const title = await browser.getTitle();
    console.log("[E5a] title =", JSON.stringify(title));
  });

  afterEach(async () => {
    await resetOverlays();
  });

  it("1) 壳元素存在", async () => {
    for (const sel of SHELL) {
      const exists = await $(sel).isExisting();
      console.log(`[E5a] shell ${sel} exists=${exists}`);
      expect(exists).toBe(true);
    }
  });

  it("2) 无会话：状态栏不出这个会话的几枚 · 暂无活跃会话", async () => {
    const tasksShown = await $(".status-tasks").isDisplayed();
    const agentsShown = await $(".status-agents").isDisplayed();
    const empty = $(".empty-state");
    const emptyVisible = await empty.isDisplayed();
    const emptyText = await empty.getText();
    console.log("[E5a] tasks chip shown=", tasksShown, "agents chip shown=", agentsShown);
    console.log("[E5a] empty-state visible=", emptyVisible, "text=", JSON.stringify(emptyText));
    expect(tasksShown).toBe(false);
    expect(agentsShown).toBe(false);
    expect(emptyVisible).toBe(true);
    expect(emptyText).toContain("暂无活跃会话");
  });

  it("3) 栏顶每颗入口按钮 + status-cmdk 存在且可点", async () => {
    const buttons = await $$(TOPBAR_BUTTONS);
    console.log(`[E5a] topbar buttons=${buttons.length}`);
    // 取不到一颗 ⇒ 栏顶搬家或没画出来，不许零命中地绿。
    expect(buttons.length).toBeGreaterThan(0);
    for (const el of [...buttons, $(CMDK)]) {
      const exists = await el.isExisting();
      const clickable = exists ? await el.isClickable() : false;
      console.log(`[E5a] topbar ${await el.getAttribute("class")} exists=${exists} clickable=${clickable}`);
      expect(exists).toBe(true);
      expect(clickable).toBe(true);
    }
  });

  it("4a) KeyH 开历史 overlay，Escape 关", async () => {
    expect(await $(".history-view").isExisting()).toBe(false);
    await browser.keys(["h"]);
    await $(".history-view").waitForExist({ timeout: 8000, timeoutMsg: "KeyH 未开历史" });
    console.log("[E5a] KeyH → .history-view existing =", await $(".history-view").isExisting());
    await browser.keys([Key.Escape]);
    await $(".history-view").waitForExist({ reverse: true, timeout: 8000, timeoutMsg: "Escape 未关历史" });
    console.log("[E5a] Escape → .history-view existing =", await $(".history-view").isExisting());
    expect(await $(".history-view").isExisting()).toBe(false);
  });

  it("4c) Ctrl+KeyK 开命令栏，Escape 关", async () => {
    expect(await $(".command-bar").isExisting()).toBe(false);
    await browser.keys([Key.Ctrl, "k"]);
    await $(".command-bar").waitForExist({ timeout: 8000, timeoutMsg: "Ctrl+K 未开命令栏" });
    console.log("[E5a] Ctrl+KeyK → .command-bar existing =", await $(".command-bar").isExisting());
    await browser.keys([Key.Escape]);
    await $(".command-bar").waitForExist({ reverse: true, timeout: 8000, timeoutMsg: "Escape 未关命令栏" });
    console.log("[E5a] Escape → .command-bar existing =", await $(".command-bar").isExisting());
    expect(await $(".command-bar").isExisting()).toBe(false);
  });
});
