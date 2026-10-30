/**
 * 落盘契约（`tabBar` 段）的判据 —— 这扇窗自己的看法：顺序 · 收着的组 · 「已隐藏」那一行收展，几个键住同一个段，所以判据也住同一份。
 * 组与固定是后端的数据（`group-book.ts`），不在这个段里。
 *
 * # 它买什么
 *
 * `§C.1` 现打：`orderedIds` 的 8 个写入点**零持久化**，而集合落在 `config.json`
 * ⇒ 「你建的分组活过重启，你拖的顺序活不过」。`§C.2` 逐字**这是设计漏洞，不是未做的功能**。
 * `§B.1` 现打：全仓 `pinned`/`isPinned`/`pinTab` **零命中** —— 固定这件事今天完全不存在。
 *
 * # 🔴 反空真：绿从哪来
 *
 * 每一格都是**相等断言**（对一个写出来的期望值），不是「没抛异常就绿」。
 * 两个 `量具自检` 组先证明「清洗真的在清」——否则下面几格可以靠一个
 * 恒等清洗函数全绿（那正是本仓反复治的形状）。
 *
 * # 🔴 那条承重不变量：**不写没别人的键**
 *
 * 几个键住同一段。任何一侧整段覆盖 ⇒ 别的当场被清空，而且是**静默**的（下次启动才看得见）。
 * `②` 钉「交出去的补丁只有自己那一格」，`⑤` 几条路轮流写、每一趟都只交自己那一格，写的那一格就是读的那一格。
 * 补丁落到盘上段里别的键留不留，是 Rust 写口的事（金样 `config-patch.golden.json`，`config_tests.rs` 判）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// config 写只交补丁；替身记下交出去的补丁（`sentEdits()`），不应用。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));

import { loadConfig } from "../../../src/frontend/ui/config";
import { sentEdits } from "./config-patch-fake";
import {
  sanitizeOrder,
  getTabOrder,
  setTabOrder,
  ORDER_CAP,
  sanitizeCollapsed,
  getCollapsed,
  setCollapsed,
  setHiddenOpen,
  COLLAPSED_CAP,
} from "../../../src/frontend/ui/tab-bar-state";

const mockLoad = vi.mocked(loadConfig);

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

  it("🔴 只交 `tabBar.order` 那一格 —— 段里别的键 · 段外的键都不在补丁里（这是几个键能共用这个段的前提）", async () => {
    await setTabOrder(["a"]);
    expect(sentEdits(), "🔴 补丁带上了别的键 ⇒ 两条路会互相写没对方").toEqual([{ op: "set", path: ["tabBar", "order"], value: ["a"] }]);
  });

  it("写进去的也过一遍清洗（脏值不许经这条路落盘）", async () => {
    await setTabOrder(["a", "a", "", "b"]);
    expect(sentEdits()).toEqual([{ op: "set", path: ["tabBar", "order"], value: ["a", "b"] }]);
  });
});

// ============================ 收着的组（`tabBar.collapsed`）============================

describe("③ 量具自检：`sanitizeCollapsed` 真的在清", () => {
  it("非字符串 · 空白 · 重复各被清掉一种，且上界恰好在 `COLLAPSED_CAP`", () => {
    expect(sanitizeCollapsed(["g1", 7, "  ", "g1", " g2 "])).toEqual(["g1", "g2"]);
    expect(sanitizeCollapsed("不是数组")).toEqual([]);
    const many = Array.from({ length: COLLAPSED_CAP + 5 }, (_, i) => `g${i}`);
    expect(sanitizeCollapsed(many)).toEqual(many.slice(0, COLLAPSED_CAP));
  });
});

describe("④ 读收着的组：坏配置不许阻断启动", () => {
  beforeEach(() => vi.clearAllMocks());
  it("`loadConfig` 抛了 ⇒ 回空表，不往外抛", async () => {
    mockLoad.mockRejectedValueOnce(new Error("坏了"));
    await expect(getCollapsed()).resolves.toEqual([]);
  });
  it("段不是对象 ⇒ 空表；读回来的照清洗", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: "坏" } as never);
    await expect(getCollapsed()).resolves.toEqual([]);
    mockLoad.mockResolvedValueOnce({ tabBar: { collapsed: ["g1", "g1", 3] } } as never);
    await expect(getCollapsed()).resolves.toEqual(["g1"]);
  });
});

describe("⑤ 🔴 几条路轮流写 —— 每一趟都只交自己那一格，谁都不许把对方写没", () => {
  // 让几条路**轮流真跑**，中间不重置 —— 「整段覆盖」那种写法在这里会当场露馅（补丁成了整段 `tabBar` ⇒ 这一格红）。
  beforeEach(() => vi.clearAllMocks());

  it("order → collapsed → hiddenOpen → order，每一趟一格，值逐字对得上", async () => {
    await setTabOrder(["a", "b"]);
    await setCollapsed(["g1"]);
    await setHiddenOpen(true);
    await setTabOrder(["b", "a"]);
    expect(sentEdits(), "🔴 有一趟交了整段 ⇒ 别的键被冲掉").toEqual([
      { op: "set", path: ["tabBar", "order"], value: ["a", "b"] },
      { op: "set", path: ["tabBar", "collapsed"], value: ["g1"] },
      { op: "set", path: ["tabBar", "hiddenOpen"], value: true },
      { op: "set", path: ["tabBar", "order"], value: ["b", "a"] },
    ]);
  });

  it("写的那一格就是读的那一格：照补丁的路径摆回盘上，读回来是同一份", async () => {
    await setTabOrder(["x", "y"]);
    await setCollapsed(["g2", "g3"]);
    expect(sentEdits().map((e) => e.path)).toEqual([["tabBar", "order"], ["tabBar", "collapsed"]]);
    mockLoad.mockResolvedValue({ tabBar: { order: ["x", "y"], collapsed: ["g2", "g3"] } } as never);
    await expect(getTabOrder(new Set(["x", "y"]))).resolves.toEqual(["x", "y"]);
    await expect(getCollapsed()).resolves.toEqual(["g2", "g3"]);
  });
});
