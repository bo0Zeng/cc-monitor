/**
 * 计划页的合成世界：本机一个工作区两片（软件 `ledger` · 研究 `export-survey`）＋ devbox 一个工作区一片（`site`）。
 * 盘上是 pb 原样的 dump 与 pb 替身（`../disk/plan.ts`），计划那一族帧命令全由真后端答；名字与正文全是合成的中性例子。
 */
import type { SessionSpec, World } from "./types";
import { Convo } from "./records";
import { LOCAL, session, sidOf, smallConvo } from "./world";
import { HOME, type MachineDisk } from "../disk";
import { dumpCells, putPb, putWorkspace, type PbStandIn } from "../disk/plan";

/** 计划用到的几个会话（会话号 11 起，不撞默认世界）。 */
export const PLAN_SIDS = { lead: sidOf(11), parse: sidOf(12), base: sidOf(13), export: sidOf(14), site: sidOf(15) };
/** 导入解析那个会话派出的子 agent（接手 D-1 那一块）。 */
const SUB_ID = "a0b1c2d3e4f5a6b7c";
/** 签过一次、对不上这台任何会话的 id。 */
const STRANGER = "7f3e9a10-0000-4000-8000-00000000c41d";

/** 两个工作区（相对家目录）。 */
export const LEDGER_REL = "work/ledger";
export const SITE_REL = "src/site";
export const LEDGER_WS = `${HOME}/${LEDGER_REL}`;
const SITE_WS = `${HOME}/${SITE_REL}`;

/** 导入解析：说完一句、派出子 agent 去写，自己停在一个提问上（pidfile 说在等回答）。 */
function parseConvo(): Convo {
  const c = new Convo(PLAN_SIDS.parse, LEDGER_WS, "2026-10-01T10:30:00Z");
  c.title("导入解析");
  c.user("导入解析这一块交给你。");
  const kid = c.child(SUB_ID);
  kid.user("读入、校验与编码探测，照 A2 的列定义。");
  kid.say("编码探测还在写。", 12_000);
  c.tool("Task", { description: "导入解析", prompt: "读入、校验与编码探测，照 A2 的列定义。", subagent_type: "general-purpose" }, null, { child: kid, launched: true });
  c.say("编码探测还在写。", 20_000);
  return c.endAt(Date.now() - 300_000);
}

export function planSessions(): SessionSpec[] {
  const cwd = LEDGER_WS;
  return [
    session(11, LOCAL, cwd, smallConvo(PLAN_SIDS.lead, cwd, "总负责", "这一片按计划往下做。", "站到「导出 CSV」了。"), { activity: "working" }),
    session(12, LOCAL, cwd, parseConvo(), { activity: "needs_you", waitingFor: "AskUserQuestion", waitingSinceMs: Date.now() - 300_000 }),
    session(13, LOCAL, cwd, smallConvo(PLAN_SIDS.base, cwd, "工程底座", "工程底座这一块交给你。", "交回了，等收下。"), { activity: "idle" }),
    session(14, LOCAL, cwd, smallConvo(PLAN_SIDS.export, cwd, "导出实现", "导出实现这一块交给你。", "先停在这里。"), { activity: "idle", ended: true }),
    session(15, "devbox", SITE_WS, smallConvo(PLAN_SIDS.site, SITE_WS, "站点总负责", "这一片按计划往下做。", "部署好了。"), { activity: "idle" }),
  ];
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
  { name: "能力", edge_words: { to: "指向", with: "连着", after: null, replaces: null } },
  { name: "模块", edge_words: { to: "指向", with: "实现", after: null, replaces: null } },
  { name: "测试", edge_words: { to: "指向", with: "测", after: null, replaces: null } },
];
const SOFT_PHASES = [
  { name: "定架构", does: "写图", marks: [] },
  { name: "审计", does: "对账", marks: [] },
  { name: "执行", does: "写代码", marks: [] },
  { name: "回看", does: "看全局", marks: ["看全局"] },
];
const NO_CHECK = { unreadable: [], red: [], undecidable: [] };

export interface PlanWorldOpts {
  /** 本机那个工作区头一次之后 `ledger` 那一片读不成（页上给上一次那一份 ＋ 原因）。 */
  sliceStale?: boolean;
  /** pb 没装 / 太旧（没有 dump）。 */
  pb?: "missing" | "old";
  /** 一个工作区都没有。 */
  empty?: boolean;
  /** 自动接着做那一下 pb 拒。 */
  autoRefuse?: boolean;
  /** 本机读计划要多久（演首次加载）。 */
  slowMs?: number;
  /** 顶块走到看全局（需手动多一条「整片做完 · 需过目」）。 */
  topDone?: boolean;
  /** 本机那个工作区多一片新开的、还没有图的。 */
  fresh?: boolean;
  /** ledger 那一片只写文档（仓库还没接过来）。 */
  docsOnly?: boolean;
}

/** 退回之后 pb 那一侧加的子格（「已落地」两张图：`cell` ⇒ 检查脚本底下一格 · `top` ⇒ 顶层一格）。 */
export type Landed = "cell" | "top";

function ledgerSlice(o: PlanWorldOpts, landed?: Landed): Record<string, unknown> {
  const P = PLAN_SIDS;
  const checkFiles = [{ path: "scripts/check.sh", state: "在", note: "0.9 KB" }, { path: "pyproject.toml", state: "在", note: "1.2 KB" }];
  const checkSigns = [{ at: today(2, 6), by: P.base, reason: "ruff · mypy · pytest 一条命令跑完。" }];
  const cells = dumpCells([
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
      view: "── A2-1-1 解析模块（站在这一格时 pb 印给 agent 的一段）\n> id: A2-1-1 | kind: 模块 | with: A2-1\n> 文件: ledger/import_csv.py\n\n要做成什么样：见上。\n下一步：pb sign A2-1-1 <理由>",
    },
    { id: "A2-1-2", title: "解析单元测试", kind: "测试", status: "做完了", owner: SUB_ID, edges: { with: ["A2-1-1"] }, files: [{ path: "tests/test_import.py", state: "在", note: "2.8 KB" }], signs: [{ at: today(2, 8), by: SUB_ID, reason: "tests/test_import.py 41 passed；变异 22/22 被杀。" }] },
    { id: "A2-1-3", title: "编码探测", kind: "模块", status: "没做完", why: "没签", owner: SUB_ID, edges: { with: ["A2-1"] }, body: "认出 GBK 与 UTF-8，别的编码直接报错。", files: [{ path: "ledger/encoding.py", state: "缺", note: null }] },
    { id: "A2-2", title: "导入验收测试", kind: "测试", status: "没做完", why: "没签", owner: P.lead, edges: { with: ["A2"], after: ["A2-1-1"] }, files: [{ path: "tests/test_accept_import.py", state: "空", note: "0 字节" }] },
    { id: "A3", title: "导出 CSV", kind: "能力", status: "没做完", why: "里面 0/2 做完了", children: ["A3-1", "A3-2"], owner: P.lead, body: "把对过账的行写回 CSV，列序照 A2。" },
    { id: "A3-1", title: "导出实现", kind: "模块", status: "没做完", why: "没签", owner: P.export, edges: { with: ["A3"] }, files: [{ path: "ledger/export_csv.py", state: "坏", note: "第 12 行语法错" }] },
    { id: "A3-2", title: "导出验收测试", kind: "测试", status: "没做完", why: "没签", owner: P.lead, edges: { with: ["A3"] }, view: false },
    { id: "A4", title: "工程底座", kind: "能力", status: "没做完", why: "等上一级收下", children: ["A4-1", "A4-2"], owner: P.base, body: "检查脚本与打包配置，一条命令跑全部检查。" },
    landed === "cell"
      ? { id: "A4-1", title: "检查脚本", kind: "模块", status: "没做完", why: "里面 0/1 做完了", children: ["A4-1-1"], owner: P.base, signer: P.lead, files: checkFiles, signs: checkSigns }
      : { id: "A4-1", title: "检查脚本", kind: "模块", status: "做完了", owner: P.base, signer: P.lead, files: checkFiles, signs: checkSigns },
    ...(landed === "cell" ? [{ id: "A4-1-1", title: "收集 0 条时退 5", kind: "测试", status: "没做完" as const, why: "没签", owner: P.base }] : []),
    { id: "A4-2", title: "打包配置", kind: "模块", status: "做完了", owner: P.base, signs: [{ at: today(2, 6), by: STRANGER, reason: "pyproject 元数据 ＋ pytest ＋ coverage(fail_under=85)，tomllib 解析通过。" }] },
    { id: "A5", title: "网页导入", kind: "能力", status: "不做了", owner: P.lead, body: "从网页表单导入账单。" },
    { id: "A6", title: "网页导入（换成命令行）", kind: "能力", status: "不做了", owner: P.lead, edges: { replaces: ["A5"] } },
    ...(landed === "top" ? [{ id: "A7", title: "导出与导入对拍", kind: "能力", status: "没做完" as const, why: "没签", owner: P.lead }] : []),
  ]);
  return {
    name: "ledger",
    domain: "软件",
    path: "repos/ledger",
    ...(o.docsOnly ? { docs_only: true } : {}),
    kinds: SOFT_KINDS,
    phases: SOFT_PHASES,
    done: false,
    top: ["A1", "A2", "A3", "A4", "A5", ...(landed === "top" ? ["A7"] : [])],
    blocks: [
      { id: "project", cells: ["project"], dir: ".planned-build/ledger", owner: P.lead, phase: o.topDone ? "回看" : "执行", at: "A3", row: true },
      { id: "D-1", cells: ["A2-1"], dir: ".planned-build/ledger/D-1", owner: SUB_ID, phase: "审计", at: "A2-1-3", row: true },
      { id: "D-2", cells: ["A3-1"], dir: ".planned-build/ledger/D-2", owner: P.export, phase: "执行", at: null, row: true },
      { id: "D-3", cells: ["A4"], dir: ".planned-build/ledger/D-3", owner: P.base, phase: "回看", at: null, row: true },
    ],
    check: { unreadable: [], red: [{ rule: "悬空", what: "A4-1 指着 A9", block: "D-3", fix: "改成一个在的编号" }], undecidable: [] },
    cells,
    archived: [{ id: "A5", title: "网页导入", kind: "能力", replaced_by: "A6" }],
  };
}

function surveySlice(): Record<string, unknown> {
  const lead = PLAN_SIDS.lead;
  return {
    name: "export-survey",
    domain: "研究",
    path: "export-survey",
    kinds: [
      { name: "问题", edge_words: { to: "指向", with: "答", after: null, replaces: null } },
      { name: "来源", edge_words: { to: "指向", with: "凭", after: null, replaces: null } },
      { name: "事实", edge_words: { to: "指向", with: "凭", after: null, replaces: null } },
      { name: "推断", edge_words: { to: "指向", with: "推自", after: null, replaces: null } },
    ],
    phases: SOFT_PHASES,
    done: false,
    top: ["Q1", "Q2", "Q3"],
    blocks: [{ id: "project", cells: ["project"], dir: ".planned-build/export-survey", owner: lead, phase: "执行", at: "Q3", row: true }],
    check: NO_CHECK,
    cells: dumpCells([
      { id: "Q1", title: "现有导出格式都有哪些", kind: "问题", status: "做完了", owner: lead, signs: [{ at: today(1, 30), by: lead, reason: "列了 5 种。" }] },
      { id: "Q2", title: "各家银行接受哪种", kind: "问题", status: "做完了", owner: lead, signs: [{ at: today(1, 40), by: lead, reason: "查了 3 家。" }] },
      { id: "Q3", title: "推荐哪一种", kind: "推断", status: "没做完", why: "没签", owner: lead },
    ]),
    archived: [],
  };
}

/** 新开的一片：pb 只记了名字与领域，还没有一格（顶块等某个会话 claim 才建）。 */
function freshSlice(): Record<string, unknown> {
  return { name: "invoice", domain: "软件", path: "invoice", kinds: SOFT_KINDS, phases: SOFT_PHASES, done: false, top: [], blocks: [], check: NO_CHECK, cells: [], archived: [] };
}

function siteSlice(): Record<string, unknown> {
  const s = PLAN_SIDS.site;
  return {
    name: "site",
    domain: "软件",
    path: "site",
    kinds: SOFT_KINDS,
    phases: SOFT_PHASES,
    done: false,
    top: ["B1", "B2", "B3", "B4", "B5"],
    blocks: [{ id: "project", cells: ["project"], dir: ".planned-build/site", owner: s, phase: "执行", at: null, row: true }],
    check: NO_CHECK,
    cells: dumpCells([
      { id: "B1", title: "首页", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 20, 2), by: s, reason: "上线。" }] },
      { id: "B2", title: "文章列表", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 21, 0), by: s, reason: "分页好了。" }] },
      { id: "B3", title: "评论", kind: "能力", status: "没做完", why: "里面 1/2 做完了", children: ["B3-1", "B3-2"], owner: s },
      { id: "B3-1", title: "评论接口", kind: "模块", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 22, 0), by: s, reason: "接口好了。" }] },
      { id: "B3-2", title: "评论审核", kind: "模块", status: "没做完", why: "没签", owner: s },
      { id: "B4", title: "部署", kind: "能力", status: "做完了", owner: s, signs: [{ at: daysAgo(1, 23, 0), by: s, reason: "部署脚本跑通。" }] },
      { id: "B5", title: "订阅", kind: "能力", status: "没做完", why: "没签", owner: s },
    ]),
    archived: [],
  };
}

const dumpOf = (workspace: string, repo: string, slices: Record<string, unknown>[]): Record<string, unknown> => ({ pb: "0.2.0", shape: 1, workspace, repo, auto: false, slices });

/** 本机那个工作区此刻的 dump（`landed` ⇒ 退回之后 pb 那一侧加了一格：场景开页之后改写那一份）。 */
export function ledgerDump(o: PlanWorldOpts, landed?: Landed): Record<string, unknown> {
  return dumpOf(LEDGER_WS, "ledger", [ledgerSlice(o, landed), surveySlice(), ...(o.fresh ? [freshSlice()] : [])]);
}

/** 计划页的世界：默认世界 ＋ 计划用到的几个会话 ＋ 各台装着 pb 替身（`pb: missing` ⇒ 不装）＋ 本机与 devbox 各一个工作区。 */
export function planWorld(base: World, o: PlanWorldOpts = {}): World {
  const disk: Record<string, MachineDisk> = { ...base.disk };
  const edit = (m: string, f: (d: MachineDisk) => void): void => {
    const d = { ...disk[m], files: { ...disk[m].files } };
    f(d);
    disk[m] = d;
  };
  if (o.pb !== "missing") {
    for (const m of base.machines) {
      const cfg: PbStandIn = {
        ...(o.pb === "old" ? { dumpRc: 4 } : {}),
        ...(o.autoRefuse && m === LOCAL ? { refuse: { continue: "这个工作区还没有顶块" } } : {}),
        ...(o.sliceStale && m === LOCAL ? { brokenAfterFirst: { slice: "ledger", said: "图.md 第 142 行：元行缺 kind" } } : {}),
      };
      edit(m, (d) => putPb(d, cfg));
    }
  }
  if (!o.empty) {
    edit(LOCAL, (d) => putWorkspace(d, LEDGER_REL, ledgerDump(o)));
    edit("devbox", (d) => putWorkspace(d, SITE_REL, dumpOf(SITE_WS, "site", [siteSlice()])));
  }
  return {
    ...base,
    disk,
    sessions: [...base.sessions, ...planSessions()],
    opDelayMs: o.slowMs ? { ...(base.opDelayMs ?? {}), "plan-list": o.slowMs } : base.opDelayMs,
  };
}
