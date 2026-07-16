/**
 * Batch15-P2：全景地图的**纯逻辑**（坐标变换 / 命中测试 / 气泡打包布局 / 覆盖文案）。
 *
 * 抽成纯函数、不碰 DOM/canvas，便于 vitest 单测（`layout.vitest.ts`）。canvas 绘制/
 * pan·zoom 交互在 `src/views/panorama.ts`，只**消费**这里算出的坐标/命中结果。
 *
 * ## 渲染形态（P2 = 聚类气泡地图，非力导边图）
 * Overview 是**文件级**、**无文件间边**（见计划「数据认知」）。所以画：
 *   - **子系统** = 带标签的色块区域，shelf（行流）打包排布，确定性；
 *   - **脊柱文件** = 子系统区域内的圆，半径随 score 增长（sqrt 缩放使**面积**≈score）；
 *   - **入口点文件** = 描环（view 侧按 `isEntry` 画）。
 * 函数级调用子图（有边、力导）留 P4——本文件不涉及边布局。
 *
 * ## 坐标系
 * 布局产出 **world 坐标**（[0,width]×[0,height]，随内容大小）。view 侧持一个 `Viewport`
 * `{x,y,scale}` 做 pan（平移 x/y）/ zoom（缩放 scale）；world↔screen 变换见 `worldToScreen`
 * /`screenToWorld`。布局只算一次（load overview 时），pan/zoom 只改 viewport → 重画，便宜。
 */

import type { Overview } from "./types";

// === 基础几何 ===

export interface Vec2 {
  x: number;
  y: number;
}

/** 视口：world→screen 的仿射变换 `screen = world * scale + {x,y}`。 */
export interface Viewport {
  /** 平移（screen 像素） */
  x: number;
  y: number;
  /** 缩放（screen 像素 / world 单位） */
  scale: number;
}

/** zoom 下限/上限（防缩没了/缩爆）。 */
export const MIN_SCALE = 0.05;
export const MAX_SCALE = 8;

export function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

/** world 点 → screen 点。 */
export function worldToScreen(p: Vec2, vp: Viewport): Vec2 {
  return { x: p.x * vp.scale + vp.x, y: p.y * vp.scale + vp.y };
}

/** screen 点 → world 点（worldToScreen 的逆）。 */
export function screenToWorld(p: Vec2, vp: Viewport): Vec2 {
  return { x: (p.x - vp.x) / vp.scale, y: (p.y - vp.y) / vp.scale };
}

/**
 * 以某 screen 点为锚缩放：缩放后该 screen 点下的 world 点不变（滚轮缩放手感）。
 * `factor` >1 放大、<1 缩小；结果 scale 被 clamp 到 [MIN_SCALE, MAX_SCALE]。
 */
export function zoomAt(vp: Viewport, sx: number, sy: number, factor: number): Viewport {
  const newScale = clamp(vp.scale * factor, MIN_SCALE, MAX_SCALE);
  // 锚点 world 坐标（用旧 scale 反算），缩放后仍要落回同一 screen 点：
  //   sx = wx * newScale + x'  →  x' = sx - wx * newScale
  const wx = (sx - vp.x) / vp.scale;
  const wy = (sy - vp.y) / vp.scale;
  return { scale: newScale, x: sx - wx * newScale, y: sy - wy * newScale };
}

/**
 * 让 world 边界 [0,worldW]×[0,worldH] 居中铺进 screen（留 pad 像素边）。空/退化时回退到
 * 居中 scale=1。用于 open 时初始视口 + 「适配」按钮。
 */
export function fitViewport(
  worldW: number,
  worldH: number,
  screenW: number,
  screenH: number,
  pad = 48,
): Viewport {
  if (worldW <= 0 || worldH <= 0 || screenW <= 0 || screenH <= 0) {
    return { x: screenW / 2, y: screenH / 2, scale: 1 };
  }
  const raw = Math.min((screenW - 2 * pad) / worldW, (screenH - 2 * pad) / worldH);
  const scale = clamp(raw, MIN_SCALE, MAX_SCALE);
  return {
    x: (screenW - worldW * scale) / 2,
    y: (screenH - worldH * scale) / 2,
    scale,
  };
}

// === 布局产物 ===

export interface FileBubble {
  file: string;
  score: number;
  symbols: number;
  /** 所属子系统标签（未归类为 UNCATEGORIZED_LABEL）。 */
  subsystem: string;
  /** 是否是入口点文件（entry_points 命中）。 */
  isEntry: boolean;
  /** 填充色相（0-360，随子系统确定性分配）。 */
  hue: number;
  /** world 圆心 + 半径。 */
  x: number;
  y: number;
  r: number;
}

export interface SubsystemRegion {
  label: string;
  hue: number;
  fileCount: number;
  /** 绘制盒（world，左上角 + 尺寸），含顶部标签留白。 */
  boxX: number;
  boxY: number;
  boxW: number;
  boxH: number;
  /** 标签锚点（world，盒内左上）。 */
  labelX: number;
  labelY: number;
}

export interface PanoramaLayout {
  bubbles: FileBubble[];
  regions: SubsystemRegion[];
  /** world 边界（0..width, 0..height）。 */
  width: number;
  height: number;
}

export interface LayoutOptions {
  minRadius?: number;
  maxRadius?: number;
  /** 气泡间留白（world）。 */
  bubblePad?: number;
  /** 子系统盒内 padding（world）。 */
  regionPad?: number;
  /** 子系统盒之间的间隔（world）。 */
  regionGap?: number;
  /** 子系统盒顶部为标签预留的高度（world）。 */
  labelSpace?: number;
}

/** 未归类子系统的标签（脊柱文件不属于任何 subsystem 时归此）。 */
export const UNCATEGORIZED_LABEL = "· 未归类";

const DEFAULTS: Required<LayoutOptions> = {
  minRadius: 10,
  maxRadius: 46,
  bubblePad: 6,
  regionPad: 18,
  regionGap: 28,
  labelSpace: 26,
};

/** 黄金角，用于子系统色相确定性分散（相邻子系统颜色差异大）。 */
const GOLDEN_ANGLE = 137.508;

/**
 * 脊柱文件 → world 圆半径。sqrt 缩放：**圆面积**≈score（视觉上不会让高分文件半径爆炸）。
 * 所有分数相等（或单文件）时取中值半径。导出供单测。
 */
export function bubbleRadius(
  score: number,
  minScore: number,
  maxScore: number,
  minR: number,
  maxR: number,
): number {
  const span = maxScore - minScore;
  const norm = span > 0 ? clamp((score - minScore) / span, 0, 1) : 0.5;
  return minR + (maxR - minR) * Math.sqrt(norm);
}

/**
 * 计算全景布局：脊柱文件按子系统聚类成气泡，子系统 shelf 打包排布。**确定性**（同一
 * Overview 恒产同一坐标，便于测试与稳定视图）。
 *
 * 决策（见交付回报 ⑥）：气泡集 = `spine_files`（后端已按 token 预算裁剪、按重要性排名）。
 * 每个脊柱文件归入**第一个**包含它的 subsystem，都不含则归「未归类」。只渲染含 ≥1 脊柱文件
 * 的子系统区域（空区不画，诚实反映数据）。
 */
export function computeLayout(overview: Overview, opts?: LayoutOptions): PanoramaLayout {
  const o = { ...DEFAULTS, ...(opts ?? {}) };
  const spine = overview.spine_files ?? [];
  if (spine.length === 0) {
    return { bubbles: [], regions: [], width: 0, height: 0 };
  }

  // 文件 → 子系统标签（首个命中者胜；确定性按 subsystems 顺序）。
  const fileToSub = new Map<string, string>();
  for (const sub of overview.subsystems ?? []) {
    for (const f of sub.files) {
      if (!fileToSub.has(f)) fileToSub.set(f, sub.label);
    }
  }

  // 入口点文件集合（entry_points 是符号 id `file#name`，取 file 段）。
  const entryFiles = new Set<string>();
  for (const id of overview.entry_points ?? []) {
    const file = id.split("#")[0];
    if (file) entryFiles.add(file);
  }

  // 分数范围（半径归一化用）。
  let minScore = Infinity;
  let maxScore = -Infinity;
  for (const f of spine) {
    if (f.score < minScore) minScore = f.score;
    if (f.score > maxScore) maxScore = f.score;
  }

  // 按子系统分组（保持子系统首次出现顺序 → 确定性；未归类恒排最后）。
  const groupOrder: string[] = [];
  const groups = new Map<string, FileBubble[]>();
  for (const f of spine) {
    const label = fileToSub.get(f.file) ?? UNCATEGORIZED_LABEL;
    let arr = groups.get(label);
    if (!arr) {
      arr = [];
      groups.set(label, arr);
      groupOrder.push(label);
    }
    arr.push({
      file: f.file,
      score: f.score,
      symbols: f.symbols,
      subsystem: label,
      isEntry: entryFiles.has(f.file),
      hue: 0, // 下面按区分配
      r: bubbleRadius(f.score, minScore, maxScore, o.minRadius, o.maxRadius),
      x: 0,
      y: 0,
    });
  }

  // 未归类排到最后（其余保持出现序）。
  groupOrder.sort((a, b) => {
    const au = a === UNCATEGORIZED_LABEL ? 1 : 0;
    const bu = b === UNCATEGORIZED_LABEL ? 1 : 0;
    return au - bu;
  });

  // 每个子系统：局部打包（圆心相对簇心 0,0），得簇半径。
  interface Cell {
    label: string;
    hue: number;
    bubbles: FileBubble[]; // 已填 local x/y
    clusterR: number;
    cellW: number;
    cellH: number;
  }
  const cells: Cell[] = [];
  let hueIdx = 0;
  for (const label of groupOrder) {
    const arr = groups.get(label)!;
    const hue = (hueIdx * GOLDEN_ANGLE) % 360;
    hueIdx += 1;
    for (const b of arr) b.hue = hue;
    // 大圆先放（中心）→ 小圆填缝，簇更紧凑。
    arr.sort((a, b) => b.r - a.r);
    const local = packCluster(
      arr.map((b) => b.r),
      o.bubblePad,
    );
    let clusterR = 0;
    for (let i = 0; i < arr.length; i++) {
      arr[i].x = local[i].x;
      arr[i].y = local[i].y;
      clusterR = Math.max(clusterR, Math.hypot(local[i].x, local[i].y) + arr[i].r);
    }
    const cellW = 2 * (clusterR + o.regionPad);
    const cellH = 2 * (clusterR + o.regionPad) + o.labelSpace;
    cells.push({ label, hue, bubbles: arr, clusterR, cellW, cellH });
  }

  // shelf（行流）打包子系统盒：行宽超过目标就换行，得紧凑确定性网格。
  const maxCellW = cells.reduce((m, c) => Math.max(m, c.cellW), 0);
  const totalArea = cells.reduce((s, c) => s + c.cellW * c.cellH, 0);
  const targetRowW = Math.max(maxCellW, Math.sqrt(totalArea) * 1.7);

  const regions: SubsystemRegion[] = [];
  const bubbles: FileBubble[] = [];
  let curX = 0;
  let curY = 0;
  let rowMaxH = 0;
  let worldW = 0;
  for (const c of cells) {
    if (curX > 0 && curX + c.cellW > targetRowW) {
      // 换行
      curY += rowMaxH + o.regionGap;
      curX = 0;
      rowMaxH = 0;
    }
    const boxX = curX;
    const boxY = curY;
    // 簇心：盒内、标签留白之下居中。
    const cx = boxX + o.regionPad + c.clusterR;
    const cy = boxY + o.labelSpace + o.regionPad + c.clusterR;
    for (const b of c.bubbles) {
      bubbles.push({ ...b, x: cx + b.x, y: cy + b.y });
    }
    regions.push({
      label: c.label,
      hue: c.hue,
      fileCount: c.bubbles.length,
      boxX,
      boxY,
      boxW: c.cellW,
      boxH: c.cellH,
      labelX: boxX + o.regionPad,
      labelY: boxY + o.labelSpace * 0.66,
    });
    curX += c.cellW + o.regionGap;
    worldW = Math.max(worldW, curX - o.regionGap);
    rowMaxH = Math.max(rowMaxH, c.cellH);
  }
  const worldH = curY + rowMaxH;

  return { bubbles, regions, width: worldW, height: worldH };
}

/**
 * 确定性圆打包：把一组半径（**已按降序传入**）从中心向外螺旋铺开、互不重叠（含 pad）。
 * 返回每个圆相对簇心 (0,0) 的坐标。O(n²)（每圆环扫找空位），n = 单子系统脊柱文件数（小）。
 * 导出供单测（验重叠）。
 */
export function packCluster(radii: number[], pad: number): Vec2[] {
  const placed: { x: number; y: number; r: number }[] = [];
  for (const r of radii) {
    if (placed.length === 0) {
      placed.push({ x: 0, y: 0, r });
      continue;
    }
    const slot = findSlot(r, placed, pad);
    placed.push({ x: slot.x, y: slot.y, r });
  }
  return placed.map((p) => ({ x: p.x, y: p.y }));
}

function overlaps(
  x: number,
  y: number,
  r: number,
  placed: { x: number; y: number; r: number }[],
  pad: number,
): boolean {
  for (const p of placed) {
    const minDist = p.r + r + pad;
    const dx = p.x - x;
    const dy = p.y - y;
    if (dx * dx + dy * dy < minDist * minDist) return true;
  }
  return false;
}

/** 从中心向外一圈圈找第一个不重叠的落点（确定性）。 */
function findSlot(
  r: number,
  placed: { x: number; y: number; r: number }[],
  pad: number,
): Vec2 {
  // 中心可用就用中心。
  if (!overlaps(0, 0, r, placed, pad)) return { x: 0, y: 0 };
  const ringStep = Math.max(2, r * 0.5);
  const maxExtent = placed.reduce((m, p) => Math.max(m, Math.hypot(p.x, p.y) + p.r), 0);
  const limit = maxExtent + 4 * r + ringStep * 4;
  for (let ring = ringStep; ring <= limit; ring += ringStep) {
    // 环周长 / 步长 → 该环采样点数（至少 8）。
    const count = Math.max(8, Math.floor((2 * Math.PI * ring) / ringStep));
    for (let i = 0; i < count; i++) {
      const a = (i / count) * 2 * Math.PI;
      const x = Math.cos(a) * ring;
      const y = Math.sin(a) * ring;
      if (!overlaps(x, y, r, placed, pad)) return { x, y };
    }
  }
  // 兜底：怎么都放不下就摆到最右外侧（不重叠也不至于压别人）。
  return { x: maxExtent + r + pad, y: 0 };
}

// === 命中测试 ===

/**
 * screen 点 → 命中的气泡（无则 null）。用 viewport 把每个 world 圆映射到 screen 再测距；
 * 命中多个（圆重叠边缘）取圆心最近者（视觉上"点到的"那个）。
 */
export function hitTest(
  sx: number,
  sy: number,
  bubbles: readonly FileBubble[],
  vp: Viewport,
): FileBubble | null {
  let best: FileBubble | null = null;
  let bestD = Infinity;
  for (const b of bubbles) {
    const cx = b.x * vp.scale + vp.x;
    const cy = b.y * vp.scale + vp.y;
    const r = b.r * vp.scale;
    const dx = sx - cx;
    const dy = sy - cy;
    const d = Math.hypot(dx, dy);
    if (d <= r && d < bestD) {
      best = b;
      bestD = d;
    }
  }
  return best;
}

// === 覆盖信号文案（诚实性硬要求）===

/**
 * 覆盖信号 banner 文案。`unresolved_calls>0 || parse_errors>0` 时返回一句诚实提示，
 * 否则返回 null（无 banner）。静态分析已知缺口，别让全景图"看起来总是完整"。
 */
export function coverageBanner(o: {
  unresolved_calls: number;
  parse_errors: number;
}): string | null {
  const uc = o.unresolved_calls ?? 0;
  const pe = o.parse_errors ?? 0;
  if (uc <= 0 && pe <= 0) return null;
  const parts: string[] = [];
  if (uc > 0) parts.push(`${uc} 处调用未解析`);
  if (pe > 0) parts.push(`${pe} 文件解析失败`);
  return `覆盖不全：${parts.join("、")}（静态分析已知缺口）`;
}

/**
 * F70：把 `panorama_touching` 返回的符号 id（`file#name`）映射回**文件段集合**（去重）。
 * 全景图画的是文件级气泡，故高亮按文件粒度。与 `computeLayout` 里 entry_points 的 id→file
 * 派生（`id.split("#")[0]`）同款约定——core 若改 SymbolId 格式两处一起坏，风险已存在非新增。
 */
export function touchedFilesFromIds(ids: string[]): Set<string> {
  const files = new Set<string>();
  for (const id of ids) {
    const file = id.split("#")[0];
    if (file) files.add(file);
  }
  return files;
}

/**
 * F70：一组高亮文件里有几个在图上真有气泡（脊柱文件）。图例「本会话碰了 total 个文件，
 * 图上高亮 shown 个（其余非脊柱、未画）」的 shown——诚实呈现，别让"看着全高亮了"骗人。
 */
export function countShown(bubbles: FileBubble[], files: Set<string>): number {
  let n = 0;
  for (const b of bubbles) if (files.has(b.file)) n += 1;
  return n;
}
