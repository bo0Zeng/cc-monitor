/**
 * 悬停提示（`kit/tooltip.ts`）：不许在 body 上越攒越多（E60，经设置里的 `?` 图标测）· 500ms 才出 · 同组立刻换 · 躲窗口边。
 *
 * E60：tooltip **不许在 body 上越攒越多**。
 *
 * 原来构造时就 `appendChild(document.body)`，而全文件没有任何回收路径。
 * `rebuildCards()` 每次重建全部 `MachineCard`、每开一次设置窗跑两遍，调用点已从 16 涨到 24。
 * `settings-ia/STATUS.md` 自己立过硬前置「**必须先于任何页面化**」，而页面化已经做完了 ——
 * **门是自己立的，越过去了，且没有任何门禁会红**。这个文件就是那道门禁。
 *
 * 修法不是补 `destroy()`（那要 24 个调用点每一个都记得调 —— 一个靠自觉维持的不变量
 * 迟早会破，而且破了照样没人知道），而是让 tooltip **只在显示期间存在**。
 */
import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { makeInfoIcon } from "../../../../src/frontend/ui/settings/info-icon";
import { adoptNativeTitles, attachTooltip, CARD_CLOSE_MS, delegateTooltip, hideTooltips, __liveTooltipCountForTests, TOOLTIP_DELAY_MS } from "../../../../src/frontend/ui/kit/tooltip";
import { closeMenu, openMenu } from "../../../../src/frontend/ui/kit/menu";
import { placeFloat } from "../../../../src/frontend/ui/kit/place";
import { button, setDisabled } from "../../../../src/frontend/ui/kit/button";

/** 悬停提示的两种摆法（`tooltip.ts` 的 PLACEMENT 表）：上方居中 · 卡式右侧顶对齐，间距 6。 */
const placeTip = (host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }) => placeFloat({ rect: host, side: "above", align: "center", gap: 6 }, tip, view);
const placeCardRight = (host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }) => placeFloat({ rect: host, side: "right", align: "start", gap: 6 }, tip, view);

const tipsInBody = () => document.querySelectorAll('[role="tooltip"]').length;
const hover = (el: HTMLElement) => el.dispatchEvent(new Event("mouseenter"));
const leave = (el: HTMLElement) => el.dispatchEvent(new Event("mouseleave"));

describe("E60：tooltip 不泄漏", () => {
  beforeEach(() => document.body.replaceChildren());

  it("★★ 造 24 个图标（= 今天的真实调用点数）而**一次都不悬停** → body 上零 tooltip", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    for (let i = 0; i < 24; i += 1) host.appendChild(makeInfoIcon(`说明 ${i}`));
    expect(tipsInBody(), "构造即 append 就是原来那条泄漏").toBe(0);
  });

  it("★★ 重建 100 次（模拟 rebuildCards）→ 仍然零残留", () => {
    for (let round = 0; round < 100; round += 1) {
      const host = document.createElement("div");
      document.body.appendChild(host);
      host.appendChild(makeInfoIcon("说明"));
      host.remove(); // rebuildCards 就是这么干的
    }
    expect(tipsInBody()).toBe(0);
    expect(__liveTooltipCountForTests()).toBe(0);
  });

  it("悬停时 tooltip 才出现，离开即从 DOM 摘掉（不是只 display:none）", () => {
    const icon = makeInfoIcon("这是说明");
    document.body.appendChild(icon);
    expect(tipsInBody()).toBe(0);

    hover(icon);
    expect(tipsInBody(), "悬停了却没显示 —— 功能坏了").toBe(1);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("这是说明");

    leave(icon);
    expect(tipsInBody(), "只 display:none 的话这里会是 1 —— 那正是原来的泄漏").toBe(0);
  });

  it("focus / blur 与鼠标同权（键盘可达性不能因为这次改动丢掉）", () => {
    const icon = makeInfoIcon("说明");
    document.body.appendChild(icon);
    icon.dispatchEvent(new Event("focusin"));
    expect(tipsInBody()).toBe(1);
    icon.dispatchEvent(new Event("focusout"));
    expect(tipsInBody()).toBe(0);
  });

  it("反复悬停同一个图标不会攒出多条", () => {
    const icon = makeInfoIcon("说明");
    document.body.appendChild(icon);
    for (let i = 0; i < 10; i += 1) {
      hover(icon);
      leave(icon);
    }
    hover(icon);
    expect(tipsInBody()).toBe(1);
  });

  /**
   * ★ 唯一一个 `hide` 兜不住的时序：**正显示着的时候图标被销毁**
   *（`rebuildCards()` 在鼠标悬停期间跑）—— 此时 `mouseleave` 永远不会来。
   * 由下一次显示前的 `sweepOrphanTooltips()` 清掉，残留上限恒为 1 条。
   */
  it("★ 悬停中被销毁 → 下一次悬停时把孤儿扫掉，残留不累积", () => {
    for (let round = 0; round < 5; round += 1) {
      const host = document.createElement("div");
      document.body.appendChild(host);
      const icon = makeInfoIcon(`说明 ${round}`);
      host.appendChild(icon);
      hover(icon); // 显示中
      host.remove(); // 主人没了，mouseleave 永不到来
      expect(tipsInBody(), "此刻确实残留一条（这是已知且有上限的）").toBe(1);

      // 下一轮的悬停会先扫
      const next = makeInfoIcon("下一个");
      document.body.appendChild(next);
      hover(next);
      expect(tipsInBody(), "孤儿没被扫掉 —— 残留会随重建次数累积").toBe(1);
      leave(next);
      next.remove();
    }
    expect(tipsInBody()).toBe(0);
  });

  it("aria-label 仍带全文（tooltip 不在 DOM 里时，读屏靠它）", () => {
    const icon = makeInfoIcon("多行\n说明");
    expect(icon.getAttribute("aria-label")).toBe("多行\n说明");
  });
});


describe("出现时机与摆法", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("悬停 500ms 才出；没到就离开 ⇒ 不出", () => {
    vi.advanceTimersByTime(1000); // 与上一条用例里最后收起的那条拉开（不算同一组）
    const a = document.createElement("button");
    document.body.appendChild(a);
    attachTooltip(a, "刷新");
    a.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1);
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(1);
    expect(tipsInBody()).toBe(1);
    a.dispatchEvent(new Event("mouseleave"));
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(1000); // 离开够久：不再算「同一组里移过去」
    a.dispatchEvent(new Event("mouseenter"));
    a.dispatchEvent(new Event("mouseleave"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS * 2);
    expect(tipsInBody(), "没等到就离开了还出了").toBe(0);
  });

  it("同一组里移到下一个：立刻换，不再等；内容在显示那一刻现取", () => {
    let chord = "Ctrl+R";
    const [a, b] = [document.createElement("button"), document.createElement("button")];
    document.body.append(a, b);
    attachTooltip(a, "刷新");
    attachTooltip(b, () => `重新连接 ${chord}`);
    a.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    a.dispatchEvent(new Event("mouseleave"));
    chord = "F5";
    b.dispatchEvent(new Event("mouseenter"));
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("重新连接 F5");
  });

  it("躲窗口边：上面放不下翻到下方；左右夹进视口 8px", () => {
    const host = { left: 2, top: 4, width: 20, height: 20, right: 22, bottom: 24 } as DOMRect;
    const at = placeTip(host, { width: 100, height: 30 }, { width: 300, height: 200 });
    expect(at.top).toBeGreaterThan(24);
    expect(at.left).toBe(8);
    const right = placeTip({ ...host, left: 290, right: 310 } as DOMRect, { width: 100, height: 30 }, { width: 300, height: 200 });
    expect(right.left).toBe(300 - 8 - 100);
  });

  it("卡式锚在宿主右侧、顶对齐；右边放不下翻到左侧；下面放不下 ⇒ 改底端对齐宿主", () => {
    const host = { left: 0, top: 100, width: 260, height: 30, right: 260, bottom: 130 } as DOMRect;
    expect(placeCardRight(host, { width: 300, height: 200 }, { width: 1280, height: 800 })).toEqual({ left: 266, top: 100 });
    expect(placeCardRight({ ...host, left: 1000, right: 1260 } as DOMRect, { width: 300, height: 200 }, { width: 1280, height: 800 }).left).toBe(1000 - 6 - 300);
    expect(placeCardRight({ ...host, top: 700, bottom: 730 } as DOMRect, { width: 300, height: 200 }, { width: 1280, height: 800 }).top).toBe(730 - 200);
  });

  it("★ 卡式（hold）：离开宿主不立刻关 —— 指针移进卡里留着；离开宿主与卡 120ms 才关", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const card = document.createElement("div");
    card.textContent = "卡";
    attachTooltip(host, () => card, { hold: true, placement: "right" });
    host.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const tip = document.querySelector<HTMLElement>('[role="tooltip"]')!;
    expect(tip).not.toBeNull();
    host.dispatchEvent(new Event("mouseleave"));
    tip.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(CARD_CLOSE_MS * 3);
    expect(tipsInBody(), "移进卡里：不收").toBe(1);
    tip.dispatchEvent(new Event("mouseleave"));
    vi.advanceTimersByTime(CARD_CLOSE_MS - 1);
    expect(tipsInBody()).toBe(1);
    vi.advanceTimersByTime(1);
    expect(tipsInBody()).toBe(0);
  });

  it("★ 委托式：一个容器四个监听器管它里面每一行；移到下一行卡跟着换；`content` 回 null 那一行不出", () => {
    const root = document.createElement("div");
    const rows = ["a", "b", "c"].map((t) => {
      const r = document.createElement("div");
      r.className = "row";
      r.textContent = t;
      root.appendChild(r);
      return r;
    });
    document.body.appendChild(root);
    delegateTooltip(root, ".row", (el) => (el.textContent === "c" ? null : `行 ${el.textContent}`));
    rows[0].dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("行 a");
    rows[0].dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: rows[1] }));
    rows[1].dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: rows[0] }));
    expect(document.querySelector('[role="tooltip"]')?.textContent, "同一组：立刻换").toBe("行 b");
    rows[1].dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: rows[2] }));
    rows[2].dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: rows[1] }));
    expect(tipsInBody()).toBe(0);
  });

  it("★ 键盘走行（↑↓ 换焦点）不逐行弹卡：每换一行重新等 500ms，停够了才出；指针那一组的「立刻换」不适用于焦点", () => {
    vi.advanceTimersByTime(1000);
    const root = document.createElement("div");
    const rows = ["a", "b", "c"].map((t) => {
      const r = document.createElement("div");
      r.className = "row";
      r.tabIndex = 0;
      r.textContent = t;
      root.appendChild(r);
      return r;
    });
    document.body.appendChild(root);
    delegateTooltip(root, ".row", (el) => `行 ${el.textContent}`, { hold: true, placement: "right" });
    const tip = () => document.querySelector('[role="tooltip"]')?.textContent ?? null;
    rows[0].focus();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tip(), "停在一行够 500ms ⇒ 出").toBe("行 a");
    rows[1].focus();
    expect(tip(), "走到下一行那一刻：上一张收、这一张不立刻出").toBeNull();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1);
    expect(tip()).toBeNull();
    rows[2].focus();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1);
    expect(tip(), "还在走：不出").toBeNull();
    vi.advanceTimersByTime(1);
    expect(tip(), "停够了才出").toBe("行 c");
    // 指针那一组照旧：从这张卡移到别的宿主立刻换。
    rows[2].blur();
    rows[0].dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect(tip(), "指针在同一组里移过去：立刻换").toBe("行 a");
    // 指针已在一行上、卡出着：点一下（焦点落到同一行）不收不重等。
    rows[0].focus();
    expect(tip()).toBe("行 a");
  });

  it("★ 单挂（attachTooltip）的宿主同理：焦点从一个移到下一个不立刻出", () => {
    vi.advanceTimersByTime(1000);
    const [a, b] = [document.createElement("button"), document.createElement("button")];
    document.body.append(a, b);
    attachTooltip(a, "甲");
    attachTooltip(b, "乙");
    a.focus();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody()).toBe(1);
    b.focus();
    expect(tipsInBody(), "焦点换过去那一刻不出").toBe(0);
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("乙");
  });

  it("★ 菜单弹出时收起悬停提示（不许压在菜单第一项上）", () => {
    const host = document.createElement("button");
    document.body.appendChild(host);
    attachTooltip(host, "提示");
    host.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody()).toBe(1);
    openMenu({ x: 1, y: 1 }, [{ label: "一项", onClick: () => {} }]);
    expect(tipsInBody()).toBe(0);
    closeMenu();
    hideTooltips(); // 空着时调也无事
  });

  it("★ 菜单开着时悬停卡不出：右键时卡还在等 500ms ⇒ 到点也不出；指针在宿主上再动也不出；菜单收了才照常", () => {
    const root = document.createElement("div");
    const row = document.createElement("div");
    row.className = "row";
    root.appendChild(row);
    document.body.appendChild(root);
    delegateTooltip(root, ".row", () => "卡", { placement: "right", hold: true });
    vi.advanceTimersByTime(1000);
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    // 还没到 500ms 就右键：菜单出来。
    openMenu({ x: 1, y: 1 }, [{ label: "一项", onClick: () => {} }]);
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody(), "菜单开着，等着的那张卡到点也不出").toBe(0);
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody(), "指针在宿主上再动也不出").toBe(0);
    closeMenu();
    vi.advanceTimersByTime(1000);
    row.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody(), "菜单收了 ⇒ 照常出").toBe(1);
    hideTooltips();
  });
});

describe("元素上的 title 改走 kit 的悬停提示（adoptNativeTitles）", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    hideTooltips();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("指针一进来 title 就挪走（系统提示不出），500ms 出 kit 那一条；离开即收；之后代码再写 title ⇒ 下一次照样挪", () => {
    adoptNativeTitles(document);
    vi.advanceTimersByTime(1000); // 出了「同一组」那段宽限（上一条测试刚收过提示）
    const row = document.createElement("div");
    row.title = "整理这周的笔记";
    const inner = document.createElement("span");
    row.appendChild(inner);
    document.body.appendChild(row);
    inner.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect(row.hasAttribute("title"), "title 还在 ⇒ 系统那种提示照样会出").toBe(false);
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const tip = document.querySelector<HTMLElement>('[role="tooltip"]');
    expect(tip?.textContent).toBe("整理这周的笔记");
    inner.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(1000);
    row.title = "改了名";
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("改了名");
    expect(row.hasAttribute("title")).toBe(false);
  });

  it("★ 按钮禁用时那句说明（悬停出着）：恢复可点那一刻收掉，之后再悬停也不再出那一句；原来的说明回来", () => {
    adoptNativeTitles(document);
    vi.advanceTimersByTime(1000);
    const plain = button({ label: "甲钮" });
    const hinted = button({ label: "看", hint: "乙钮的说明" });
    document.body.append(plain, hinted);
    for (const [b, back] of [[plain, null], [hinted, "乙钮的说明"]] as const) {
      setDisabled(b, "丙原因");
      b.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
      expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("丙原因");
      setDisabled(b, null);
      expect(tipsInBody(), "恢复可点 ⇒ 禁用那句当场收掉").toBe(0);
      b.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
      vi.advanceTimersByTime(1000);
      b.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
      expect(document.querySelector('[role="tooltip"]')?.textContent ?? null, "再悬停：原来的说明（没有就不出）").toBe(back);
      b.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
      vi.advanceTimersByTime(1000);
    }
  });

  it("三个窗口的入口都装它（entry-common 一处）", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/frontend/ui/entry-common.ts", "utf8");
    expect(src).toMatch(/^adoptNativeTitles\(\);$/m);
  });
});

describe("接管 title 不丢读屏名、全文已显示就不重复出", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    hideTooltips();
    vi.useFakeTimers();
    vi.advanceTimersByTime(1000);
  });
  afterEach(() => vi.useRealTimers());

  /** 可访问名（accname 1.2 里与 title 相关的那几步）：labelledby → aria-label → 名字来自内容的角色取文字 → title。 */
  const NAME_FROM_CONTENT = new Set(["BUTTON", "A", "SUMMARY", "OPTION"]);
  const ROLES_FROM_CONTENT = new Set(["button", "link", "menuitem", "menuitemradio", "menuitemcheckbox", "option", "tab", "tooltip", "cell", "row"]);
  function accName(el: HTMLElement): string {
    const by = el.getAttribute("aria-labelledby");
    if (by) return by.split(/\s+/).map((id) => document.getElementById(id)?.textContent?.trim() ?? "").join(" ");
    const label = el.getAttribute("aria-label");
    if (label?.trim()) return label.trim();
    const role = el.getAttribute("role");
    const text = (el.textContent ?? "").trim();
    if ((NAME_FROM_CONTENT.has(el.tagName) || (role !== null && ROLES_FROM_CONTENT.has(role))) && text) return text;
    return el.getAttribute("title")?.trim() ?? "";
  }

  it("★ 每个可聚焦元素：接管前后可访问名不变（图标按钮靠 title 当名字的，挪走前写进 aria-label）", () => {
    adoptNativeTitles(document);
    const mk = (html: string): HTMLElement => {
      const w = document.createElement("div");
      w.innerHTML = html;
      document.body.appendChild(w);
      return w.firstElementChild as HTMLElement;
    };
    const els = [
      mk(`<button title="历史会话浏览器"><svg></svg></button>`),
      mk(`<button title="恢复 · 不用 tmux">恢复</button>`),
      mk(`<div tabindex="0" title="整理这周的笔记"><span>整理这周的笔记</span><span>notes</span></div>`),
      mk(`<a href="#" aria-label="打开" title="在浏览器里打开">↗</a>`),
      mk(`<span id="lbl">设置</span>`),
      mk(`<button aria-labelledby="lbl" title="打开设置"></button>`),
      mk(`<div role="tab" tabindex="0" title="终端">终端</div>`),
    ];
    const focusable = els.filter((e) => e.matches("button, a[href], [tabindex]"));
    const before = focusable.map(accName);
    for (const el of focusable) el.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    expect(focusable.every((e) => !e.hasAttribute("title")), "title 没挪走").toBe(true);
    expect(focusable.map(accName)).toEqual(before);
  });

  /** 可访问说明：`aria-description` → title（没被拿去当名字时）。 */
  function accDesc(el: HTMLElement): string {
    const d = el.getAttribute("aria-description");
    if (d !== null) return d.trim();
    const t = el.getAttribute("title")?.trim() ?? "";
    return t !== "" && accName(el) !== t ? t : "";
  }

  /** 等接管那一圈（观察者回调 · 它自己挪走 title 引出的那一圈）都跑完。 */
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 4; i++) await Promise.resolve();
  };

  it("★ 不等悬停：元素一挂上就接管 —— 已有可访问名的不覆盖、名字与说明不相同才挂说明；没名字的才拿说明当名字", async () => {
    adoptNativeTitles(document);
    const mk = (html: string): HTMLElement => {
      const w = document.createElement("div");
      w.innerHTML = html;
      document.body.appendChild(w);
      return w.firstElementChild as HTMLElement;
    };
    const cases: [HTMLElement, string, string][] = [
      // 头上那枚标签（不可聚焦、有字）：名字是它的字，原因是说明。
      [mk(`<span title="甲机的 tmux 低于 3.2">仅快照甲</span>`), "", "甲机的 tmux 低于 3.2"],
      // 字与说明一样：不再挂说明（读屏不念两遍）。
      [mk(`<button title="收起甲">收起甲</button>`), "收起甲", ""],
      [mk(`<button title="看它的终端甲">看甲</button>`), "看甲", "看它的终端甲"],
      [mk(`<button aria-label="设置甲" title="设置甲"><svg></svg></button>`), "设置甲", ""],
      [mk(`<button aria-label="设置乙" title="打开设置乙"><svg></svg></button>`), "设置乙", "打开设置乙"],
      [mk(`<textarea aria-label="回车送出甲" title="回车送出甲"></textarea>`), "回车送出甲", ""],
      // 只有 title 能当名字的（图标按钮 · 可聚焦的一行）：说明挪成名字，不再另挂说明。
      [mk(`<button title="历史甲"><svg></svg></button>`), "历史甲", ""],
      [mk(`<div tabindex="0" title="整理笔记甲"><span>整理笔记甲</span><span>notes</span></div>`), "整理笔记甲", ""],
    ];
    await Promise.resolve();
    for (const [el, name, desc] of cases) {
      expect(el.hasAttribute("title"), `${el.outerHTML}：title 还在 ⇒ 系统提示照出、读屏照旧念`).toBe(false);
      expect([accName(el), accDesc(el)], el.outerHTML).toEqual([name, desc]);
    }
  });

  it("★ 代码后来改 title（按钮禁用 / 恢复）：说明跟着换；恢复后没有原说明 ⇒ 说明撤掉", async () => {
    adoptNativeTitles(document);
    const plain = button({ label: "甲钮" });
    const hinted = button({ label: "乙", hint: "乙钮的说明" });
    document.body.append(plain, hinted);
    await settle();
    expect([accName(hinted), accDesc(hinted)]).toEqual(["乙", "乙钮的说明"]);
    for (const b of [plain, hinted]) setDisabled(b, "丙原因");
    await settle();
    expect([accDesc(plain), accDesc(hinted)]).toEqual(["丙原因", "丙原因"]);
    for (const b of [plain, hinted]) setDisabled(b, null);
    await settle();
    expect([accName(plain), accDesc(plain)]).toEqual(["甲钮", ""]);
    expect([accName(hinted), accDesc(hinted)]).toEqual(["乙", "乙钮的说明"]);
  });

  it("全文已经完整显示（没被截断）⇒ 不再弹一遍；截断了 ⇒ 照出", () => {
    adoptNativeTitles(document);
    const row = document.createElement("div");
    row.title = "整理这周的笔记";
    const name = document.createElement("span");
    name.textContent = "整理这周的笔记";
    row.appendChild(name);
    document.body.appendChild(row);
    name.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(tipsInBody(), "全文就在眼前，又弹一遍").toBe(0);
    name.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
    vi.advanceTimersByTime(1000);
    Object.defineProperty(name, "scrollWidth", { configurable: true, value: 300 });
    Object.defineProperty(name, "clientWidth", { configurable: true, value: 120 });
    name.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("整理这周的笔记");
  });
});
