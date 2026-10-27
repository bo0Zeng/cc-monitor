/**
 * 计划那一族的盘上输入：一个装在插件缓存里的 pb 替身（真后端照常找名字是 planned-build 的插件、起 `python3 <入口> <动词>`）＋
 * 工作区里 pb 原样那一份 dump（pb 0.5 · 形状 6，键名照 pb：`current_repo` · `repos[]` · `nodes[]` · `state` · `file_check` ·
 * `scopes[].handoff` · `pending_sets` …；每个节点另存一段 `agent_view`，替身的 `dump` 摘掉它、`dump --node` 才给，同 0.5）。
 * 替身只吐那一份、照几条命令改那一份、演 pb 的退出码（判据 `tests/shots/pb-stand-in.vitest.ts`）；数 · 排 · 短词一律是真后端算的。
 */
import type { MachineDisk } from "./index";

/** 插件根（相对家目录）：配置根 `.claude` 底下插件缓存里的一个版本。 */
export const PB_ROOT = ".claude/plugins/cache/shots/planned-build/0.5.2";
/** 入口脚本（相对家目录）。 */
export const PB_ENTRY = `${PB_ROOT}/skills/planned-build/tools/entry.py`;

/**
 * 替身怎么演：`dumpRc` ⇒ `dump` 直接以它退（4 ＝ 没有 dump 的旧 pb）· `refuse` ⇒ 那几条命令照 pb 拒（退 2、stderr 那一句）· `brokenAfterFirst` ⇒ 头一次之后那个仓库读不成 ·
 * `domains` ⇒ `dump --domains` 往上找不到工作区时给的插件那几档（缺 ⇒ 空）。
 */
export interface PbStandIn {
  dumpRc?: number;
  refuse?: Record<string, string>;
  brokenAfterFirst?: { repo: string; said: string };
  domains?: unknown[];
}

const ENTRY_PY = String.raw`import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(os.path.dirname(os.path.dirname(here)))
cfg = json.load(open(os.path.join(root, "stand-in.json")))
verb = sys.argv[1] if len(sys.argv) > 1 else ""
if verb == "help":
    print("planned-build"); sys.exit(0)
if verb == "dump" and cfg.get("dumpRc") is not None:
    sys.stderr.write("用法：没有 dump 这个动词\n"); sys.exit(cfg["dumpRc"])
domains = verb == "dump" and sys.argv[2:3] == ["--domains"]
d = sys.argv[3] if domains and len(sys.argv) > 3 else os.getcwd()
while not os.path.isdir(os.path.join(d, ".planned-build")):
    up = os.path.dirname(d)
    if up == d:
        if domains:
            sys.stdout.write(json.dumps({"pb": "0.6.0", "shape": 6, "workspace": None, "domains": cfg.get("domains", [])}, ensure_ascii=False)); sys.exit(0)
        sys.stderr.write("读不成：这里不是一个 pb 工作区\n"); sys.exit(3)
    d = up
f = os.path.join(d, ".planned-build", "dump.json")
doc = json.load(open(f))
if domains:
    sys.stdout.write(json.dumps({"pb": doc.get("pb"), "shape": doc.get("shape"), "workspace": doc.get("workspace"), "domains": doc.get("domains", [])}, ensure_ascii=False)); sys.exit(0)
def save():
    tmp = f + ".tmp"
    open(tmp, "w").write(json.dumps(doc, ensure_ascii=False))
    os.replace(tmp, f)
if verb in cfg.get("refuse", {}):
    sys.stderr.write(cfg["refuse"][verb] + "\n"); sys.exit(2)
if verb == "dump" and sys.argv[2:3] == ["--node"] and len(sys.argv) >= 5:
    rn, nid = sys.argv[3], sys.argv[4]
    out = {"pb": doc.get("pb"), "shape": doc.get("shape"), "workspace": doc.get("workspace"), "repo": rn, "id": nid}
    r = next((x for x in doc["repos"] if x["name"] == rn), None)
    n = next((x for x in (r or {}).get("nodes", []) if x["id"] == nid), None)
    if n is not None and n.get("agent_view") is not None:
        out["agent_view"] = n["agent_view"]
    else:
        out["error"] = "%s 不在 %s 的图上" % (nid, rn)
    sys.stdout.write(json.dumps(out, ensure_ascii=False)); sys.exit(0)
if verb == "dump":
    for r in doc["repos"]:
        for n in r.get("nodes", []):
            n.pop("agent_view", None)
    k_file = os.path.join(root, "dumps.n")
    k = int(open(k_file).read()) + 1 if os.path.exists(k_file) else 1
    open(k_file, "w").write(str(k))
    b = cfg.get("brokenAfterFirst")
    if b and k > 1:
        doc["repos"] = [{"name": r["name"], "domain": r.get("domain"), "path": r.get("path"), "plan_only": r.get("plan_only", False), "error": b["said"]} if r["name"] == b["repo"] else r for r in doc["repos"]]
    sys.stdout.write(json.dumps(doc, ensure_ascii=False)); sys.exit(0)
if verb in ("continue", "pause"):
    doc["auto_continue"] = verb == "continue"; save(); sys.exit(0)
if verb == "repo" and len(sys.argv) > 2:
    doc["current_repo"] = sys.argv[2]; save(); sys.exit(0)
if verb == "adopt":
    r = next(x for x in doc["repos"] if x["name"] == doc["current_repo"])
    r["plan_only"] = False; save()
    print("「%s」的仓库接过来了（%s）：从现在起照规则守" % (r["name"], r.get("path", r["name"]))); sys.exit(0)
if verb == "render":
    print("写了 /tmp/pb-读图-0000.html"); print("用浏览器打开它（再敲一次 render 就换成此刻的图）"); sys.exit(0)
sys.stderr.write("用法：认不得 %s\n" % verb); sys.exit(4)
`;

/** 装上 pb 替身。 */
export function putPb(d: MachineDisk, cfg: PbStandIn = {}): void {
  d.files[`${PB_ROOT}/.claude-plugin/plugin.json`] = JSON.stringify({ name: "planned-build", version: "0.5.2" }) + "\n";
  d.files[PB_ENTRY] = ENTRY_PY;
  d.files[`${PB_ROOT}/stand-in.json`] = JSON.stringify(cfg) + "\n";
}

/** dump 在工作区里的那一份（相对家目录）：场景开页之后改它（`writeFile`）就是 pb 那一侧变了。 */
export const dumpFile = (ws: string): string => `${ws}/.planned-build/dump.json`;

/** 一个工作区（`ws` 相对家目录）：计划仓 `.planned-build/` ＋ 工作区配置 ＋ 替身吐的那一份 dump。 */
export function putWorkspace(d: MachineDisk, ws: string, doc: unknown): void {
  d.files[dumpFile(ws)] = JSON.stringify(doc);
  d.files[`${ws}/.planned-build/.env`] = JSON.stringify({ current_repo: (doc as { current_repo?: string }).current_repo ?? null }) + "\n";
}

// ── dump 里的一个节点（pb 原样那一形）──

export interface NodeIn {
  id: string;
  title: string;
  kind: string;
  /** 状态里记着的值（`已签收` · `decided` …），一个值一笔；`[值, 笔数]` ⇒ 那几笔。 */
  state?: (string | [string, number])[];
  body?: string;
  children?: string[];
  edges?: Partial<Record<"to" | "with" | "after" | "replaces", string[]>>;
  files?: { path: string; check: "present" | "missing" | "empty" | "invalid"; note: string }[];
  /** 依据它的节点（凭据类写了边指着它的）。 */
  evidencedBy?: string[];
  /** 它的子节点在哪个权力范围里加（默认 `project`）· 那个权力范围的负责人 · 写得动它元数据的那一位。 */
  childrenIn?: string;
  assignee?: string | null;
  signer?: string | null;
  covers?: string[];
  coveredBy?: string[];
  /** 自己写的、还没执行的 sets。 */
  sets?: { target: string; value: string; op?: "add" | "remove" }[];
  /** 住哪种图（默认 permanent 那一种；排期的节点写 `排期`）。 */
  graph?: string;
  /** 站在这个节点时 pb 印给 agent 的那一段；`false` ⇒ 没有。缺 ⇒ 照编号拼一段。 */
  view?: string | false;
}

/** 话里提到的编号的位置（pb 给的 `refs`：按码点算的起止）。 */
function refsIn(text: string, ids: ReadonlySet<string>): { start: number; end: number; ids: string[] }[] {
  const out = [];
  for (const m of text.matchAll(/[A-Z]\d+(?:-\d+)*/g)) {
    if (!ids.has(m[0])) continue;
    const start = Array.from(text.slice(0, m.index)).length;
    out.push({ start, end: start + Array.from(m[0]).length, ids: [m[0]] });
  }
  return out;
}

const agentView = (n: NodeIn): string => `── ${n.id} ${n.title}（${n.kind}）\n> id: ${n.id} | kind: ${n.kind}\n\n要做成什么样：见上。\n${n.id} 底下下一个可用编号：${n.id}-1`;

/** 一个仓库的节点（pb 原样）：父节点照 `children` 倒出来，标题与正文里的编号标出位置。`graph` ⇒ permanent 那一种图的名字。 */
export function dumpNodes(nodes: NodeIn[], graph: string, lead: string | null): Record<string, unknown>[] {
  const ids = new Set(nodes.map((n) => n.id));
  const parentOf = new Map<string, string>();
  for (const n of nodes) for (const k of n.children ?? []) parentOf.set(k, n.id);
  return nodes.map((n) => ({
    id: n.id,
    title: n.title,
    kind: n.kind,
    graph: n.graph ?? graph,
    body: n.body ?? "",
    parent: parentOf.get(n.id) ?? null,
    children: n.children ?? [],
    edges: { to: [], with: [], after: [], replaces: [], ...n.edges, covers: n.covers ?? [], file: (n.files ?? []).map((f) => f.path), sets: (n.sets ?? []).map((s) => ({ op: "add", ...s })) },
    files: (n.files ?? []).map((f) => ({ path: f.path, file_check: f.check, note: f.note, chars: null, bytes: null })),
    state: (n.state ?? []).map((v) => (Array.isArray(v) ? { value: v[0], count: v[1] } : { value: v, count: 1 })),
    evidenced_by: n.evidencedBy ?? [],
    children_in: n.childrenIn ?? "project",
    assignee: n.assignee === undefined ? lead : n.assignee,
    signer: n.signer === undefined ? lead : n.signer,
    covered_by: n.coveredBy ?? [],
    refs: { title: [], body: refsIn(n.body ?? "", ids) },
    ...(n.view === false ? {} : { agent_view: n.view ?? agentView(n) }),
  }));
}
