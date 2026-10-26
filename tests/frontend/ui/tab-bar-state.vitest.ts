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
 * `②` 钉「写的时候有没有带上对方」，`⑤` 用一份**真的在内存里的 config** 走一遍几条路轮流写。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`
// （补丁语义与 Rust 写口同一份金样，见 `tests/frontend/ui/config-patch-fake.ts`）。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));

import { loadConfig } from "../../../src/frontend/ui/config";
import { fakeCfg } from "./config-patch-fake";
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
const mockSave = fakeCfg.saved;

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

  it("🔴 段里别的键原样留着 —— 这是几个键能共用这个段的前提", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: { collapsed: ["g1"], 别人的键: 1 } } as never);
    await setTabOrder(["a"]);
    const written = mockSave.mock.calls[0][0] as Record<string, unknown>;
    const seg = written.tabBar as Record<string, unknown>;
    expect(seg.order, "顺序没写进去").toEqual(["a"]);
    expect(seg.collapsed, "🔴 把收着的组写没了 —— 两条路会互相清空对方").toEqual(["g1"]);
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

  it("段外的顶层键一个字节不碰", async () => {
    mockLoad.mockResolvedValueOnce({ sessionHead: { buttons: ["x"] } } as never);
    await setTabOrder(["a"]);
    const written = mockSave.mock.calls[0][0] as Record<string, unknown>;
    expect(written.sessionHead, "顺序落盘把别的顶层键改了").toEqual({ buttons: ["x"] });
  });

  it("写进去的也过一遍清洗（脏值不许经这条路落盘）", async () => {
    mockLoad.mockResolvedValueOnce({} as never);
    await setTabOrder(["a", "a", "", "b"]);
    const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<string, unknown>;
    expect(seg.order).toEqual(["a", "b"]);
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

describe("⑤ 🔴 几条路交替写一份**真的在内存里**的 config —— 谁都不许把对方写没", () => {
  // 让几条路**轮流真跑**，中间不重置 —— 「整段覆盖」那种写法在这里会当场露馅（把 `writeSegKey` 改成整段覆盖 ⇒ 这一格红）。
  let disk: Record<string, unknown>;
  beforeEach(() => {
    vi.clearAllMocks();
    disk = { 别人的: { 留着: true }, tabBar: { 不认识的: 1 } };
    mockLoad.mockImplementation(async () => JSON.parse(JSON.stringify(disk)) as never);
    mockSave.mockImplementation(async (v: unknown) => {
      disk = JSON.parse(JSON.stringify(v)) as Record<string, unknown>;
    });
  });

  it("order → collapsed → hiddenOpen → order，几样东西都在，且值逐字对得上", async () => {
    await setTabOrder(["a", "b"]);
    await setCollapsed(["g1"]);
    await setHiddenOpen(true);
    await setTabOrder(["b", "a"]);
    const seg = disk.tabBar as Record<string, unknown>;
    expect(seg.order, "最后一趟的顺序没落上").toEqual(["b", "a"]);
    expect(seg.collapsed, "🔴 写 order 把收着的组冲掉了").toEqual(["g1"]);
    expect(seg.hiddenOpen).toBe(true);
    expect(seg.不认识的, "段里不认识的键也不许动").toBe(1);
    expect(disk.别人的).toEqual({ 留着: true });
  });

  it("读回来的与写进去的**是同一份**", async () => {
    await setTabOrder(["x", "y"]);
    await setCollapsed(["g2", "g3"]);
    await expect(getTabOrder(new Set(["x", "y"]))).resolves.toEqual(["x", "y"]);
    await expect(getCollapsed()).resolves.toEqual(["g2", "g3"]);
  });
});
