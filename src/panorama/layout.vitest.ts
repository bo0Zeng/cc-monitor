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
  UNCATEGORIZED_LABEL,
  MIN_SCALE,
  MAX_SCALE,
  type Viewport,
  type FileBubble,
} from "./layout";
import type { Overview } from "./types";

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
      { label: "core", files: ["src/a.ts", "src/b.ts"], size: 2 },
      { label: "util", files: ["src/c.ts"], size: 1 },
    ],
    entry_points: ["src/a.ts#main"],
    total_symbols: 26,
    total_files: 4,
    unresolved_calls: 5,
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
