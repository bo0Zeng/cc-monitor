/**
 * 计划页的合成世界：本机一个工作区两片（软件 `ledger` · 研究 `export-survey`）＋ devbox 一个工作区一片（`site`）。
 * 形状照后端 `plan-read` 的成品（`src/backend/plan/product.rs`）；名字与正文全是合成的中性例子。
 */
import type { OpHandler, SessionSpec, World } from "./types";
import { Refuse } from "./types";
import { LOCAL, session, sidOf, smallConvo } from "./world";

/** 计划用到的几个会话（会话号 11 起，不撞默认世界）。 */
export const PLAN_SIDS = { lead: sidOf(11), parse: sidOf(12), base: sidOf(13), export: sidOf(14), site: sidOf(15) };
const SUB_ID = "a0b1c2d3e4f5a6b7c";
const STRANGER = "7f3e9a10-0000-4000-8000-00000000c41d";

export function planSessions(): SessionSpec[] {
  const cwd = "/home/user/work/ledger";
  return [
    session(11, LOCAL, cwd, smallConvo(PLAN_SIDS.lead, cwd, "总负责", "这一片按计划往下做。", "站到「导出 CSV」了。"), { activity: "working" }),
    session(12, "devbox", cwd, smallConvo(PLAN_SIDS.parse, cwd, "导入解析", "导入解析这一块交给你。", "编码探测还在写。"), { activity: "needs_you", waitingFor: "AskUserQuestion", waitingSinceMs: Date.now() - 300_000 }),
    session(13, LOCAL, cwd, smallConvo(PLAN_SIDS.base, cwd, "工程底座", "工程底座这一块交给你。", "交回了，等收下。"), { activity: "idle" }),
    session(14, LOCAL, cwd, smallConvo(PLAN_SIDS.export, cwd, "导出实现", "导出实现这一块交给你。", "先停在这里。"), { activity: "idle", ended: true }),
    session(15, "devbox", "/srv/src/site", smallConvo(PLAN_SIDS.site, "/srv/src/site", "站点总负责", "这一片按计划往下做。", "部署好了。"), { activity: "idle" }),
  ];
}

type Who = Record<string, unknown> | null;

function who(id: string | null): Who {
  if (id === null) return null;
  const live = (sid: string, activity: string | null, needs: string | null = null): Who => ({ id, kind: "session", sid, alive: true, activity, needs });
  if (id === PLAN_SIDS.lead) return live(id, "working");
  if (id === PLAN_SIDS.parse) return live(id, "needs_you", "answer");
  if (id === PLAN_SIDS.base) return live(id, "idle");
  if (id === PLAN_SIDS.site) return live(id, "idle");
  if (id === PLAN_SIDS.export) return { id, kind: "session", sid: id, alive: false, activity: null, needs: null };
  if (id === SUB_ID) return { id, kind: "subagent", sid: PLAN_SIDS.parse, alive: true, activity: "needs_you", needs: "answer" };
  return { id, kind: "unknown", sid: null, alive: false, activity: null, needs: null };
}

const STATUS_CODE: Record<string, string> = { 做完了: "done", 没做完: "open", 不做了: "dropped" };

/** 话里提到的编号的位置（合成数据自己找；真数据是 pb 给的）。 */
function refsIn(text: string, ids: ReadonlySet<string>): { start: number; end: number; ids: string[] }[] {
  const chars = Array.from(text);
  const out = [];
  const re = /[A-Z]\d+(?:-\d+)*/g;
  const joined = chars.join("");
  for (const m of joined.matchAll(re)) {
    if (!ids.has(m[0])) continue;
    const start = Array.from(joined.slice(0, m.index)).length;
    out.push({ start, end: start + Array.from(m[0]).length, ids: [m[0]] });
  }
  return out;
}

interface CellIn {
  id: string;
  title: string;
  kind: string;
  status: "做完了" | "没做完" | "不做了";
  why?: string;
  body?: string;
  children?: string[];
  edges?: Partial<Record<"to" | "with" | "after" | "replaces", string[]>>;
  files?: { path: string; state: string; note: string | null }[];
  signs?: { at: string; by: string; reason: string }[];
  owner?: string;
}

function whyCode(why: string | undefined): unknown {
  if (!why) return null;
  if (why === "没签") return { kind: "nosign" };
  if (why === "等上一级收下") return { kind: "upper" };
  const m = /^里面 (\d+)\/(\d+) 做完了$/.exec(why);
  return m ? { kind: "inside", done: Number(m[1]), of: Number(m[2]) } : null;
}

const FILE_CODE: Record<string, string> = { 在: "ok", 缺: "missing", 空: "empty", 坏: "broken" };

function build(cells: CellIn[]): Record<string, unknown>[] {
  const ids = new Set(cells.map((c) => c.id));
  const parentOf = new Map<string, string>();
  for (const c of cells) for (const k of c.children ?? []) parentOf.set(k, c.id);
  const pointed = new Map<string, Record<string, string[]>>();
  const byPath = new Map<string, string[]>();
  for (const c of cells) {
    for (const [k, ts] of Object.entries(c.edges ?? {})) for (const t of ts) {
      const p = pointed.get(t) ?? { to: [], with: [], after: [], replaces: [] };
      p[k].push(c.id);
      pointed.set(t, p);
    }
    for (const f of c.files ?? []) byPath.set(f.path, [...(byPath.get(f.path) ?? []), c.id]);
  }
  return cells.map((c) => ({
    id: c.id,
    title: c.title,
    kind: c.kind,
    body: c.body ?? null,
    parent: parentOf.get(c.id) ?? null,
    children: c.children ?? [],
    edges: { to: [], with: [], after: [], replaces: [], ...c.edges },
    pointedBy: pointed.get(c.id) ?? { to: [], with: [], after: [], replaces: [] },
    files: (c.files ?? []).map((f) => ({ ...f, stateCode: FILE_CODE[f.state] ?? null, alsoBy: (byPath.get(f.path) ?? []).filter((x) => x !== c.id) })),
    status: c.status,
    statusCode: STATUS_CODE[c.status],
    why: c.why ?? null,
    whyCode: whyCode(c.why),
    signs: (c.signs ?? []).map((g) => ({ at: g.at, by: who(g.by), reason: g.reason, refs: refsIn(g.reason, ids) })),
    owner: who(c.owner ?? null),
    refs: { title: [], body: refsIn(c.body ?? "", ids) },
    hasView: true,
  }));
}

const today = (h: number, m: number): string => {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d.toISOString();
};
const daysAgo = (n: number, h: number, m: number): string => {
  const d = new Date();
  d.setDate(d.getDate() - n);
  d.setHours(h, m, 0, 0);
  return d.toISOString();
};

const SOFT_KINDS = [
  { name: "能力", edgeWords: { to: "指向", with: "连着", after: null, replaces: null } },
  { name: "模块", edgeWords: { to: "指向", with: "实现", after: null, replaces: null } },
  { name: "测试", edgeWords: { to: "指向", with: "测", after: null, replaces: null } },
];
const SOFT_PHASES = [
  { name: "定架构", does: "写图", marks: [] },
  { name: "审计", does: "对账", marks: [] },
  { name: "执行", does: "写代码", marks: [] },
  { name: "回看", does: "看全局", marks: ["看全局"] },
];

export function ledgerSlice(): Record<string, unknown> {
  const P = PLAN_SIDS;
  const cells = build([
    { id: "A1", title: "命令行入口", kind: "能力", status: "做完了", children: ["A1-1", "A1-2"], owner: P.lead, body: "一条命令跑完导入与导出，参数照 A2 与 A3 的约定。" },
    { id: "A1-1", title: "包与分发", kind: "模块", status: "做完了", owner: P.lead, files: [{ path: "pyproject.toml", state: "在", note: "1.2 KB" }], signs: [{ at: daysAgo(2, 23, 56), by: P.lead, reason: "装得上、入口命令跑得起来。" }] },
    { id: "A1-2", title: "入口端到端测试", kind: "测试", status: "做完了", owner: P.lead, edges: { with: ["A1-1"] }, signs: [{ at: daysAgo(2, 23, 56), by: P.lead, reason: "端到端 6 条全过。" }] },
    { id: "A2", title: "导入账单", kind: "能力", status: "没做完", why: "里面 0/2 做完了", children: ["A2-1", "A2-2"], owner: P.lead, body: "账单 CSV 读进来，逐行校验，坏行列出来不吞。列定义：日期 · 金额 · 摘要 · 对方。" },
    { id: "A2-1", title: "导入解析", kind: "模块", status: "没做完", why: "里面 2/3 做完了", children: ["A2-1-1", "A2-1-2", "A2-1-3"], owner: SUB_ID, edges: { with: ["A2"] }, body: "把 CSV 读成行对象，交给 A2-2 去验收。" },
    {
      id: "A2-1-1",
      title: "解析模块",
      kind: "模块",
      status: "做完了",
      owner: SUB_ID,
      edges: { with: ["A2-1"] },
      body: [
        "管什么：账单 CSV 的读入与逐行校验；照 A2 的列定义划边界。",
        "",
        "对外两样：",
        "",
        "- `parse(path) -> list[Row]`：表头不分大小写、允许 BOM；金额两位小数，多一位算错。",
        "- `main(argv) -> int`：成功打印行数返回 0；错了 stderr 一行、返回 2，不调 `sys.exit`。",
        "",
        "实现里定下的解读：空行跳过不计数；表头多出的列忽略；编码不对交给 A2-1-3。",
      ].join("\n"),
      files: [
        { path: "ledger/import_csv.py", state: "在", note: "4.1 KB" },
        { path: "ledger/rows.py", state: "在", note: "1.3 KB" },
      ],
      signs: [
        { at: today(1, 52), by: SUB_ID, reason: "读入与校验写完，单测 38 passed。" },
        { at: today(2, 8), by: SUB_ID, reason: "修了 _parse 里没还原的变异：len(cols) < 5 改成 != 5；22/22 被杀。对过 A2 的边界三条。" },
      ],
    },
    { id: "A2-1-2", title: "解析单元测试", kind: "测试", status: "做完了", owner: SUB_ID, edges: { with: ["A2-1-1"] }, files: [{ path: "tests/test_import.py", state: "在", note: "2.8 KB" }], signs: [{ at: today(2, 8), by: SUB_ID, reason: "tests/test_import.py 41 passed；变异 22/22 被杀。" }] },
    { id: "A2-1-3", title: "编码探测", kind: "模块", status: "没做完", why: "没签", owner: SUB_ID, edges: { with: ["A2-1"] }, body: "认出 GBK 与 UTF-8，别的编码直接报错。", files: [{ path: "ledger/encoding.py", state: "缺", note: null }] },
    { id: "A2-2", title: "导入验收测试", kind: "测试", status: "没做完", why: "没签", owner: P.lead, edges: { with: ["A2"], after: ["A2-1-1"] }, files: [{ path: "tests/test_accept_import.py", state: "空", note: "0 字节" }] },
    { id: "A3", title: "导出 CSV", kind: "能力", status: "没做完", why: "里面 0/2 做完了", children: ["A3-1", "A3-2"], owner: P.lead, body: "把对过账的行写回 CSV，列序照 A2。" },
    { id: "A3-1", title: "导出实现", kind: "模块", status: "没做完", why: "没签", owner: P.export, edges: { with: ["A3"] }, files: [{ path: "ledger/export_csv.py", state: "坏", note: "第 12 行语法错" }] },
    { id: "A3-2", title: "导出验收测试", kind: "测试", status: "没做完", why: "没签", owner: P.lead, edges: { with: ["A3"] } },
    { id: "A4", title: "工程底座", kind: "能力", status: "没做完", why: "等上一级收下", children: ["A4-1", "A4-2"], owner: P.base, body: "检查脚本与打包配置，一条命令跑全部检查。" },
    { id: "A4-1", title: "检查脚本", kind: "模块", status: "做完了", owner: P.base, files: [{ path: "scripts/check.sh", state: "在", note: "0.9 KB" }, { path: "pyproject.toml", state: "在", note: "1.2 KB" }], signs: [{ at: today(2, 6), by: P.base, reason: "ruff · mypy · pytest 一条命令跑完。" }] },
    { id: "A4-2", title: "打包配置", kind: "模块", status: "做完了", owner: P.base, signs: [{ at: today(2, 6), by: STRANGER, reason: "pyproject 元数据 ＋ pytest ＋ coverage(fail_under=85)，tomllib 解析通过。" }] },
    { id: "A5", title: "网页导入", kind: "能力", status: "不做了", owner: P.lead, body: "从网页表单导入账单。" },
    { id: "A6", title: "网页导入（换成命令行）", kind: "能力", status: "不做了", owner: P.lead, edges: { replaces: ["A5"] } },
  ]);
  return {
    name: "ledger",
    domain: "软件",
    current: true,
    error: null,
    stale: null,
    kinds: SOFT_KINDS,
    phases: SOFT_PHASES,
    done: false,
    top: ["A1", "A2", "A3", "A4", "A5"],
    progress: { done: 1, open: 3, dropped: 1 },
    blocks: [
      { id: "project", cells: ["project"], dir: ".planned-build/ledger", owner: who(P.lead), phase: "执行", at: "A3", row: true },
      { id: "D-1", cells: ["A2-1"], dir: ".planned-build/ledger/D-1", owner: who(SUB_ID), phase: "审计", at: "A2-1-3", row: true },
      { id: "D-2", cells: ["A3-1"], dir: ".planned-build/ledger/D-2", owner: who(P.export), phase: "执行", at: null, row: true },
      { id: "D-3", cells: ["A4"], dir: ".planned-build/ledger/D-3", owner: who(P.base), phase: "回看", at: null, row: true },
    ],
    check: { unreadable: [], red: [{ rule: "悬空", what: "A4-1 指着 A9", block: "D-3", fix: "改成一个在的编号" }], undecidable: [] },
    cells,
    archived: [{ id: "A5", title: "网页导入", kind: "能力", replacedBy: "A6" }],
    needs: null,
  };
}

function surveySlice(): Record<string, unknown> {
  const cells = build([
    { id: "Q1", title: "现有导出格式都有哪些", kind: "问题", status: "做完了", children: [], owner: PLAN_SIDS.lead, signs: [{ at: today(1, 30), by: PLAN_SIDS.lead, reason: "列了 5 种。" }] },
    { id: "Q2", title: "各家银行接受哪种", kind: "问题", status: "做完了", owner: PLAN_SIDS.lead, signs: [{ at: today(1, 40), by: PLAN_SIDS.lead, reason: "查了 3 家。" }] },
    { id: "Q3", title: "推荐哪一种", kind: "推断", status: "没做完", why: "没签", owner: PLAN_SIDS.lead },
  ]);
  return {
    name: "export-survey",
    domain: "研究",
    current: false,
    error: null,
    stale: null,
    kinds: [
      { name: "问题", edgeWords: { to: "指向", with: "答", after: null, replaces: null } },
      { name: "来源", edgeWords: { to: "指向", with: "凭", after: null, replaces: null } },
      { name: "事实", edgeWords: { to: "指向", with: "凭", after: null, replaces: null } },
      { name: "推断", edgeWords: { to: "指向", with: "推自", after: null, replaces: null } },
    ],
    phases: SOFT_PHASES,
    done: false,
    top: ["Q1", "Q2", "Q3"],
    progress: { done: 2, open: 1, dropped: 0 },
    blocks: [{ id: "project", cells: ["project"], dir: ".planned-build/export-survey", owner: who(PLAN_SIDS.lead), phase: "执行", at: "Q3", row: true }],
    check: { unreadable: [], red: [], undecidable: [] },
    cells,
    archived: [],
    needs: null,
  };
}

function siteSlice(): Record<string, unknown> {
  const s = PLAN_SIDS.site;
  const cells = build([
    { id: "B1", title: "首页", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 20, 2), by: s, reason: "上线。" }] },
    { id: "B2", title: "文章列表", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 21, 0), by: s, reason: "分页好了。" }] },
    { id: "B3", title: "评论", kind: "能力", status: "没做完", why: "里面 1/2 做完了", children: ["B3-1", "B3-2"], owner: s },
    { id: "B3-1", title: "评论接口", kind: "模块", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 22, 0), by: s, reason: "接口好了。" }] },
    { id: "B3-2", title: "评论审核", kind: "模块", status: "没做完", why: "没签", owner: s },
    { id: "B4", title: "部署", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 23, 0), by: s, reason: "部署脚本跑通。" }] },
    { id: "B5", title: "订阅", kind: "能力", status: "没做完", why: "没签", owner: s },
  ]);
  return {
    name: "site",
    domain: "软件",
    current: true,
    error: null,
    stale: null,
    kinds: SOFT_KINDS,
    phases: SOFT_PHASES,
    done: false,
    top: ["B1", "B2", "B3", "B4", "B5"],
    progress: { done: 3, open: 2, dropped: 0 },
    blocks: [{ id: "project", cells: ["project"], dir: ".planned-build/site", owner: who(s), phase: "执行", at: null, row: true }],
    check: { unreadable: [], red: [], undecidable: [] },
    cells,
    archived: [],
    needs: null,
  };
}

export const LEDGER_WS = "/home/user/work/ledger";
export const SITE_WS = "/srv/src/site";

/** 一个工作区的成品（`plan-read` 的回包）。 */
function readOf(origin: string, slices: Record<string, unknown>[], auto: boolean): Record<string, unknown> {
  return { pb: "0.2.0", workspace: origin === LOCAL ? LEDGER_WS : SITE_WS, repo: (slices[0] as { name: string }).name, auto, slices, rev: `rev-${origin}`, readAt: Date.now() - 60_000, stale: null };
}

export interface PlanWorldOpts {
  /** 本机那个工作区读不成的那一片：`ledger` 给上一次那一份 ＋ 原因。 */
  sliceStale?: boolean;
  /** pb 没装 / 太旧。 */
  pb?: "missing" | "old";
  /** 一片都没有。 */
  empty?: boolean;
  /** devbox 第一次答了、之后离线（页上留着第一次读到的那一份）。 */
  devboxDown?: boolean;
  /** 自动接着做那一下 pb 拒。 */
  autoRefuse?: boolean;
  /** 本机读计划要多久（演首次加载）。 */
  slowMs?: number;
}

export function planOps(o: PlanWorldOpts = {}): Record<string, OpHandler> {
  const ledger = (): Record<string, unknown> => {
    const sl = ledgerSlice();
    if (o.sliceStale) sl.stale = { said: "图.md 第 142 行：元行缺 kind", since: Date.now() - 5 * 60_000 };
    return sl;
  };
  let auto = false;
  let devboxAsked = 0;
  const reads = (origin: string): Record<string, unknown> => (origin === LOCAL ? readOf(LOCAL, [ledger(), surveySlice()], auto) : readOf(origin, [siteSlice()], false));
  const summary = (sl: Record<string, unknown>): Record<string, unknown> => ({ name: sl.name, domain: sl.domain, current: sl.current, progress: sl.progress, error: sl.error, stale: sl.stale });
  return {
    "plan-list": (origin) => {
      if (o.pb === "missing") return { pb: { state: "missing", said: "未装 planned-build" }, workspaces: [] };
      if (o.pb === "old") return { pb: { state: "unsupported", said: "要更新 · 计划不可用\nplanned-build 缺 dump" }, workspaces: [] };
      if (o.empty) return { pb: { state: "ok", said: null }, workspaces: [] };
      if (origin === "devbox" && o.devboxDown && ++devboxAsked > 1) throw { err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] };
      if (origin !== LOCAL && origin !== "devbox") return { pb: { state: "ok", said: null }, workspaces: [] };
      const r = reads(origin);
      return { pb: { state: "ok", said: null }, workspaces: [{ workspace: r.workspace, repo: r.repo, auto: r.auto, rev: r.rev, stale: null, slices: (r.slices as Record<string, unknown>[]).map(summary) }] };
    },
    "plan-read": (origin) => reads(origin),
    "plan-cell-view": (_origin, req) => {
      const id = String(req.id);
      if (id === "A3-2") throw new Refuse("no_view", "无 agent 视角 · 重读计划后再看");
      return { view: `── ${id}（站在这一格时 pb 印给 agent 的一段）\n> id: ${id} | kind: 模块 | with: A2-1\n> 文件: ledger/import_csv.py\n\n要做成什么样：见上。\n下一步：pb sign ${id} <理由>` };
    },
    "plan-command": (_origin, req) => {
      if (req.cmd === "view") return { rc: 0, said: null, path: "/tmp/pb-读图-0000.html" };
      if (o.autoRefuse) throw new Refuse("refused", "pb 拒：这个工作区还没有顶块");
      auto = req.cmd === "continue";
      return { rc: 0, said: null, path: null };
    },
  };
}

/** 计划页的世界：默认世界 ＋ 计划用到的几个会话 ＋ 计划那几条命令。 */
export function planWorld(base: World, o: PlanWorldOpts = {}): World {
  const ops = planOps(o);
  return {
    ...base,
    sessions: [...base.sessions, ...planSessions()],
    ops: { ...base.ops, ...ops },
    opDelayMs: o.slowMs ? { ...(base.opDelayMs ?? {}), "plan-list": o.slowMs } : base.opDelayMs,
  };
}
