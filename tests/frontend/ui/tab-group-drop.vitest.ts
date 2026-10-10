/**
 * 标签页分组的落点算术（`tab-drop.ts`，纯函数）：组与散的混排 · 组员挨着 · 一行切三段 · 组头 · 组外 · 栏底 ·
 * 建组原位 · 默认组名。
 */
import { describe, expect, it } from "vitest";
import {
  barRows,
  contiguous,
  defaultGroupName,
  dwellCandidate,
  groupMoveForDrop,
  pickDropTarget,
  planDrop,
  planGroupDrop,
  pickGroupDropTarget,
  autoScrollStep,
  keyMoveTarget,
  keyGroupMoveTarget,
  keyLeaveTarget,
  keyJoinPrevTarget,
  sameDirName,
  DRAG_THRESHOLD_PX,
  DWELL_MS,
  DWELL_MOVE_PX,
  type DropTarget,
  type RowRect,
} from "../../../src/frontend/ui/tab-drop";
import { copyText } from "../../../src/frontend/ui/copy-table";

const of =
  (m: Record<string, string>) =>
  (sid: string): string | null =>
    m[sid] ?? null;

describe("组与散的混排：一个顺序，组员永远挨着", () => {
  it("组员散在各处 ⇒ 聚到第一个组员那一格，组外的相对次序不变", () => {
    const g = of({ a: "g1", c: "g1", e: "g2" });
    expect(contiguous(["x", "a", "b", "c", "d", "e"], g, new Set(["g1", "g2"]))).toEqual(["x", "a", "c", "b", "d", "e"]);
  });

  it("组表里没有的组 id ⇒ 当散的（不聚）", () => {
    expect(contiguous(["a", "x", "b"], of({ a: "gone", b: "gone" }), new Set())).toEqual(["a", "x", "b"]);
  });

  it("栏里的行：组头在第一个组员之前，散的排在原位（不是所有组整块在上）", () => {
    const rows = barRows(["x", "a", "b", "y"], of({ a: "g1", b: "g1" }), ["g1"]);
    expect(rows).toEqual([
      { kind: "tab", sid: "x", gid: null },
      { kind: "head", gid: "g1" },
      { kind: "tab", sid: "a", gid: "g1" },
      { kind: "tab", sid: "b", gid: "g1" },
      { kind: "tab", sid: "y", gid: null },
    ]);
  });

  it("组次序 ＝ 第一个组员的位置，不是组表的先后", () => {
    const rows = barRows(["b1", "a1"], of({ a1: "ga", b1: "gb" }), ["ga", "gb"]);
    expect(rows.filter((r) => r.kind === "head").map((r) => (r as { gid: string }).gid)).toEqual(["gb", "ga"]);
  });

  it("一个组员都还没到的组 ⇒ 组头排在最后", () => {
    expect(barRows(["x"], of({}), ["g1"])).toEqual([
      { kind: "tab", sid: "x", gid: null },
      { kind: "head", gid: "g1" },
    ]);
  });
});

/** 栏：x（散）· 组头 g1 · a b（g1）· 6px 空 · y（散）。行高 30，组头 26。 */
const ROWS: RowRect[] = [
  { kind: "tab", id: "x", gid: null, top: 0, height: 30 },
  { kind: "head", id: "g1", gid: "g1", top: 30, height: 26 },
  { kind: "tab", id: "a", gid: "g1", top: 56, height: 30 },
  { kind: "tab", id: "b", gid: "g1", top: 86, height: 30 },
  { kind: "tab", id: "y", gid: null, top: 122, height: 30 },
];
const none = new Set<string>(["z"]);
const ins = (sid: string | null, side: "before" | "after", gid: string | null): DropTarget => ({
  kind: "insert",
  at: sid === null ? null : { sid, side },
  gid,
});

describe("一行切三段：上 1/4 插前 · 中 1/2 停住才建组 · 下 1/4 插后", () => {
  it("上 1/4 ⇒ 插到它前面；下 1/4 ⇒ 插到它后面", () => {
    expect(pickDropTarget(ROWS, 2, none, null)).toEqual(ins("x", "before", null));
    expect(pickDropTarget(ROWS, 28, none, null)).toEqual(ins("x", "after", null));
  });

  it("中段没停够 ⇒ 当插入，线画在离指针近的那条边", () => {
    expect(pickDropTarget(ROWS, 10, none, null)).toEqual(ins("x", "before", null));
    expect(pickDropTarget(ROWS, 20, none, null)).toEqual(ins("x", "after", null));
  });

  it("中段停够了（计时器说是它、指针也还在它中段）⇒ 和它建组 / 进它的组", () => {
    expect(pickDropTarget(ROWS, 15, none, "x")).toEqual({ kind: "onto", sid: "x" });
    expect(pickDropTarget(ROWS, 71, none, "a")).toEqual({ kind: "onto", sid: "a" });
  });

  it("计时器说是它，指针却已在它的上 / 下 1/4 或别的行 ⇒ 不建组", () => {
    expect(pickDropTarget(ROWS, 2, none, "x"), "上 1/4 永远只插入").toEqual(ins("x", "before", null));
    expect(pickDropTarget(ROWS, 71, none, "x"), "指针在 a 上").toEqual(ins("a", "after", "g1"));
  });

  it("停留只在中段攒：上 / 下 1/4、组头、被拖的那一行都不攒", () => {
    expect(dwellCandidate(ROWS, 15, none)).toBe("x");
    expect(dwellCandidate(ROWS, 3, none)).toBeNull();
    expect(dwellCandidate(ROWS, 27, none)).toBeNull();
    expect(dwellCandidate(ROWS, 40, none), "组头").toBeNull();
    expect(dwellCandidate(ROWS, 15, new Set(["x"])), "压在自己身上").toBeNull();
    expect(pickDropTarget(ROWS, 15, new Set(["x"]), "x"), "自己身上停够了也不建组").toEqual(ins("x", "after", null));
  });

  it("组员上插入 ⇒ 在组里；组的最后一行的下 1/4 ⇒ 排到组末尾（还在组里）", () => {
    expect(pickDropTarget(ROWS, 58, none, null)).toEqual(ins("a", "before", "g1"));
    expect(pickDropTarget(ROWS, 113, none, null)).toEqual(ins("b", "after", "g1"));
  });

  it("组下沿那 6px ⇒ 组后面、组外", () => {
    expect(pickDropTarget(ROWS, 118, none, null)).toEqual(ins("b", "after", null));
  });

  it("组头：上 1/3 ⇒ 插到组前面（组外）；下 2/3 ⇒ 进组排第一，不用停", () => {
    expect(pickDropTarget(ROWS, 32, none, null)).toEqual(ins("a", "before", null));
    expect(pickDropTarget(ROWS, 50, none, null)).toEqual({ ...ins("a", "before", "g1"), head: "g1" });
  });

  it("栏底空白 ⇒ 末尾、组外", () => {
    expect(pickDropTarget(ROWS, 400, none, null)).toEqual(ins(null, "after", null));
  });

  it("三个常数：停 400ms · 抖 4px 清零 · 起拖 4px", () => {
    expect([DWELL_MS, DWELL_MOVE_PX, DRAG_THRESHOLD_PX]).toEqual([400, 4, 4]);
  });
});

describe("落下之后：顺序 ＋ 归属一次算好", () => {
  const known = new Set(["g1"]);
  const g = of({ a: "g1", b: "g1" });
  const base = ["x", "a", "b", "y"];

  it("散的插到散的之间 ⇒ 只排序", () => {
    expect(planDrop(base, g, known, ["y"], ins("x", "before", null))).toEqual({ order: ["y", "x", "a", "b"], move: { kind: "stay" } });
  });

  it("插到组末尾（组里）⇒ 进组，排最后", () => {
    expect(planDrop(base, g, known, ["x"], ins("b", "after", "g1"))).toEqual({ order: ["a", "b", "x", "y"], move: { kind: "join", gid: "g1" } });
  });

  it("组员放到组后面（组外）⇒ 出组，留在落点", () => {
    expect(planDrop(base, g, known, ["a"], ins("b", "after", null))).toEqual({ order: ["x", "b", "a", "y"], move: { kind: "leave" } });
  });

  it("组员放到组前面（组头上 1/3）⇒ 出组，在组前面", () => {
    expect(planDrop(base, g, known, ["b"], ins("a", "before", null))).toEqual({ order: ["x", "b", "a", "y"], move: { kind: "leave" } });
  });

  it("建组 ⇒ 组就在目标那一格：目标在前、被拖的在后", () => {
    expect(planDrop(base, g, known, ["x"], { kind: "onto", sid: "y" })).toEqual({ order: ["a", "b", "y", "x"], move: { kind: "found", with: "y" } });
    expect(planDrop(["p", "q", "r", "s"], of({}), new Set(), ["s"], { kind: "onto", sid: "q" }).order, "不跳到最上面").toEqual(["p", "q", "s", "r"]);
  });

  it("压在组员上停住 ⇒ 进它的组，排在它后面", () => {
    expect(planDrop(base, g, known, ["y"], { kind: "onto", sid: "a" })).toEqual({ order: ["x", "a", "y", "b"], move: { kind: "join", gid: "g1" } });
  });

  it("盘上读回来的顺序组员不挨着 ⇒ 按看到的样子算（不是按底序）", () => {
    // 底序 a x b y：看到的是 g1{a b} x y。y 放到 b 后面（组外）⇒ 在 x 前面。
    expect(planDrop(["a", "x", "b", "y"], g, known, ["y"], ins("b", "after", null)).order).toEqual(["a", "b", "y", "x"]);
  });

  it("压在自己身上 ⇒ 什么都不变", () => {
    expect(planDrop(base, g, known, ["x"], { kind: "onto", sid: "x" })).toEqual({ order: base, move: { kind: "stay" } });
  });

  it("归属：几个一起拖、都已在目标组里 ⇒ stay；有一个不在 ⇒ join", () => {
    expect(groupMoveForDrop(g, ["a", "b"], ins("a", "before", "g1"))).toEqual({ kind: "stay" });
    expect(groupMoveForDrop(g, ["a", "x"], ins("a", "before", "g1"))).toEqual({ kind: "join", gid: "g1" });
  });
});

describe("新组名：两个 cwd 完全相同 ⇒ 那个目录名，否则「分组 N」", () => {
  it("同目录（含两种分隔符 · 尾斜杠）⇒ 目录名", () => {
    expect(sameDirName("/home/u/web-console", "/home/u/web-console/")).toBe("web-console");
    expect(sameDirName("C:\\work\\app", "C:/work/app")).toBe("app");
  });

  it("只共前缀不算（「work」没意义）；盘符不当名字；缺一边 ⇒ 没有", () => {
    expect(sameDirName("/home/user/work/docs", "/home/user/work/cli")).toBeNull();
    expect(sameDirName("C:\\", "C:\\")).toBeNull();
    expect(sameDirName(null, "/a")).toBeNull();
  });

  it("不同目录 ⇒ 分组 N（现有最大号 ＋1）", () => {
    const n = (k: number): string => copyText("tabDrop.group.defaultName", { n: k });
    expect(defaultGroupName("/home/user/work/docs", "/home/user/work/cli", [n(1), "白天", n(3)])).toBe(n(4));
    expect(defaultGroupName("/w/web-console", "/w/web-console", [n(1)])).toBe("web-console");
  });
});

describe("收着的组", () => {
  /** 栏：x · 组头 g1（收着，组员 a b 量出来高 0）· y。 */
  const FOLDED: RowRect[] = [
    { kind: "tab", id: "x", gid: null, top: 0, height: 30 },
    { kind: "head", id: "g1", gid: "g1", top: 34, height: 26, collapsed: true },
    { kind: "tab", id: "a", gid: "g1", top: 0, height: 0 },
    { kind: "tab", id: "b", gid: "g1", top: 0, height: 0 },
    { kind: "tab", id: "y", gid: null, top: 66, height: 30 },
  ];

  it("整个组头 ＝ 进组排最后（组保持收着）", () => {
    expect(pickDropTarget(FOLDED, 35, none, null)).toEqual({ ...ins("b", "after", "g1"), head: "g1" });
    expect(pickDropTarget(FOLDED, 58, none, null)).toEqual({ ...ins("b", "after", "g1"), head: "g1" });
  });

  it("收着的组头下面那段空 ⇒ 组后面、组外", () => {
    expect(pickDropTarget(FOLDED, 63, none, null)).toEqual(ins("b", "after", null));
  });

  it("组员藏着：停留攒不到它们身上，指针也落不到它们身上", () => {
    expect(dwellCandidate(FOLDED, 0, none)).toBeNull();
    expect(pickDropTarget(FOLDED, 15, none, null)).toEqual(ins("x", "after", null));
  });

  it("行：收着的组的组员标 hidden（数字键 · ] [ 跳过它们），组头带 collapsed", () => {
    expect(barRows(["x", "a", "y"], of({ a: "g1" }), ["g1"], new Set(["g1"]))).toEqual([
      { kind: "tab", sid: "x", gid: null },
      { kind: "head", gid: "g1", collapsed: true },
      { kind: "tab", sid: "a", gid: "g1", hidden: true },
      { kind: "tab", sid: "y", gid: null },
    ]);
  });
});

describe("拖整个组：只认组外的落点，不合并、不嵌套", () => {
  /** 栏：x · 组 g1（a b）· 6px 空 · 组 g2（c）· y。 */
  const R: RowRect[] = [
    { kind: "tab", id: "x", gid: null, top: 0, height: 30 },
    { kind: "head", id: "g1", gid: "g1", top: 30, height: 26 },
    { kind: "tab", id: "a", gid: "g1", top: 56, height: 30 },
    { kind: "tab", id: "b", gid: "g1", top: 86, height: 30 },
    { kind: "head", id: "g2", gid: "g2", top: 122, height: 26 },
    { kind: "tab", id: "c", gid: "g2", top: 148, height: 30 },
    { kind: "tab", id: "y", gid: null, top: 184, height: 30 },
  ];
  const at = (sid: string | null, side: "before" | "after" = "before") => (sid === null ? null : { sid, side });

  it("压在散的上 ⇒ 上半插前、下半插后", () => {
    expect(pickGroupDropTarget(R, 5, "g2")).toEqual(at("x"));
    expect(pickGroupDropTarget(R, 25, "g2")).toEqual(at("x", "after"));
  });

  it("压在别的组里（组头或组员）⇒ 那个组的前面或后面（按离哪头近），不进组", () => {
    expect(pickGroupDropTarget(R, 40, "g2"), "g1 上半").toEqual(at("a"));
    expect(pickGroupDropTarget(R, 100, "g2"), "g1 下半").toEqual(at("b", "after"));
  });

  it("压在自己组上 ⇒ 原位（顺序不变）；栏底 ⇒ 末尾", () => {
    expect(planGroupDrop(["x", "a", "b", "c", "y"], of({ a: "g1", b: "g1", c: "g2" }), new Set(["g1", "g2"]), "g2", pickGroupDropTarget(R, 160, "g2"))).toEqual(["x", "a", "b", "c", "y"]);
    expect(pickGroupDropTarget(R, 500, "g2")).toBeNull();
  });

  it("挪过去组员次序不变、组还是那个组", () => {
    const g = of({ a: "g1", b: "g1", c: "g2" });
    expect(planGroupDrop(["x", "a", "b", "c", "y"], g, new Set(["g1", "g2"]), "g1", at("y", "after"))).toEqual(["x", "c", "y", "a", "b"]);
    expect(planGroupDrop(["x", "a", "b", "c", "y"], g, new Set(["g1", "g2"]), "g1", at("x"))).toEqual(["a", "b", "x", "c", "y"]);
  });
});

describe("多选一起拖", () => {
  const known = new Set(["g1"]);
  const g = of({ a: "g1", b: "g1" });

  it("落下后按原先后挨在一起", () => {
    expect(planDrop(["p", "q", "r", "s", "t"], of({}), new Set(), ["t", "q"], ins("p", "before", null)).order).toEqual(["q", "t", "p", "r", "s"]);
  });

  it("一起进组 / 一起建组", () => {
    expect(planDrop(["x", "a", "b", "y", "z"], g, known, ["y", "z"], ins("b", "after", "g1"))).toEqual({ order: ["x", "a", "b", "y", "z"], move: { kind: "join", gid: "g1" } });
    expect(planDrop(["x", "a", "b", "y", "z"], g, known, ["a", "z"], { kind: "onto", sid: "x" })).toEqual({ order: ["x", "a", "z", "b", "y"], move: { kind: "found", with: "x" } });
  });
});

describe("拖的时候栏自己滚", () => {
  it("进栏顶 / 栏底 24px 内开始滚，离边越近越快，贴边每帧 12px；中间不滚", () => {
    expect(autoScrollStep(300, 100, 600)).toBe(0);
    expect(autoScrollStep(124, 100, 600)).toBe(0);
    expect(autoScrollStep(112, 100, 600)).toBe(-6);
    expect(autoScrollStep(100, 100, 600)).toBe(-12);
    expect(autoScrollStep(90, 100, 600), "越过上沿也按最快").toBe(-12);
    expect(autoScrollStep(594, 100, 600)).toBe(9);
  });
});

describe("键盘挪位：按看到的顺序上下各一格算落点，再走拖放那一套", () => {
  // 栏：x · 组 ga（a1 a2）· y · 组 gb（b1 b2）· z
  const order = ["x", "a1", "a2", "y", "b1", "b2", "z"];
  const g = of({ a1: "ga", a2: "ga", b1: "gb", b2: "gb" });
  const known = new Set(["ga", "gb"]);
  const rows = (folded: string[] = []) => barRows(order, g, ["ga", "gb"], new Set(folded));
  /** 挪一格之后：新顺序 ＋ 归属怎么变；`null` ＝ 不动。 */
  const step = (dragged: string[], dir: -1 | 1, folded: string[] = []) => {
    const t = keyMoveTarget(rows(folded), dragged, dir);
    return t === null ? null : planDrop(order, g, known, dragged, t);
  };

  it("组里往上 / 往下 ⇒ 组里换位", () => {
    expect(step(["a2"], -1)).toEqual({ order: ["x", "a2", "a1", "y", "b1", "b2", "z"], move: { kind: "stay" } });
    expect(step(["a1"], 1)).toEqual({ order: ["x", "a2", "a1", "y", "b1", "b2", "z"], move: { kind: "stay" } });
  });

  it("组里第一个再往上 ⇒ 出组放到组前面；最后一个再往下 ⇒ 出组放到组后面", () => {
    expect(step(["a1"], -1)).toEqual({ order, move: { kind: "leave" } });
    expect(keyMoveTarget(rows(), ["a1"], -1)).toEqual({ kind: "insert", at: { sid: "a1", side: "before" }, gid: null });
    expect(step(["a2"], 1)).toEqual({ order, move: { kind: "leave" } });
    expect(keyMoveTarget(rows(), ["a2"], 1)).toEqual({ kind: "insert", at: { sid: "a2", side: "after" }, gid: null });
  });

  it("从组上面往下碰到组头 ⇒ 进组排第一；从组下面往上 ⇒ 进组排最后", () => {
    expect(step(["x"], 1)).toEqual({ order, move: { kind: "join", gid: "ga" } });
    expect(keyMoveTarget(rows(), ["x"], 1)).toEqual({ kind: "insert", at: { sid: "a1", side: "before" }, gid: "ga" });
    expect(step(["y"], -1)).toEqual({ order, move: { kind: "join", gid: "ga" } });
    expect(keyMoveTarget(rows(), ["y"], -1)).toEqual({ kind: "insert", at: { sid: "a2", side: "after" }, gid: "ga" });
  });

  it("收着的组：整段跳过去，不进组", () => {
    expect(step(["x"], 1, ["ga"])).toEqual({ order: ["a1", "a2", "x", "y", "b1", "b2", "z"], move: { kind: "stay" } });
    expect(step(["y"], -1, ["ga"])).toEqual({ order: ["x", "y", "a1", "a2", "b1", "b2", "z"], move: { kind: "stay" } });
  });

  it("顶上再往上 · 底下再往下（散的）⇒ 不动", () => {
    expect(step(["x"], -1)).toBeNull();
    expect(step(["z"], 1)).toBeNull();
  });

  it("选了几个一起挪：按最上面那个往上算，落下后挨在一起", () => {
    expect(step(["y", "z"], -1)).toEqual({ order: ["x", "a1", "a2", "y", "z", "b1", "b2"], move: { kind: "join", gid: "ga" } });
  });

  it("组头上 ⇒ 整组挪一格，跳过别的组整段", () => {
    const gstep = (gid: string, dir: -1 | 1) => {
      const at = keyGroupMoveTarget(rows(), gid, dir);
      return at === undefined ? null : planGroupDrop(order, g, known, gid, at);
    };
    expect(gstep("gb", -1)).toEqual(["x", "a1", "a2", "b1", "b2", "y", "z"]);
    expect(gstep("ga", -1)).toEqual(["a1", "a2", "x", "y", "b1", "b2", "z"]);
    expect(gstep("ga", 1)).toEqual(["x", "y", "a1", "a2", "b1", "b2", "z"]);
    expect(keyGroupMoveTarget(barRows(["x", "b1", "b2", "a1", "a2"], g, ["ga", "gb"]), "ga", -1), "上面是别的组 ⇒ 跳过它整段").toEqual({ sid: "b1", side: "before" });
    expect(gstep("gb", 1)).toEqual(["x", "a1", "a2", "y", "z", "b1", "b2"]);
    expect(keyGroupMoveTarget(barRows(["a1", "a2", "x"], g, ["ga"]), "ga", -1), "已在顶上").toBeUndefined();
  });

  it("出组（Alt+←）⇒ 放到组后面；散的 ⇒ 不动", () => {
    const t = keyLeaveTarget(rows(), ["a1"]);
    expect(t).toEqual({ kind: "insert", at: { sid: "a2", side: "after" }, gid: null });
    expect(planDrop(order, g, known, ["a1"], t!)).toEqual({ order: ["x", "a2", "a1", "y", "b1", "b2", "z"], move: { kind: "leave" } });
    expect(keyLeaveTarget(rows(), ["x"])).toBeNull();
  });

  it("与上一行成组（Alt+→）：上一行散的 ⇒ 建组；上一行在组里 ⇒ 进那个组；收着的组头 ⇒ 进组排最后；已同组 ⇒ 不动", () => {
    expect(keyJoinPrevTarget(barRows(["p", "q"], of({}), []), ["q"])).toEqual({ kind: "onto", sid: "p" });
    expect(keyJoinPrevTarget(rows(), ["y"])).toEqual({ kind: "onto", sid: "a2" });
    expect(planDrop(order, g, known, ["y"], keyJoinPrevTarget(rows(), ["y"])!).move).toEqual({ kind: "join", gid: "ga" });
    expect(keyJoinPrevTarget(rows(["ga"]), ["y"])).toEqual({ kind: "insert", at: { sid: "a2", side: "after" }, gid: "ga" });
    expect(keyJoinPrevTarget(rows(), ["a2"])).toBeNull();
    expect(keyJoinPrevTarget(rows(), ["a1"])).toBeNull();
    expect(keyJoinPrevTarget(rows(), ["x"])).toBeNull();
  });
});
