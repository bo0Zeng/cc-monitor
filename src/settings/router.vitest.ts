/**
 * S2：`SettingsRouter` 的行为测试（真 DOM，jsdom）。
 *
 * 路由器最核心的性质只有一条 —— **同一时刻只有一页可见**。它听起来废话，
 * 但写错的方向很具体：`hidden` 只设不清（切过去的页面永远藏着）、
 * 只设当前页不清旧页（越切越多页叠在一起）。下面逐条钉。
 */
import { describe, it, expect } from "vitest";
import { SettingsRouter } from "./router";

function page(text: string): HTMLElement {
  const el = document.createElement("div");
  el.textContent = text;
  return el;
}

/** 当前可见的页 id（按 DOM 顺序）。用 `data-route-id` 认页，不认文本。 */
function visibleIds(r: SettingsRouter): string[] {
  return [...r.element.querySelectorAll<HTMLElement>(".settings-page")]
    .filter((el) => !el.hidden)
    .map((el) => el.dataset.routeId ?? "");
}

function navButtons(r: SettingsRouter): HTMLButtonElement[] {
  return [...r.element.querySelectorAll<HTMLButtonElement>(".settings-nav-item")];
}

describe("SettingsRouter", () => {
  it("注册第一页就有东西可看（不会开局空白）", () => {
    const r = new SettingsRouter({ landingId: "machines" });
    r.addRoute({ id: "app", title: "应用", element: page("app") });
    expect(r.activeId).toBe("app");
    expect(visibleIds(r)).toEqual(["app"]);
  });

  it("★ 落地页后注册也会被切过去（注册顺序 ≠ 导航顺序）", () => {
    // 导航顺序按 §2.3 是 应用/机器/改动足迹，而落地页是「机器」——
    // 若实现成「谁先注册谁当家」，落地页这个设计决定就静默失效了。
    const r = new SettingsRouter({ landingId: "machines" });
    r.addRoute({ id: "app", title: "应用", element: page("app") });
    r.addRoute({ id: "machines", title: "机器", element: page("machines") });
    r.addRoute({ id: "footprint", title: "改动足迹", element: page("fp") });
    expect(r.activeId).toBe("machines");
    expect(visibleIds(r)).toEqual(["machines"]);
  });

  it("★ 任何时刻**恰好**一页可见（切来切去都不叠）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.addRoute({ id: "c", title: "C", element: page("c") });
    for (const id of ["b", "c", "a", "c", "b"]) {
      r.navigate(id);
      // 用「可见集合恰好等于 [id]」而不是「id 可见」——后者对「旧页没藏起来」是瞎的。
      expect(visibleIds(r)).toEqual([id]);
    }
  });

  it("点导航按钮 = 切页（不是只有 API 能切）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    navButtons(r)[1]!.click();
    expect(r.activeId).toBe("b");
    expect(visibleIds(r)).toEqual(["b"]);
  });

  it("当前页的导航项带 aria-selected + 活动样式类，其余不带", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.navigate("b");
    const [ba, bb] = navButtons(r);
    expect(bb!.getAttribute("aria-selected")).toBe("true");
    expect(ba!.getAttribute("aria-selected")).toBe("false");
    expect(bb!.classList.contains("settings-nav-item-active")).toBe(true);
    expect(ba!.classList.contains("settings-nav-item-active")).toBe(false);
    // 非当前项退出 Tab 序（tablist 惯例）
    expect(bb!.tabIndex).toBe(0);
    expect(ba!.tabIndex).toBe(-1);
  });

  it("navigate 到没注册的 id = no-op，不抛也不把当前页藏起来", () => {
    // 切页是 UI 动作，不该因为调用方拼错一个 id 就炸掉整个面板。
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    expect(() => r.navigate("不存在")).not.toThrow();
    expect(r.activeId).toBe("a");
    expect(visibleIds(r)).toEqual(["a"]);
  });

  it("重复注册同一个 id 直接抛（静默覆盖会让一整页凭空消失）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    expect(() => r.addRoute({ id: "a", title: "A2", element: page("a2") })).toThrow(
      /重复注册/,
    );
  });

  it("routeIds 按注册顺序，且与导航项一一对应", () => {
    const r = new SettingsRouter({ landingId: "x" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.addRoute({ id: "c", title: "C", element: page("c") });
    expect(r.routeIds).toEqual(["a", "b", "c"]);
    expect(navButtons(r).map((b) => b.textContent)).toEqual(["A", "B", "C"]);
  });

  it("落地页 id 压根没注册时，退回第一页而不是全空", () => {
    const r = new SettingsRouter({ landingId: "根本没有这一页" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    expect(r.activeId).toBe("a");
    expect(visibleIds(r)).toEqual(["a"]);
  });

  it("★ 方向键能在导航里走（配了 roving tabindex 就必须配方向键）", () => {
    // 非当前项 tabIndex=-1 ⇒ Tab 键到不了它们。若不实现方向键，那些页就
    // **键盘完全不可达** —— 这两件事必须成对，缺一个就是引入了 a11y 回归。
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.addRoute({ id: "c", title: "C", element: page("c") });
    const nav = r.element.querySelector<HTMLElement>(".settings-nav")!;
    const key = (k: string) =>
      nav.dispatchEvent(
        new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }),
      );

    key("ArrowDown");
    expect(r.activeId).toBe("b");
    key("ArrowDown");
    expect(r.activeId).toBe("c");
    key("ArrowDown"); // 循环回头
    expect(r.activeId).toBe("a");
    key("ArrowUp"); // 反向也循环
    expect(r.activeId).toBe("c");
    key("Home");
    expect(r.activeId).toBe("a");
    key("End");
    expect(r.activeId).toBe("c");
  });

  /**
   * ★★ E61：**生产里唯一真实的那个构型**——父 + **多个**子项 + 子项**后注册**。
   *
   * 既有三条测试互补但从不组合：扁平三页 / 只查 DOM 序 / 父 + **一个**子项。
   * 恰好漏掉这一个，而它就是设置面板每次打开时的样子（机器页是 `RemoteSection`
   * 异步 `refresh()` 之后才注册的 ⇒ 注册序 应用/机器/改动足迹/aya/nano，
   * 视觉序 应用/机器/aya/nano/改动足迹）。
   *
   * 症状：焦点在「机器」上按 ↓ 跳到「改动足迹」，`End` 落到最后一台机器。
   */
  it("★★ 方向键按**视觉序**走：父 + 多个后注册的子项（E61 的真实构型）", () => {
    const r = new SettingsRouter({ landingId: "app" });
    r.addRoute({ id: "app", title: "应用", element: page("app") });
    r.addRoute({ id: "machines", title: "机器", element: page("machines") });
    r.addRoute({ id: "footprint", title: "改动足迹", element: page("footprint") });
    // 机器页的子项**在 footprint 之后**注册（生产里就是这个时序）
    r.addRoute({ id: "aya", title: "aya", element: page("aya"), parentId: "machines" });
    r.addRoute({ id: "nano", title: "nano", element: page("nano"), parentId: "machines" });

    const nav = r.element.querySelector<HTMLElement>(".settings-nav")!;
    const domOrder = [...nav.querySelectorAll<HTMLElement>(".settings-nav-item")].map((b) =>
      b.id.replace("settings-tab-", ""),
    );
    // 先钉住前提：视觉序确实 ≠ 注册序，否则这条测试在测一个不存在的差异
    expect(domOrder).toEqual(["app", "machines", "aya", "nano", "footprint"]);
    expect(r.routeIds).toEqual(["app", "machines", "footprint", "aya", "nano"]);

    const key = (k: string) =>
      nav.dispatchEvent(
        new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }),
      );

    // 从「机器」往下：必须是 aya，而不是「改动足迹」
    r.navigate("machines");
    key("ArrowDown");
    expect(r.activeId, "↓ 应当走到视觉上的下一项 aya").toBe("aya");
    key("ArrowDown");
    expect(r.activeId).toBe("nano");
    key("ArrowDown");
    expect(r.activeId).toBe("footprint");
    // End 落到视觉最后一项（改动足迹），不是最后一台机器
    key("End");
    expect(r.activeId, "End 应当落到视觉最后一项").toBe("footprint");
    // 反向：从 footprint 往上回到 nano
    key("ArrowUp");
    expect(r.activeId).toBe("nano");
    key("Home");
    expect(r.activeId).toBe("app");
  });

  it("方向键切完焦点跟着走（否则按第二下会跳回原处）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    // jsdom 的 focus() 需要元素在文档里
    document.body.replaceChildren(r.element);
    const nav = r.element.querySelector<HTMLElement>(".settings-nav")!;
    nav.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    expect(document.activeElement).toBe(navButtons(r)[1]);
  });

  it("不认识的键**不**吞（否则 Tab / Esc 在导航上会失灵）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    const nav = r.element.querySelector<HTMLElement>(".settings-nav")!;
    const ev = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    nav.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(r.activeId).toBe("a");
  });

  it("页与导航项互相引用（tabpanel / aria-labelledby / aria-controls）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    const btn = navButtons(r)[0]!;
    const panel = r.element.querySelector<HTMLElement>(".settings-page")!;
    expect(panel.getAttribute("role")).toBe("tabpanel");
    expect(btn.getAttribute("aria-controls")).toBe(panel.id);
    expect(panel.getAttribute("aria-labelledby")).toBe(btn.id);
  });

  // ---- S4b：动态增删 + 一层子项 ----

  it("★ 子项紧跟父项之后，不跑到导航末尾", () => {
    // 若只 append，新机器会排在「改动足迹」后面，和它的父项「机器」隔开 ——
    // 导航就不再是一棵树，而是一堆平铺项。
    const r = new SettingsRouter({ landingId: "machines" });
    r.addRoute({ id: "app", title: "应用", element: page("app") });
    r.addRoute({ id: "machines", title: "机器", element: page("m") });
    r.addRoute({ id: "footprint", title: "改动足迹", element: page("fp") });
    r.addRoute({ id: "m:aya", title: "aya", element: page("aya"), parentId: "machines" });
    r.addRoute({ id: "m:nano", title: "nano", element: page("nano"), parentId: "machines" });
    expect(navButtons(r).map((b) => b.textContent)).toEqual([
      "应用",
      "机器",
      "aya",
      "nano",
      "改动足迹",
    ]);
  });

  it("子项带缩进样式类，父项不带", () => {
    const r = new SettingsRouter({ landingId: "m" });
    r.addRoute({ id: "m", title: "机器", element: page("m") });
    r.addRoute({ id: "m:aya", title: "aya", element: page("aya"), parentId: "m" });
    const [parent, child] = navButtons(r);
    expect(parent!.classList.contains("settings-nav-item-child")).toBe(false);
    expect(child!.classList.contains("settings-nav-item-child")).toBe(true);
  });

  it("removeRoute 把导航项和页面一起摘掉", () => {
    const r = new SettingsRouter({ landingId: "m" });
    r.addRoute({ id: "m", title: "机器", element: page("m") });
    r.addRoute({ id: "m:aya", title: "aya", element: page("aya"), parentId: "m" });
    r.removeRoute("m:aya");
    expect(r.routeIds).toEqual(["m"]);
    expect(navButtons(r).map((b) => b.textContent)).toEqual(["机器"]);
    expect(r.element.querySelectorAll(".settings-page")).toHaveLength(1);
  });

  it("★ 注销**当前页**时切到父页（否则用户停在一个已被摘掉的页上看空白）", () => {
    const r = new SettingsRouter({ landingId: "m" });
    r.addRoute({ id: "m", title: "机器", element: page("m") });
    r.addRoute({ id: "m:aya", title: "aya", element: page("aya"), parentId: "m" });
    r.navigate("m:aya");
    expect(r.activeId).toBe("m:aya");
    r.removeRoute("m:aya");
    expect(r.activeId).toBe("m");
    expect(visibleIds(r)).toEqual(["m"]);
  });

  it("注销的是当前页且它没有父 → 退到第一页，不留空白", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.navigate("b");
    r.removeRoute("b");
    expect(r.activeId).toBe("a");
    expect(visibleIds(r)).toEqual(["a"]);
  });

  it("注销**非**当前页不影响当前页", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.removeRoute("b");
    expect(r.activeId).toBe("a");
    expect(visibleIds(r)).toEqual(["a"]);
  });

  it("注销不存在的 id = no-op", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    expect(() => r.removeRoute("没有这一页")).not.toThrow();
    expect(r.routeIds).toEqual(["a"]);
  });

  it("注销后同名 id 可以重新注册（改名/重建机器页要靠这个）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.removeRoute("a");
    expect(() => r.addRoute({ id: "a", title: "A2", element: page("a2") })).not.toThrow();
    expect(r.routeIds).toEqual(["a"]);
  });

  it("方向键把子项也算进去（子项不能变成键盘到不了的死角）", () => {
    const r = new SettingsRouter({ landingId: "m" });
    r.addRoute({ id: "m", title: "机器", element: page("m") });
    r.addRoute({ id: "m:aya", title: "aya", element: page("aya"), parentId: "m" });
    const nav = r.element.querySelector<HTMLElement>(".settings-nav")!;
    nav.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    expect(r.activeId).toBe("m:aya");
  });

  it("★ onNavigate 只在真的换页时发（订阅者要做搬 DOM 这类有代价的事）", () => {
    const seen: string[] = [];
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.onNavigate((id) => seen.push(id));
    r.navigate("b");
    r.navigate("b");
    r.navigate("b");
    expect(seen).toEqual(["b"]);
    r.navigate("a");
    expect(seen).toEqual(["b", "a"]);
    // 点导航按钮走同一条路，也不该重复通知
    navButtons(r)[0]!.click();
    expect(seen).toEqual(["b", "a"]);
  });

  it("onNavigate 订阅者抛异常不影响切页本身（页面已经切了，只是附带的事没做成）", () => {
    const r = new SettingsRouter({ landingId: "a" });
    r.addRoute({ id: "a", title: "A", element: page("a") });
    r.addRoute({ id: "b", title: "B", element: page("b") });
    r.onNavigate(() => {
      throw new Error("BOOM");
    });
    expect(() => r.navigate("b")).not.toThrow();
    expect(r.activeId).toBe("b");
    expect(visibleIds(r)).toEqual(["b"]);
  });
});
