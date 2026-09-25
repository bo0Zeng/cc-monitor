/**
 * `设计/30` 的落盘契约（`§4` 那个 `tabBar` 段）的判据 ——
 * `§C`（顺序）与 `§B`（固定）两个键住同一个文件，所以判据也住同一份。
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
 * `order` 与 `pinned` 住同一段。任何一侧整段覆盖 ⇒ 另一侧当场被清空，
 * 而且是**静默**的（下次启动才看得见）。`②`（order 侧）与 `⑥`（pinned 侧）
 * 各钉一格，`⑦` 再用一份**真的在内存里的 config** 走一遍两向往返 ——
 * 前两格钉的是「写的时候有没有带上对方」，`⑦` 钉的是「两条路真跑一遍还在不在」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../src/config", () => ({
  loadConfig: vi.fn(),
  saveConfig: vi.fn(),
}));

import { loadConfig, saveConfig } from "../src/config";
import {
  sanitizeOrder,
  getTabOrder,
  setTabOrder,
  ORDER_CAP,
  sanitizePinned,
  getPinned,
  setPinned,
  isDegradedPin,
  PINNED_CAP,
  type PinnedTab,
} from "../src/tab-bar-state";
import { LOCAL_ORIGIN } from "../src/ipc/origin";

const mockLoad = vi.mocked(loadConfig);
const mockSave = vi.mocked(saveConfig);

/** 一条**字段齐全**的固定记录 —— 每格只改它关心的那一两个字段，其余不参与判定。 */
function pin(over: Partial<PinnedTab> = {}): PinnedTab {
  return {
    sid: "s1",
    jsonlPath: "/p/s1.jsonl",
    cwd: "/home/u/proj",
    origin: LOCAL_ORIGIN,
    account: "work",
    lastActiveAt: 1700000000000,
    kind: "interactive",
    name: null,
    title: "proj",
    ...over,
  };
}

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

// ============================ `§B` 固定（pinned）============================

describe("③ 量具自检：`sanitizePinned` 真的在清（不过这格，下面全是空转）", () => {
  it("五种脏输入各被清掉一种，且**每一种单独可见**", () => {
    // 🔴 分开断言的理由同 `⓪`：合成一个大用例的话，只要有一种没清掉，
    //    另外四种也能让结果「看起来对」—— 分开才说得出是哪一种漏了。
    expect(sanitizePinned("不是数组"), "非数组应当回空表，不是抛").toEqual([]);
    expect(
      sanitizePinned([{ jsonlPath: "/a" }, { sid: "  " }, 3, null, "x"]).length,
      "没有 sid（主键）的条目必须整条丢 —— 复活的时候它连 resume 都发不出去",
    ).toBe(0);
    expect(
      sanitizePinned([pin({ sid: "a", title: "先" }), pin({ sid: "a", title: "后" })]).map(
        (p) => p.title,
      ),
      "同一个 sid 出现两次没去重 ⇒ 那个 tab 会被复活两遍",
    ).toEqual(["先"]);
    expect(
      sanitizePinned([
        { sid: "a", cwd: 3, origin: "<local>", account: [], kind: null, lastActiveAt: "昨天" },
      ])[0],
      "字段类型不对的没收敛成 null ⇒ 复活时会把 `3` 当 cwd 传给 `createSkeletonTab`",
    ).toEqual({
      sid: "a",
      jsonlPath: "",
      cwd: null,
      origin: "<local>",
      account: null,
      lastActiveAt: null,
      kind: null,
      name: null,
      title: "a",
    });
    // 〔C4a · `设计/05 §8` 步 2〕origin 不是非空字符串 ⇒ **整条丢**：本机也有名字（`"<local>"`），
    //   盘上的旧 `null` / 空串 / 缺键都复活不出「哪台机器」，猜成本机正是步 2 治的那件事。
    expect(
      sanitizePinned([
        { sid: "n1", origin: null },
        { sid: "n2", origin: "" },
        { sid: "n3" },
        { sid: "ok", origin: "devbox" },
      ]).map((p) => p.sid),
      "origin 说不出哪台机器的那几条没被丢掉 ⇒ 复活时会被悄悄当成本机",
    ).toEqual(["ok"]);
    expect(
      sanitizePinned([{ sid: "abcdefghijkl", origin: "<local>" }])[0].title,
      "标题缺了没兜底 ⇒ 栏里出现一个没名字的 tab（全仓的兜底是 sid 前 8 位）",
    ).toBe("abcdefgh");
  });

  it("字段齐全的一条**原样穿过** —— 清洗不许顺手改好东西", () => {
    // 🔴 这是上一格的反面控：只证明「脏的被清掉」的话，一个「什么都回空表」的
    //    清洗函数同样全绿。这一格钉住它不许把干净数据也吃掉。
    expect(sanitizePinned([pin()])).toEqual([pin()]);
  });

  it("上界是 `PINNED_CAP`，而且**恰好**在那里截断（`§4` 逐字「上界 32 条」）", () => {
    const many = Array.from({ length: PINNED_CAP + 5 }, (_, i) => pin({ sid: `s${i}` }));
    expect(sanitizePinned(many).length, "上界没生效 ⇒ 一份被撑爆的配置能写进盘").toBe(PINNED_CAP);
    const exact = Array.from({ length: PINNED_CAP }, (_, i) => pin({ sid: `s${i}` }));
    expect(sanitizePinned(exact).length, "恰好到上界时不许少截一个（差一错）").toBe(PINNED_CAP);
  });

  it("🔴 `jsonlPath` 为空的**保留**（`§4` 逐字「保留但标记降级」），不是丢掉", () => {
    // 丢掉它 = 「用户固定过的东西第二天自己没了」，那比降级坏。
    expect(sanitizePinned([pin({ sid: "a", jsonlPath: "" })]).map((p) => p.sid)).toEqual(["a"]);
  });

  it("🔴 `pinned` 这一路**没有** `alive` 过滤 —— 那正是它存在的理由", () => {
    // `order` 读的时候按「今天真的存在的 sid」过滤；`pinned` 不许有那一道：
    // 「这条今天不在、明天也要把它造出来」就是固定的全部意义，按存活过滤等于把功能过滤掉。
    expect(
      sanitizePinned.length,
      "`sanitizePinned` 多了一个参数 ⇒ 大概率是有人给它加了存活过滤",
    ).toBe(1);
  });
});

describe("④ `isDegradedPin` 正反两控（`§B.6` 第一格的判词）", () => {
  it("空 `jsonlPath` ⇒ 降级；有路径 ⇒ 不降级", () => {
    expect(isDegradedPin(pin({ jsonlPath: "" })), "降级判不出来 ⇒ 点进去是一片空白").toBe(true);
    expect(
      isDegradedPin(pin({ jsonlPath: "/p/s1.jsonl" })),
      "把正常的也判成降级 ⇒ 所有固定 tab 都拿不到 resume 入口",
    ).toBe(false);
  });
});

describe("⑤ 读 pinned：坏配置不许阻断启动（`§4` 逐字）", () => {
  beforeEach(() => vi.clearAllMocks());

  it("`loadConfig` 抛了 ⇒ 回空表，不往外抛", async () => {
    mockLoad.mockRejectedValueOnce(new Error("盘坏了"));
    await expect(getPinned()).resolves.toEqual([]);
  });

  it("段不是对象（数组 / 字符串 / 缺失）⇒ 一律回空表", async () => {
    for (const seg of [["a"], "x", 3, null, undefined]) {
      mockLoad.mockResolvedValueOnce({ tabBar: seg } as never);
      await expect(getPinned()).resolves.toEqual([]);
    }
  });

  it("读回来的一条**字段逐个对拍**（不是「长度对了就算」）", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: { pinned: [pin({ sid: "s9" })] } } as never);
    await expect(getPinned()).resolves.toEqual([pin({ sid: "s9" })]);
  });
});

describe("⑥ 写 pinned：只动自己那个键（与 `②` 互为镜像）", () => {
  beforeEach(() => vi.clearAllMocks());

  it("🔴 段里的 `order` 原样留着 —— 这是两个键能共用一个段的前提", async () => {
    mockLoad.mockResolvedValueOnce({ tabBar: { order: ["a", "b"], 别人的键: 1 } } as never);
    await setPinned([pin()]);
    const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<
      string,
      unknown
    >;
    expect(seg.pinned, "固定表没写进去").toEqual([pin()]);
    expect(seg.order, "🔴 把 `order` 写没了 —— 用户拖的顺序被固定这条路清空了").toEqual(["a", "b"]);
    expect(seg.别人的键, "段里不认识的键也不许动").toBe(1);
  });

  it("段原本不存在 / 不是对象 ⇒ 新建一个，不抛", async () => {
    for (const prev of [undefined, "坏", ["坏"]]) {
      vi.clearAllMocks();
      mockLoad.mockResolvedValueOnce({ tabBar: prev } as never);
      await setPinned([pin()]);
      const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<
        string,
        unknown
      >;
      expect(seg.pinned).toEqual([pin()]);
    }
  });

  it("`tabCollections` 一个字节不碰", async () => {
    mockLoad.mockResolvedValueOnce({
      tabCollections: [{ id: "c1", name: "n", members: [] }],
    } as never);
    await setPinned([pin()]);
    const written = mockSave.mock.calls[0][0] as Record<string, unknown>;
    expect(written.tabCollections).toEqual([{ id: "c1", name: "n", members: [] }]);
  });

  it("写进去的也过一遍清洗（脏值不许经这条路落盘）", async () => {
    mockLoad.mockResolvedValueOnce({} as never);
    await setPinned([pin({ sid: "a" }), pin({ sid: "a" }), pin({ sid: "" })]);
    const seg = (mockSave.mock.calls[0][0] as Record<string, unknown>).tabBar as Record<
      string,
      unknown
    >;
    expect((seg.pinned as PinnedTab[]).map((p) => p.sid)).toEqual(["a"]);
  });
});

describe("⑦ 🔴 两条路交替写一份**真的在内存里**的 config —— 谁都不许把对方写没", () => {
  // 上面 `②`/`⑥` 钉的是「这一次写有没有带上对方的键」。这一格不同：它让两条路
  // **轮流真跑**，中间不重置 —— 「整段覆盖」那种写法在这里会当场露馅，
  // 而且它是死值验刀 4 的正题（把 `writeSegKey` 改成整段覆盖 ⇒ 这一格红）。
  let disk: Record<string, unknown>;
  beforeEach(() => {
    vi.clearAllMocks();
    disk = { tabCollections: [{ id: "c1", name: "保留我", members: ["z"] }] };
    mockLoad.mockImplementation(async () => JSON.parse(JSON.stringify(disk)) as never);
    mockSave.mockImplementation(async (v: unknown) => {
      disk = JSON.parse(JSON.stringify(v)) as Record<string, unknown>;
    });
  });

  it("order → pinned → order，三趟之后三样东西都在，且值逐字对得上", async () => {
    await setTabOrder(["a", "b"]);
    await setPinned([pin({ sid: "b" })]);
    await setTabOrder(["b", "a"]);

    const seg = disk.tabBar as Record<string, unknown>;
    expect(seg.order, "最后一趟的顺序没落上").toEqual(["b", "a"]);
    expect(seg.pinned, "🔴 第三趟（写 order）把 `pinned` 冲掉了").toEqual([pin({ sid: "b" })]);
    expect(disk.tabCollections, "两条路谁把集合动了").toEqual([
      { id: "c1", name: "保留我", members: ["z"] },
    ]);
  });

  it("反向也走一遍：pinned → order → pinned", async () => {
    await setPinned([pin({ sid: "a" })]);
    await setTabOrder(["a"]);
    await setPinned([pin({ sid: "a" }), pin({ sid: "b" })]);

    const seg = disk.tabBar as Record<string, unknown>;
    expect(seg.order, "🔴 第三趟（写 pinned）把 `order` 冲掉了").toEqual(["a"]);
    expect((seg.pinned as PinnedTab[]).map((p) => p.sid)).toEqual(["a", "b"]);
  });

  it("读回来的与写进去的**是同一份**（往返恒等，钉住「写了但读不回来」那一族）", async () => {
    await setTabOrder(["x", "y"]);
    await setPinned([pin({ sid: "x", jsonlPath: "", title: "降级的那条" })]);
    await expect(getTabOrder(new Set(["x", "y"]))).resolves.toEqual(["x", "y"]);
    await expect(getPinned()).resolves.toEqual([
      pin({ sid: "x", jsonlPath: "", title: "降级的那条" }),
    ]);
  });
});
