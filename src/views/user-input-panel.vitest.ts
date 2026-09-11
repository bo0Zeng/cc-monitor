/**
 * K-R45 第三轮：**「跳成功了就把标记去掉」这一半的判据。**
 *
 * # 这个文件为什么存在（病史，不是背景故事）
 *
 * PM 09-10 第二拍切刀实测：把 `jumpToUserInput` 里的 `delete row.dataset.unjumpable;`
 * **整句删掉**，跑全量门禁 ⇒ **`GATE: OK`，11 格无一红**。
 * ⇒「跳空了标出来」有判据，「跳成功了把标记去掉」**零判据**。
 * 而删掉之后的行为是：**一行只要跳空过一次就永远灰着** —— 正是上一轮声称自己修掉的
 * 那个「只加不减」，只是从 `style.opacity` 搬到了 `dataset` 上。
 *
 * 本轮同时逮到**同一形的第二处**：撤标记时 `title` **根本没有撤的代码** ——
 * 跳成功了，「跳不过去」那句提示还挂在那一行上。所以下面那一格断的是**两样**。
 *
 * # 🔴 可达性：这条转移**真的会发生**，而它住在实时窗口，不住在查看器
 *
 * PM 给的可达性理由是「查看器分批渲染，T 时刻没渲染、T+1 渲染出来」。
 * 实现方现打核过**这句话对查看器不成立**（读数与推理在件 `§5.5`）：
 * `scrollToMessage` 在跳之前会**先把目标那一段渲出来**（`uuidToIdx` + `unrendered.contains`），
 * 所以查看器里一行会被标上 `unjumpable`，当且仅当那条记录**渲染出来就是空的**
 * （`stripInternalNoise` 剥空 ⇒ 不建卡）—— 而那是**永久**的，重渲一次还是空。
 *
 * **转移的真住址是实时窗口**：那里的记录可以「现在没渲染（收纳在 `TailWindow` 里）、
 * 上翻补一批之后就有卡了」，两次点击之间状态真的会翻。
 * ⇒ 本文件断的是**面板这一层的契约**（宿主说落到了 ⇒ 两样标记都撤），
 *   活体读数（真会发生的那条转移，走真 `TabManager` + 真渲染管线）在
 *   `live-user-inputs.vitest.ts`「上翻补批之后再点」那一格。**两格是一对，各买各的。**
 *
 * # 台子
 *
 * 这一层**不需要**渲染管线：面板对「跳到哪儿」一无所知，它只认宿主的返回值
 * （`jumpTo` 返回那张卡 / `null`）。⇒ 宿主用 `vi.fn()`，**这不是把被测对象 mock 掉**，
 * 宿主本来就是注入的接缝。两条真路各自的接线由各自的套件盯着。
 */
import { describe, it, expect, vi, beforeEach, type Mock } from "vitest";
import { UserInputPanel } from "./user-input-panel";
import type { UserInputEntry } from "./user-input-index";

const HINT = "这一条还没加载出来 —— 上翻到更早的消息之后再点";

const entry = (uuid: string, excerpt: string, i = 0): UserInputEntry => ({
  uuid,
  excerpt,
  timestamp: "2026-09-10T00:00:00.000Z",
  payloadIndex: i,
});

interface Fixture {
  panel: UserInputPanel;
  jumpTo: Mock<(uuid: string) => HTMLElement | null>;
  rows(): HTMLButtonElement[];
}

function fixture(): Fixture {
  const jumpTo = vi.fn<(uuid: string) => HTMLElement | null>(() => null);
  const panel = new UserInputPanel({ jumpTo, unjumpableHint: HINT });
  document.body.replaceChildren(panel.toggle, panel.panel);
  return {
    panel,
    jumpTo,
    rows: () => [...panel.panel.querySelectorAll<HTMLButtonElement>(".user-input-row")],
  };
}

let f: Fixture;
beforeEach(() => {
  f = fixture();
});

describe("K-R45 清单面板：跳空了标出来", () => {
  it("跳空 ⇒ 标记 + 宿主给的那句人话（不许静默做一个没用的动作）", () => {
    f.panel.setEntries([entry("u1", "第一句")]);
    const row = f.rows()[0];
    expect(row.dataset.unjumpable).toBeUndefined(); // 没点之前不许预先灰着
    row.click();
    expect(f.jumpTo).toHaveBeenCalledWith("u1");
    expect(row.dataset.unjumpable).toBe("1");
    expect(row.title).toBe(HINT); // 那句话由宿主给：两条路落空的成因不是同一件事
  });
});

describe("K-R45 清单面板：🔴 跳成功了，标记与提示都要跟着撤", () => {
  // ★★ 这一格就是 PM 那一刀逮到的空档。它断的是**两样**：
  //    `data-unjumpable`（变灰的钩子）与 `title`（那句人话）。
  //    只撤一样 = 把「只加不减」从一个字段搬到另一个字段，形状一个字没变。
  it("同一行：先跳空（标上），后来跳得过去了 ⇒ 标记删掉、提示还原成摘要", () => {
    f.panel.setEntries([entry("u1", "第一句")]);
    const row = f.rows()[0];

    // T 时刻：宿主说落空
    row.click();
    expect(row.dataset.unjumpable, "先得真的标上，不然下面那半是空真").toBe("1");
    expect(row.title).toBe(HINT);

    // T+1：同一行、同一个 uuid，这次宿主说落到卡上了
    const card = document.createElement("div");
    f.jumpTo.mockReturnValue(card);
    row.click();

    expect(row.dataset.unjumpable, "跳得过去了还灰着 ⇒ 一行只要跳空过一次就永远灰着").toBeUndefined();
    expect(row.title, "跳得过去了还挂着「跳不过去」那句 ⇒ 同一形的第二处").toBe("第一句");
    expect(f.jumpTo).toHaveBeenCalledTimes(2);
  });

  // 上一格的两条断言里，第二条（title）在第一条之后 —— 断言会短路，所以单独证它**真执行到了**：
  // 这一格只看 title，第一条断言换成必然成立的那个。
  it("撤标记那一支里 title 这一半单独有牙（证上一格第二条断言不是摆设）", () => {
    f.panel.setEntries([entry("u1", "第一句")]);
    const row = f.rows()[0];
    row.click();
    f.jumpTo.mockReturnValue(document.createElement("div"));
    row.click();
    expect(row.title).toBe("第一句");
  });

  it("从没跳空过的一行，跳成功之后也不许莫名其妙多出标记", () => {
    f.jumpTo.mockReturnValue(document.createElement("div"));
    f.panel.setEntries([entry("u1", "第一句")]);
    const row = f.rows()[0];
    row.click();
    expect(row.dataset.unjumpable).toBeUndefined();
    expect(row.title).toBe("第一句");
  });
});

describe("K-R45 清单面板：换一份清单时，已经标上的那一行不许被顺手抹掉", () => {
  // ★ 实时那条路每来一句用户输入就调一次 `setEntries`。整表重建会把标记连带清掉 ——
  //   那是「只加不减」的**镜像形**：一次**假的「减」**（这一行明明还是跳不过去的）。
  it("追加了新的一句 ⇒ 旧行还是同一个 DOM 节点，标记与提示都还在", () => {
    f.panel.setEntries([entry("u1", "第一句", 0)]);
    const first = f.rows()[0];
    first.click(); // 落空 ⇒ 标上
    expect(first.dataset.unjumpable).toBe("1");

    f.panel.setEntries([entry("u1", "第一句", 0), entry("u2", "第二句", 1)]);

    expect(f.rows().length).toBe(2);
    expect(f.rows()[0], "旧行被整表重建换成了新节点 ⇒ 标记必然丢").toBe(first);
    expect(f.rows()[0].dataset.unjumpable).toBe("1");
    expect(f.rows()[0].title).toBe(HINT);
    expect(f.rows()[1].textContent).toBe("2. 第二句");
  });

  it("换成完全不同的一份清单 ⇒ 旧行不许留下（序号与 uuid 都要跟着换）", () => {
    f.panel.setEntries([entry("u1", "第一句"), entry("u2", "第二句")]);
    f.panel.setEntries([entry("n1", "新会话唯一一句")]);
    expect(f.rows().map((r) => r.dataset.inputUuid)).toEqual(["n1"]);
    expect(f.rows()[0].textContent).toBe("1. 新会话唯一一句");
  });
});

describe("K-R45 清单面板：开关与收起", () => {
  it("一条都没有 ⇒ 开关禁用、清单为空（不给一个点了没反应的入口）", () => {
    f.panel.setEntries([]);
    expect(f.rows().length).toBe(0);
    expect(f.panel.toggle.disabled).toBe(true);
    expect(f.panel.toggle.textContent).toBe("我说过的 0 句");
  });

  it("面板默认收着，点开关才展开（默认收着 ⇒ 对宿主既有布局零影响）", () => {
    f.panel.setEntries([entry("u1", "第一句")]);
    expect(f.panel.panel.hidden).toBe(true);
    expect(f.panel.toggle.getAttribute("aria-expanded")).toBe("false");
    f.panel.toggle.click();
    expect(f.panel.panel.hidden).toBe(false);
    expect(f.panel.toggle.getAttribute("aria-expanded")).toBe("true");
  });

  it("clear() ⇒ 行清空、面板收起、开关回默认（换会话 / 关 tab 都走它）", () => {
    f.panel.setEntries([entry("u1", "第一句")]);
    f.panel.toggle.click();
    expect(f.panel.panel.hidden).toBe(false);

    f.panel.clear();

    expect(f.rows().length).toBe(0);
    expect(f.panel.panel.hidden).toBe(true);
    expect(f.panel.toggle.disabled).toBe(true);
    expect(f.panel.toggle.textContent).toBe("我说过的 0 句");
    expect(f.panel.toggle.getAttribute("aria-expanded")).toBe("false");
  });
});
