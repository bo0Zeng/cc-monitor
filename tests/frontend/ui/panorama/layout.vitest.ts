import { describe, it, expect } from "vitest";
import {
  worldToScreen,
  screenToWorld,
  zoomAt,
  fitViewport,
  clamp,
  bubbleRadius,
  packCluster,
  hitTest,
  coverageBanner,
  computeLayout,
  fitLabel,
  UNCATEGORIZED_LABEL,
  MIN_SCALE,
  MAX_SCALE,
  type Viewport,
  type FileBubble,
  touchedFiles,
  countShown,
} from "../../../../src/frontend/ui/panorama/layout";
import type { Overview } from "../../../../src/frontend/ui/panorama/types";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const vp = (x: number, y: number, scale: number): Viewport => ({ x, y, scale });

describe("坐标变换 world↔screen", () => {
  it("worldToScreen 应用 scale + 平移", () => {
    expect(worldToScreen({ x: 10, y: 20 }, vp(100, 50, 2))).toEqual({
      x: 120,
      y: 90,
    });
  });

  it("screenToWorld 是 worldToScreen 的逆", () => {
    const v = vp(37, -12, 1.7);
    for (const p of [
      { x: 0, y: 0 },
      { x: 5, y: 9 },
      { x: -3.2, y: 41 },
    ]) {
      const back = screenToWorld(worldToScreen(p, v), v);
      expect(back.x).toBeCloseTo(p.x, 9);
      expect(back.y).toBeCloseTo(p.y, 9);
    }
  });

  it("clamp 夹取边界", () => {
    expect(clamp(5, 0, 10)).toBe(5);
    expect(clamp(-1, 0, 10)).toBe(0);
    expect(clamp(11, 0, 10)).toBe(10);
  });
});

describe("zoomAt 锚点缩放", () => {
  it("锚点下的 world 点缩放后仍落回同一 screen 点", () => {
    const v = vp(20, 30, 1);
    const sx = 200;
    const sy = 150;
    const worldBefore = screenToWorld({ x: sx, y: sy }, v);
    const v2 = zoomAt(v, sx, sy, 1.25);
    const screenAfter = worldToScreen(worldBefore, v2);
    expect(screenAfter.x).toBeCloseTo(sx, 6);
    expect(screenAfter.y).toBeCloseTo(sy, 6);
    expect(v2.scale).toBeCloseTo(1.25, 9);
  });

  it("scale 被 clamp 到 [MIN_SCALE, MAX_SCALE]", () => {
    expect(zoomAt(vp(0, 0, MAX_SCALE), 0, 0, 4).scale).toBe(MAX_SCALE);
    expect(zoomAt(vp(0, 0, MIN_SCALE), 0, 0, 0.1).scale).toBe(MIN_SCALE);
  });
});

describe("fitViewport", () => {
  it("居中铺进 screen（world 中心映射到 screen 中心）", () => {
    const v = fitViewport(100, 100, 500, 300, 0);
    const center = worldToScreen({ x: 50, y: 50 }, v);
    expect(center.x).toBeCloseTo(250, 6);
    expect(center.y).toBeCloseTo(150, 6);
    // 受高度限制 → scale = 300/100 = 3（clamp 内）
    expect(v.scale).toBeCloseTo(3, 6);
  });

  it("退化输入回退到居中 scale=1", () => {
    expect(fitViewport(0, 0, 400, 200, 20)).toEqual({ x: 200, y: 100, scale: 1 });
  });
});

describe("bubbleRadius", () => {
  it("min/max score 映射到 min/max 半径", () => {
    expect(bubbleRadius(0, 0, 100, 10, 50)).toBeCloseTo(10, 6);
    expect(bubbleRadius(100, 0, 100, 10, 50)).toBeCloseTo(50, 6);
  });
  it("分数全相等取中值半径", () => {
    // norm=0.5 → 10 + 40*sqrt(0.5)
    expect(bubbleRadius(7, 7, 7, 10, 50)).toBeCloseTo(10 + 40 * Math.SQRT1_2, 6);
  });
  it("半径随 score 单调不减", () => {
    const rs = [0, 25, 50, 75, 100].map((s) => bubbleRadius(s, 0, 100, 10, 50));
    for (let i = 1; i < rs.length; i++) expect(rs[i]).toBeGreaterThanOrEqual(rs[i - 1]);
  });
});

describe("packCluster 确定性无重叠打包", () => {
  it("首圆在中心", () => {
    const pos = packCluster([20], 4);
    expect(pos[0]).toEqual({ x: 0, y: 0 });
  });

  it("放置的圆两两不重叠（含 pad）", () => {
    const radii = [30, 22, 22, 18, 15, 15, 12, 12, 10, 8, 8, 6];
    const pad = 5;
    const pos = packCluster(radii, pad);
    expect(pos).toHaveLength(radii.length);
    for (let i = 0; i < radii.length; i++) {
      for (let j = i + 1; j < radii.length; j++) {
        const d = Math.hypot(pos[i].x - pos[j].x, pos[i].y - pos[j].y);
        // 允许极小浮点余量
        expect(d).toBeGreaterThanOrEqual(radii[i] + radii[j] + pad - 1e-6);
      }
    }
  });

  it("确定性：同输入同输出", () => {
    const radii = [12, 12, 12, 12, 12];
    expect(packCluster(radii, 3)).toEqual(packCluster(radii, 3));
  });
});

describe("hitTest", () => {
  const bubbles: FileBubble[] = [
    { file: "a", score: 1, symbols: 1, subsystem: "s", isEntry: false, hue: 0, x: 0, y: 0, r: 10 },
    { file: "b", score: 1, symbols: 1, subsystem: "s", isEntry: false, hue: 0, x: 100, y: 0, r: 20 },
  ];
  it("命中圆内点（identity viewport）", () => {
    expect(hitTest(0, 0, bubbles, vp(0, 0, 1))?.file).toBe("a");
    expect(hitTest(105, 5, bubbles, vp(0, 0, 1))?.file).toBe("b");
  });
  it("圆外点返回 null", () => {
    expect(hitTest(50, 50, bubbles, vp(0, 0, 1))).toBeNull();
  });
  it("viewport 变换后按 screen 命中", () => {
    // scale=2, 平移 (10,10)：world a(0,0) → screen(10,10)，半径 20
    expect(hitTest(12, 12, bubbles, vp(10, 10, 2))?.file).toBe("a");
  });
  it("重叠时取圆心更近者", () => {
    const overlap: FileBubble[] = [
      { file: "big", score: 1, symbols: 1, subsystem: "s", isEntry: false, hue: 0, x: 0, y: 0, r: 50 },
      { file: "small", score: 1, symbols: 1, subsystem: "s", isEntry: false, hue: 0, x: 40, y: 0, r: 20 },
    ];
    expect(hitTest(41, 0, overlap, vp(0, 0, 1))?.file).toBe("small");
  });
});

describe("coverageBanner 覆盖信号文案", () => {
  it("零缺口返回 null", () => {
    expect(coverageBanner({ unresolved_calls: 0, parse_errors: 0 })).toBeNull();
  });
  it("只有未解析调用", () => {
    expect(coverageBanner({ unresolved_calls: 7, parse_errors: 0 })).toBe(
      "覆盖不全：7 处调用未解析（静态分析已知缺口）",
    );
  });
  it("只有解析失败", () => {
    expect(coverageBanner({ unresolved_calls: 0, parse_errors: 3 })).toBe(
      "覆盖不全：3 文件解析失败（静态分析已知缺口）",
    );
  });
  it("两者都有 → 顿号连接", () => {
    expect(coverageBanner({ unresolved_calls: 7, parse_errors: 3 })).toBe(
      "覆盖不全：7 处调用未解析、3 文件解析失败（静态分析已知缺口）",
    );
  });
});

describe("computeLayout", () => {
  const overview: Overview = {
    spine_files: [
      { file: "src/a.ts", score: 100, symbols: 12 },
      { file: "src/b.ts", score: 80, symbols: 8 },
      { file: "src/c.ts", score: 40, symbols: 4 },
      { file: "src/lonely.ts", score: 20, symbols: 2 },
    ],
    subsystems: [
      { label: "core", files: ["src/a.ts", "src/b.ts"], size: 2, member_hash: "", anchors: [], internal_edges: 0, external_edges: 0 },
      { label: "util", files: ["src/c.ts"], size: 1, member_hash: "", anchors: [], internal_edges: 0, external_edges: 0 },
    ],
    entry_points: [{ id: "src/a.ts#main", file: "src/a.ts", symbol: "main" }],
    total_symbols: 26,
    total_files: 4,
    unresolved_calls: 5,
    ambiguous_calls: 0,
    unresolved_imports: 0,
    parse_errors: 1,
  };

  it("每个脊柱文件产一个气泡，归对子系统", () => {
    const layout = computeLayout(overview);
    expect(layout.bubbles).toHaveLength(4);
    const byFile = new Map(layout.bubbles.map((b) => [b.file, b]));
    expect(byFile.get("src/a.ts")!.subsystem).toBe("core");
    expect(byFile.get("src/b.ts")!.subsystem).toBe("core");
    expect(byFile.get("src/c.ts")!.subsystem).toBe("util");
    // 不属任何 subsystem → 未归类
    expect(byFile.get("src/lonely.ts")!.subsystem).toBe(UNCATEGORIZED_LABEL);
  });

  it("入口点文件标 isEntry", () => {
    const layout = computeLayout(overview);
    const a = layout.bubbles.find((b) => b.file === "src/a.ts")!;
    expect(a.isEntry).toBe(true);
    expect(layout.bubbles.find((b) => b.file === "src/b.ts")!.isEntry).toBe(false);
  });

  it("同子系统气泡同色相；未归类恒排最后一个区", () => {
    const layout = computeLayout(overview);
    const a = layout.bubbles.find((b) => b.file === "src/a.ts")!;
    const b = layout.bubbles.find((b) => b.file === "src/b.ts")!;
    expect(a.hue).toBe(b.hue);
    expect(layout.regions[layout.regions.length - 1].label).toBe(UNCATEGORIZED_LABEL);
  });

  it("区域含正确文件数 + world 边界为正", () => {
    const layout = computeLayout(overview);
    const core = layout.regions.find((r) => r.label === "core")!;
    expect(core.fileCount).toBe(2);
    expect(layout.width).toBeGreaterThan(0);
    expect(layout.height).toBeGreaterThan(0);
  });

  it("空 spine 产空布局", () => {
    const empty = computeLayout({ ...overview, spine_files: [] });
    expect(empty.bubbles).toHaveLength(0);
    expect(empty.regions).toHaveLength(0);
    expect(empty.width).toBe(0);
  });

  it("确定性：同输入同坐标", () => {
    expect(computeLayout(overview)).toEqual(computeLayout(overview));
  });
});

describe("F70 高亮派生（touchedFiles / countShown）", () => {
  const bub = (file: string): FileBubble => ({ file }) as FileBubble;
  it("touchedFiles：〔P7〕读上游给的 file 字段去重（id 长什么样不看：这里故意给一个拆不出文件的 id）", () => {
    const ref = (id: string, file: string) => ({ id, file, symbol: null });
    expect([...touchedFiles([ref("x", "a.ts"), ref("y", "a.ts"), ref("z", "b.rs")])]).toEqual(["a.ts", "b.rs"]);
  });
  it("countShown：只数在气泡集里的高亮文件（非脊柱文件不计）", () => {
    const bubbles = [bub("a.ts"), bub("b.rs"), bub("c.py")];
    const touched = new Set(["a.ts", "c.py", "z.go"]); // z.go 无气泡
    expect(countShown(bubbles, touched)).toBe(2);
  });
  it("countShown：空集 / 无交集 → 0", () => {
    expect(countShown([], new Set(["a"]))).toBe(0);
    expect(countShown([bub("a.ts")], new Set(["b.ts"]))).toBe(0);
  });
});

// SHOTS 报备「气泡全景的文件名截断得厉害、泡没居中」（并进 `§4.4` 全景那一行）。只摆位置（CP1）。
describe("〔P3〕气泡在子系统盒里居中", () => {
  // 半径悬殊的一簇：打包从第一个圆心向外长 ⇒ 圆群偏在一边（改前按「最远圆心距」留边，左右不等）
  const ov: Overview = {
    spine_files: [
      { file: "a/1.ts", score: 100, symbols: 1 },
      { file: "a/2.ts", score: 60, symbols: 1 },
      { file: "a/3.ts", score: 10, symbols: 1 },
      { file: "b/1.ts", score: 30, symbols: 1 },
    ],
    subsystems: [
      { label: "a", files: ["a/1.ts", "a/2.ts", "a/3.ts"], size: 3, member_hash: "", anchors: [], internal_edges: 0, external_edges: 0 },
      { label: "b", files: ["b/1.ts"], size: 1, member_hash: "", anchors: [], internal_edges: 0, external_edges: 0 },
    ],
    entry_points: [],
    total_symbols: 4,
    total_files: 4,
    unresolved_calls: 0,
    ambiguous_calls: 0,
    unresolved_imports: 0,
    parse_errors: 0,
  };
  const PAD = 18;
  const LABEL = 26;

  it("每个区：圆群真包围盒到盒子四边的留白都 == 盒内边距（上边从标签留白之下量）", () => {
    const layout = computeLayout(ov, { regionPad: PAD, labelSpace: LABEL });
    expect(layout.regions.length).toBe(2);
    for (const rg of layout.regions) {
      const mine = layout.bubbles.filter((b) => b.subsystem === rg.label);
      const minX = Math.min(...mine.map((b) => b.x - b.r));
      const maxX = Math.max(...mine.map((b) => b.x + b.r));
      const minY = Math.min(...mine.map((b) => b.y - b.r));
      const maxY = Math.max(...mine.map((b) => b.y + b.r));
      const gaps = [minX - rg.boxX, rg.boxX + rg.boxW - maxX, minY - (rg.boxY + LABEL), rg.boxY + rg.boxH - maxY];
      for (const g of gaps) expect(g, `${rg.label} 的四边留白 ${gaps.join(" / ")}`).toBeCloseTo(PAD, 6);
    }
  });
});

describe("〔P3〕fitViewport 封顶倍数（选图那一块：小图不撑满屏）", () => {
  it("给了 maxScale ⇒ 放大到它为止；不给照旧到 MAX_SCALE", () => {
    expect(fitViewport(10, 10, 1000, 1000, 0, 2).scale).toBe(2);
    expect(fitViewport(10, 10, 1000, 1000, 0).scale).toBe(MAX_SCALE);
  });
});

describe("〔P3〕fitLabel：气泡里的文件名先缩字号、再按实测宽度截断", () => {
  // 替身量法：每个字符 0.6 em（与 canvas 无关，只要单调）
  const measure = (t: string, px: number): number => [...t].length * px * 0.6;

  it("放得下 ⇒ 全名、最大字号", () => {
    expect(fitLabel("a.ts", 100, 13, 9, measure)).toEqual({ text: "a.ts", px: 13 });
  });

  it("最大字号放不下、小一号放得下 ⇒ 缩字号，不截断", () => {
    const name = "abcdefghij"; // 13px 宽 78 · 12px 宽 72
    expect(fitLabel(name, 75, 13, 9, measure)).toEqual({ text: name, px: 12 });
  });

  it("最小字号也放不下 ⇒ 最小字号截断，且是放得下的最长那一截", () => {
    const name = "a-very-long-module-name.ts";
    const got = fitLabel(name, 60, 13, 9, measure)!;
    expect(got.px).toBe(9);
    expect(measure(got.text, 9)).toBeLessThanOrEqual(60);
    const kept = [...got.text].length - 1;
    const longer = copyText("layout.label.ellipsis", { text: [...name].slice(0, kept + 1).join("") });
    expect(got.text).toBe(copyText("layout.label.ellipsis", { text: [...name].slice(0, kept).join("") }));
    expect(measure(longer, 9), "还能多放一个字却截掉了").toBeGreaterThan(60);
  });

  it("连一个字加省略号都放不下 ⇒ null（不画）", () => {
    expect(fitLabel("abc", 5, 13, 9, measure)).toBeNull();
  });
});
