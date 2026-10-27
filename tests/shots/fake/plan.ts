/**
 * 计划页的合成世界：本机一个工作区两个仓库（软件 `ledger` · 研究 `export-survey`）＋ devbox 一个工作区一个仓库（`site`）。
 * 盘上是 pb 0.5 原样的 dump（形状 6）与 pb 替身（`../disk/plan.ts`），计划那一族帧命令全由真后端答；名字与正文全是合成的中性例子。
 */
import type { SessionSpec, World } from "./types";
import { Convo } from "./records";
import { LOCAL, session, sidOf, smallConvo } from "./world";
import { HOME, type MachineDisk } from "../disk";
import { dumpNodes, putPb, putWorkspace, type PbStandIn } from "../disk/plan";

/** 计划用到的几个会话（会话号 11 起，不撞默认世界）。 */
export const PLAN_SIDS = { lead: sidOf(11), parse: sidOf(12), base: sidOf(13), export: sidOf(14), site: sidOf(15) };
/** 导入解析那个会话派出的子 agent（负责 A2-1 那个权力范围）。 */
const SUB_ID = "a0b1c2d3e4f5a6b7c";
/** 写得动 A4-2 元数据、对不上这台任何会话的 id。 */
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

const labels = (w: string): Record<string, string> => ({ to: w, with: w, after: "排在后面", covers: w, file: "管文件", replaces: "替换", sets: "改状态" });
const kind = (name: string, w: string, graph: string, evidence: boolean, values: string[]): Record<string, unknown> => ({ name, graph, evidence, values, edge_labels: labels(w) });

/** 软件那张表（插件那一份的形状；阶段说明、规则只留真后端读得到的几条）。 */
const SOFT = {
  graphs: [
    { name: "架构", file: "图.md", lifetime: "permanent", copies: "per_scope", prefix: null, order_by: null, scope_by: null },
    { name: "排期", file: "排期.md", lifetime: "temporary", copies: "per_scope", prefix: "P", order_by: "to", scope_by: "covers" },
  ],
  kinds: [
    kind("能力", "连着", "架构", false, ["已签收", "decided"]),
    kind("模块", "实现", "架构", false, ["已签收", "decided"]),
    kind("测试", "测", "架构", false, ["decided"]),
    kind("验收记录", "签收", "架构", true, []),
    kind("原话", "连着", "架构", true, []),
    kind("规矩", "管", "架构", false, ["decided"]),
    kind("待定", "连着", "架构", false, ["decided"]),
    kind("排期", "指向", "排期", false, []),
  ],
  phases: [
    { name: "定架构", description: "先定结构" },
    { name: "审计", description: "审架构与实现" },
    { name: "开发", description: "写代码" },
    { name: "回看", description: "回看整体，决定下一步" },
  ],
  views: [{ name: "没签收的", query: "subtree=this kind=能力,模块 state!=已签收" }],
  rules: [
    { moments: ["update"], when: "when: state!=已签收 delegated=false | all_children: kind=能力,模块 state=已签收", effect: "add", arg: "已签收", line: 33 },
    { moments: ["update"], when: "when: state=已签收 delegated=false | any_child: kind=能力,模块 state!=已签收", effect: "remove", arg: "已签收", line: 34 },
    { moments: ["write"], when: "when: phase!=开发", effect: "deny", arg: "只在开发阶段改仓库、推远端、打 tag、发版", line: 41 },
  ],
};

/** 研究那张表（没有加 · 去状态的规则 ⇒ 不出进度）。 */
const RESEARCH = {
  graphs: [
    { name: "知识", file: "图.md", lifetime: "permanent", copies: "per_scope", prefix: null, order_by: null, scope_by: null },
    { name: "排期", file: "排期.md", lifetime: "temporary", copies: "per_scope", prefix: "P", order_by: "to", scope_by: "covers" },
  ],
  kinds: [kind("问题", "连着", "知识", false, ["decided"]), kind("来源", "连着", "知识", false, ["支持", "反对", "decided"]), kind("推断", "推自", "知识", false, ["支持", "反对", "decided"]), kind("佐证", "连着", "知识", true, [])],
  phases: [
    { name: "定题", description: "拆问题" },
    { name: "搜集", description: "找来源" },
    { name: "核证", description: "核来源与论断" },
  ],
  views: [],
  rules: [{ moments: ["update"], when: "when: value=支持,-支持 | source: kind!=佐证", effect: "deny", arg: "支持只由佐证写", line: 34 }],
};

const NO_CHECK = { read_errors: [], unparsed_nodes: [], failures: [], unknown: [] };

/** 交接说明（pb 原样那一句，不带认领码）。 */
const handoff = (repo: string, id: string, title: string): string =>
  id === "project"
    ? `你负责 ${repo} 的 project：先 pb claim project <认领码>，再 pb where。认领码只在人登记这个仓库与重发时印出来；丢了请人敲 \`pb reissue project\``
    : `你负责 ${repo} 的 ${id} ${title}：先 pb claim ${id} <认领码>，再 pb where。认领码只在划出与重发时印出来；丢了找上一级重发`;

const scope = (repo: string, id: string, title: string, assignee: string | null, phase: string, at: string | null): Record<string, unknown> => ({
  id,
  root_nodes: [id],
  dir: id === "project" ? `.planned-build/${repo}` : `.planned-build/${repo}/${id}`,
  assignee,
  first_claimed_at: null,
  created_at: null,
  phase,
  current_node: at,
  listed: true,
  handoff: handoff(repo, id, title),
  handoff_error: null,
});

export interface PlanWorldOpts {
  /** 本机那个工作区头一次之后 `ledger` 那个仓库读不成（页上给上一次那一份 ＋ 原因）。 */
  sliceStale?: boolean;
  /** pb 没装 / 太旧（没有 dump）。 */
  pb?: "missing" | "old";
  /** 一个工作区都没有。 */
  empty?: boolean;
  /** 自动继续那一下 pb 拒。 */
  autoRefuse?: boolean;
  /** 本机读计划要多久（演首次加载）。 */
  slowMs?: number;
  /** 顶层全签收（需手动多一条「整个仓库签收完 · 需过目」）。 */
  topDone?: boolean;
  /** 本机那个工作区多一个新登记的、还没有图的仓库。 */
  fresh?: boolean;
  /** ledger 只做计划（人还没 `pb adopt`）。 */
  docsOnly?: boolean;
  /** 本机有两组（注解文件里）：「导入组」里有导入解析、「底座组」里有工程底座；导出实现不在组里。 */
  groups?: boolean;
}

/** 两组（id 全局不重 · 名字 · 几时建 · 名字几时起，毫秒）与组员（会话注解上的 `groups`）。 */
function groupsNotes(): string {
  const at = Date.parse("2026-10-01T09:00:00Z");
  const group = (name: string, k: number): Record<string, unknown> => ({ name, named: at + k, made: at + k });
  const member = (g: string): Record<string, unknown> => ({ groups: [g], updatedAt: at });
  return JSON.stringify({
    version: 1,
    entries: { [PLAN_SIDS.parse]: member("g-import"), [PLAN_SIDS.base]: member("g-base") },
    groups: { "g-import": group("导入组", 1), "g-base": group("底座组", 2) },
  });
}

/** 退回之后 pb 那一侧加的子节点（「已落地」两张图：`cell` ⇒ 检查脚本底下一个 · `top` ⇒ 顶层一个）。 */
export type Landed = "cell" | "top";

function ledgerRepo(o: PlanWorldOpts, landed?: Landed): Record<string, unknown> {
  const P = PLAN_SIDS;
  const signed = o.topDone ? ["已签收"] : [];
  const inA21 = { childrenIn: "A2-1", assignee: SUB_ID, signer: SUB_ID, coveredBy: ["A8"] };
  const inA4 = { childrenIn: "A4", assignee: P.base, signer: P.base };
  const nodes = dumpNodes(
    [
      { id: "A1", title: "命令行入口", kind: "能力", state: ["已签收"], children: ["A1-1", "A1-2"], body: "一条命令跑完导入与导出，参数照 A2 与 A3 的约定。" },
      { id: "A1-1", title: "包与分发", kind: "模块", state: ["已签收"], edges: { with: ["A1"] }, files: [{ path: "pyproject.toml", check: "present", note: "1203 字" }], evidencedBy: ["A1-3"] },
      { id: "A1-2", title: "入口端到端测试", kind: "测试", edges: { with: ["A1-1"] }, files: [{ path: "tests/test_cli.py", check: "present", note: "2210 字" }] },
      { id: "A1-3", title: "签收包与分发", kind: "验收记录", edges: { with: ["A1-1"] }, body: "装得上、入口命令跑得起来；端到端 6 条全过。" },
      { id: "A2", title: "导入账单", kind: "能力", state: signed, children: ["A2-1", "A2-2"], coveredBy: ["A8"], body: "账单 CSV 读进来，逐行校验，坏行列出来不吞。列定义：日期 · 金额 · 摘要 · 对方。" },
      { id: "A2-1", title: "导入解析", kind: "模块", state: signed, children: ["A2-1-1", "A2-1-2", "A2-1-3", "A2-1-4"], edges: { with: ["A2"] }, ...inA21, signer: P.lead, body: "把 CSV 读成行对象，交给 A2-2 去验收。" },
      {
        id: "A2-1-1",
        title: "解析模块",
        kind: "模块",
        state: [["已签收", 2]],
        edges: { with: ["A2-1"] },
        ...inA21,
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
          { path: "ledger/import_csv.py", check: "present", note: "4102 字" },
          { path: "ledger/rows.py", check: "present", note: "1288 字" },
        ],
        evidencedBy: ["A2-1-5"],
        view: "── A2-1-1 解析模块（模块 · 已签收 2 · 元数据写在 .planned-build/ledger/A2-1/图.md）\n> id: A2-1-1 | kind: 模块 | with: A2-1\n> file: ledger/import_csv.py\n> state: 已签收 2\n\n管什么：见上。\nA2-1-1 底下下一个可用编号：A2-1-1-1",
      },
      { id: "A2-1-2", title: "解析单元测试", kind: "测试", edges: { with: ["A2-1-1"] }, ...inA21, files: [{ path: "tests/test_import.py", check: "present", note: "2840 字" }] },
      { id: "A2-1-3", title: "编码探测", kind: "模块", state: signed, edges: { with: ["A2-1"] }, ...inA21, body: "认出 GBK 与 UTF-8，别的编码直接报错。", files: [{ path: "ledger/encoding.py", check: "missing", note: "" }] },
      { id: "A2-1-4", title: "签收编码探测", kind: "验收记录", ...inA21, body: "GBK 与 UTF-8 各一份样本读得出，别的编码报错。", sets: [{ target: "A2-1-3", value: "已签收" }] },
      { id: "A2-1-5", title: "签收解析模块", kind: "验收记录", edges: { with: ["A2-1-1"] }, ...inA21, body: "读入与校验写完，单测 38 passed；变异 22/22 被杀，对过 A2 的边界三条。" },
      { id: "A2-2", title: "导入验收测试", kind: "测试", edges: { with: ["A2"], after: ["A2-1-1"] }, coveredBy: ["A8"], files: [{ path: "tests/test_accept_import.py", check: "empty", note: "0 字" }] },
      { id: "A3", title: "导出 CSV", kind: "能力", state: signed, children: ["A3-1", "A3-2", "A3-3"], body: "把对过账的行写回 CSV，列序照 A2。" },
      { id: "A3-1", title: "导出实现", kind: "模块", state: signed, edges: { with: ["A3"] }, childrenIn: "A3-1", assignee: P.export, files: [{ path: "ledger/export_csv.py", check: "invalid", note: "是个目录" }] },
      { id: "A3-2", title: "导出验收测试", kind: "测试", edges: { with: ["A3"] }, view: false },
      { id: "A3-3", title: "导出格式选项", kind: "模块", state: signed, edges: { with: ["A3"] }, childrenIn: "A3-3", assignee: null, body: "分隔符与编码两个选项，默认逗号与 UTF-8。" },
      { id: "A4", title: "工程底座", kind: "能力", state: signed, children: ["A4-1", "A4-2", "A4-3"], ...inA4, signer: P.lead, body: "检查脚本与打包配置，一条命令跑全部检查。" },
      landed === "cell"
        ? { id: "A4-1", title: "检查脚本", kind: "模块", children: ["A4-1-1"], edges: { with: ["A4"] }, ...inA4, files: [{ path: "scripts/check.sh", check: "present", note: "912 字" }] }
        : { id: "A4-1", title: "检查脚本", kind: "模块", state: ["已签收"], edges: { with: ["A4"] }, ...inA4, files: [{ path: "scripts/check.sh", check: "present", note: "912 字" }] },
      ...(landed === "cell" ? [{ id: "A4-1-1", title: "收集 0 条时退 5", kind: "模块", edges: { with: ["A4-1"] }, ...inA4 }] : []),
      { id: "A4-2", title: "打包配置", kind: "模块", state: ["已签收"], edges: { with: ["A4"] }, ...inA4, signer: STRANGER },
      { id: "A4-3", title: "交回工程底座", kind: "验收记录", ...inA4, body: "A4-1 · A4-2 已签收；一条命令跑全部检查。", sets: [{ target: "A4", value: "已签收" }] },
      { id: "A5", title: "要不要支持网页导入", kind: "待定", body: "问题：网页表单导入值不值得做。\n推荐：先不做，命令行够用。\n撤不撤得回：撤得回。" },
      { id: "A6", title: "列定义只写一处", kind: "待定", state: ["decided"], children: ["A6-1"], evidencedBy: ["A6-1"], body: "问题：导入与导出的列定义放哪。" },
      { id: "A6-1", title: "列定义只写一处", kind: "原话", edges: { with: ["A6"] }, body: "「列定义只写一处」「导出照导入的列序」" },
      { id: "A8", title: "列定义一处", kind: "规矩", covers: ["A2"], body: "守什么：导入与导出照同一份列定义（A2）。" },
      ...(landed === "top" ? [{ id: "A7", title: "导出与导入对拍", kind: "能力" }] : []),
      { id: "P1", title: "先后", kind: "排期", graph: "排期", edges: { to: ["A1"] } },
      { id: "P2", title: "划出", kind: "排期", graph: "排期", covers: ["A2-1"] },
    ],
    "架构",
    P.lead,
  );
  return {
    name: "ledger",
    domain: "软件",
    path: "repos/ledger",
    plan_only: o.docsOnly === true,
    ...SOFT,
    top_level: ["A1", "A2", "A3", "A4", "A5", "A6", "A8", ...(landed === "top" ? ["A7"] : [])],
    scopes: [
      scope("ledger", "project", "", P.lead, o.topDone ? "回看" : "开发", "A3"),
      scope("ledger", "A2-1", "导入解析", SUB_ID, "审计", "A2-1-3"),
      scope("ledger", "A3-1", "导出实现", P.export, "开发", null),
      scope("ledger", "A3-3", "导出格式选项", null, "定架构", null),
      scope("ledger", "A4", "工程底座", P.base, "回看", null),
    ],
    pending_scopes: [],
    unowned: { paths: ["README.md"], error: null },
    check: {
      read_errors: [{ file: ".planned-build/ledger/A4/图.md", line: 31, message: "元数据那一行写不对：`with A4` 少了冒号" }],
      unparsed_nodes: ["A4-9"],
      failures: [{ rule: "边指着图上的节点", message: "A4-1 --to--> A9：A9 不在图上", scope: "A4", fix: "改掉那条边，或者补上 A9", refs: [{ start: 0, end: 4, ids: ["A4-1"] }] }],
      unknown: [],
    },
    nodes,
    pending_sets: [
      { source: "A2-1-4", target: "A2-1-3", value: "已签收", op: "add", blocked_reason: null, blocked_code: null, rule: null },
      { source: "A4-3", target: "A4", value: "已签收", op: "add", blocked_reason: "源节点在 A4 那个权力范围（还划出着）：等上一级删掉划出那一项", blocked_code: "source_delegated", rule: null },
    ],
    pending_replaces: [],
    archived: [{ id: "A9", title: "网页导入", kind: "能力", replaced_by: null }],
  };
}

function surveyRepo(): Record<string, unknown> {
  const lead = PLAN_SIDS.lead;
  return {
    name: "export-survey",
    domain: "研究",
    path: "export-survey",
    plan_only: false,
    ...RESEARCH,
    top_level: ["Q1", "Q2", "Q3"],
    scopes: [scope("export-survey", "project", "", lead, "搜集", "Q3")],
    pending_scopes: [],
    unowned: { paths: [], error: null },
    check: NO_CHECK,
    nodes: dumpNodes(
      [
        { id: "Q1", title: "现有导出格式都有哪些", kind: "问题", body: "列了 5 种。" },
        { id: "Q2", title: "各家银行接受哪种", kind: "来源", state: ["支持"], children: ["Q2-1"], evidencedBy: ["Q2-1"] },
        { id: "Q2-1", title: "三家银行的文档", kind: "佐证", edges: { with: ["Q2"] } },
        { id: "Q3", title: "推荐哪一种", kind: "推断" },
      ],
      "知识",
      lead,
    ),
    pending_sets: [],
    pending_replaces: [],
    archived: [],
  };
}

/** 新登记的仓库：pb repo 建了 project，还没有人认领、一个节点都没有。 */
function freshRepo(): Record<string, unknown> {
  return {
    name: "invoice",
    domain: "软件",
    path: "invoice",
    plan_only: true,
    ...SOFT,
    top_level: [],
    scopes: [scope("invoice", "project", "", null, "定架构", null)],
    pending_scopes: [],
    unowned: { paths: null, error: "仓库 invoice 不在盘上" },
    check: NO_CHECK,
    nodes: [],
    pending_sets: [],
    pending_replaces: [],
    archived: [],
  };
}

function siteRepo(): Record<string, unknown> {
  const s = PLAN_SIDS.site;
  return {
    name: "site",
    domain: "软件",
    path: "site",
    plan_only: false,
    ...SOFT,
    top_level: ["B1", "B2", "B3", "B4", "B5"],
    scopes: [scope("site", "project", "", s, "开发", null)],
    pending_scopes: [],
    unowned: { paths: [], error: null },
    check: NO_CHECK,
    nodes: dumpNodes(
      [
        { id: "B1", title: "首页", kind: "能力", state: ["已签收"] },
        { id: "B2", title: "文章列表", kind: "能力", state: ["已签收"] },
        { id: "B3", title: "评论", kind: "能力", children: ["B3-1", "B3-2"] },
        { id: "B3-1", title: "评论接口", kind: "模块", state: ["已签收"], edges: { with: ["B3"] } },
        { id: "B3-2", title: "评论审核", kind: "模块", edges: { with: ["B3"] } },
        { id: "B4", title: "部署", kind: "能力", state: ["已签收"] },
        { id: "B5", title: "订阅", kind: "能力" },
      ],
      "架构",
      s,
    ),
    pending_sets: [],
    pending_replaces: [],
    archived: [],
  };
}

const DOMAINS = [
  { name: "研究", source: "plugin", ...RESEARCH },
  { name: "软件", source: "plugin", ...SOFT },
];

const dumpOf = (workspace: string, repo: string, repos: Record<string, unknown>[]): Record<string, unknown> => ({ pb: "0.5.2", shape: 6, workspace, current_repo: repo, auto_continue: false, domains: DOMAINS, repos });

/** 本机那个工作区此刻的 dump（`landed` ⇒ 退回之后 pb 那一侧加了一个节点：场景开页之后改写那一份）。 */
export function ledgerDump(o: PlanWorldOpts, landed?: Landed): Record<string, unknown> {
  return dumpOf(LEDGER_WS, "ledger", [ledgerRepo(o, landed), surveyRepo(), ...(o.fresh ? [freshRepo()] : [])]);
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
        ...(o.autoRefuse && m === LOCAL ? { refuse: { continue: "这个工作区还没有人认领 project" } } : {}),
        ...(o.sliceStale && m === LOCAL ? { brokenAfterFirst: { repo: "ledger", said: "图.md 第 142 行：元数据缺 kind" } } : {}),
        domains: DOMAINS,
      };
      edit(m, (d) => putPb(d, cfg));
    }
  }
  if (!o.empty) {
    edit(LOCAL, (d) => putWorkspace(d, LEDGER_REL, ledgerDump(o)));
    edit("devbox", (d) => putWorkspace(d, SITE_REL, dumpOf(SITE_WS, "site", [siteRepo()])));
  }
  if (o.groups) edit(LOCAL, (d) => (d.files[".cc-monitor/history-metadata.json"] = groupsNotes()));
  return {
    ...base,
    disk,
    sessions: [...base.sessions, ...planSessions()],
    opDelayMs: o.slowMs ? { ...(base.opDelayMs ?? {}), "plan-list": o.slowMs } : base.opDelayMs,
  };
}
