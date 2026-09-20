/**
 * `设计/30 §C`（顺序落盘）的判据。
 *
 * # 它买什么
 *
 * `§C.1` 现打：`orderedIds` 的 8 个写入点**零持久化**，而集合落在 `config.json`
 * ⇒ 「你建的分组活过重启，你拖的顺序活不过」。`§C.2` 逐字**这是设计漏洞，不是未做的功能**。
 *
 * # 🔴 反空真：绿从哪来
 *
 * 每一格都是**相等断言**（对一个写出来的期望值），不是「没抛异常就绿」。
 * 最后那格 `⓪ 量具自检` 先证明「清洗真的在清」——否则下面几格可以靠一个
 * 恒等清洗函数全绿（那正是本仓反复治的形状）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../src/config", () => ({
  loadConfig: vi.fn(),
  saveConfig: vi.fn(),
}));

import { loadConfig, saveConfig } from "../src/config";
import { sanitizeOrder, getTabOrder, setTabOrder, ORDER_CAP } from "../src/tab-bar-state";

const mockLoad = vi.mocked(loadConfig);
const mockSave = vi.mocked(saveConfig);

describe("⓪ 量具自检：清洗真的在清（不过这格，下面全是空转）", () => {
  it("四种脏输入各被清掉一种，且**每一种单独可见**", () => {
    // 🔴 一条一条分开断言，不合成一个大用例：合起来写，只要有一种没清掉，
    //    另外三种也能让集合「看起来对」——分开才说得出是哪一种漏了。
    expect(sanitizeOrder(["a", "a", "b"], null), "重复的 sid 没去重 ⇒ 那个 tab 会渲两遍").toEqual(["a", "b"]);
    expect(sanitizeOrder(["a", 1, null, "b"], null), "非字符串没滤掉").toEqual(["a", "b"]);
    expect(sanitizeOrder(["  ", "a", " b "], null), "空白没 trim / 空串没滤掉").toEqual(["a", "b"]);
    expect(sanitizeOrder("不是数组", null), "非数组应当回空表，不是抛").toEqual([]);
  });

  it("`alive` 那一参真的在过滤，而且 `null` 真的不过滤", () => {
    expect(sanitizeOrder(["a", "b"], new Set(["a"])), "已不存在的 sid 没摘掉（`§C.3` 逐字）").toEqual(["a"]);
    expect(sanitizeOrder(["a", "b"], null), "写盘那侧传 null ⇒ 不许按存活过滤").toEqual(["a", "b"]);
  });

  it("上界是 `ORDER_CAP`，而且**恰好**在那里截断", () => {
    const many = Array.from({ length: ORDER_CAP + 5 }, (_, i) => `s${i}`);
    expect(sanitizeOrder(many, null).length, "上界没生效 ⇒ 一份被撑爆的配置能写进盘").toBe(ORDER_CAP);
    const exact = Array.from({ length: ORDER_CAP }, (_, i) => `s${i}`);
    expect(sanitizeOrder(exact, null).length, "恰好到上界时不许少截一个（差一错）").toBe(ORDER_CAP);
  });
});

describe("① 读：坏配置不许阻断启动（`§4` 逐字）", () => {
  beforeEach(() => vi.clearAllMocks());

  it("`loadConfig` 抛了 ⇒ 回空表，不往外抛", async () => {
    mockLoad.mockRejectedValueOnce(new Error("盘坏了"));
    await expect(getTabOrder(new Set(["a"]))).resolves.toEqual([]);
  });

  it("段不是对象（数组 / 字符串 / 缺失）⇒ 一律回空表", async () => {
    for (const seg of [["a"], "x", 3, null, undefined]) {
      mockLoad.mockResolvedValueOnce({ tabBar: seg } as never);
      await expect(getTabOrder(new Set(["a"]))).resolves.toEqual([]);
    }
  });

  it("读回来的顺序按**今天真的存在的 sid** 过滤", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: { order: ["a", "死了的", "b"] } } as never);
    await expect(getTabOrder(new Set(["a", "b"]))).resolves.toEqual(["a", "b"]);
  });
});

describe("② 写：只动自己那个键（`B` 落地时不许互相写没）", () => {
  beforeEach(() => vi.clearAllMocks());

  it("🔴 段里别的键原样留着 —— 这是 `§B`（pinned）能共用这个段的前提", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: { pinned: [{ sid: "p1" }], 别人的键: 1 } } as never);
    await setTabOrder(["a"]);
    const written = mockSave.mock.calls[0][0] as Record<string, unknown>;
    const seg = written.tabBar as Record<string, unknown>;
    expect(seg.order, "顺序没写进去").toEqual(["a"]);
    expect(seg.pinned, "🔴 把 `pinned` 写没了 —— `§B` 落地后两条路会互相清空对方").toEqual([{ sid: "p1" }]);
    expect(seg.别人的键, "段里不认识的键也不许动").toBe(1);
  });

  it("段原本不存在 / 不是对象 ⇒ 新建一个，不抛", async () => {
    for (const prev of [undefined, "坏", ["坏"]]) {
      vi.clearAllMocks();
      mockLoad.mockResolvedValueOnce({ tabBar: prev } as never);
      await setTabOrder(["a"]);
      const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<string, unknown>;
      expect(seg.order).toEqual(["a"]);
    }
  });

  it("`tabCollections` 一个字节不碰（`§4` 逐字「不动 tabCollections」）", async () => {
    mockLoad.mockResolvedValueOnce({ tabCollections: [{ id: "c1", name: "n", members: [] }] } as never);
    await setTabOrder(["a"]);
    const written = mockSave.mock.calls[0][0] as Record<string, unknown>;
    expect(written.tabCollections, "顺序落盘把集合改了 ⇒ `§4`「新的东西住新地方」破了").toEqual([
      { id: "c1", name: "n", members: [] },
    ]);
  });

  it("写进去的也过一遍清洗（脏值不许经这条路落盘）", async () => {
    mockLoad.mockResolvedValueOnce({} as never);
    await setTabOrder(["a", "a", "", "b"]);
    const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<string, unknown>;
    expect(seg.order).toEqual(["a", "b"]);
  });
});
