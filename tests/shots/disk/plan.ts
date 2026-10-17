/**
 * 计划那一族的盘上输入：一个装在插件缓存里的 pb 替身（真后端照常找名字是 planned-build 的插件、起 `python3 <入口> <动词>`）＋
 * 工作区里 pb 原样那一份 dump（形状 1，键名照 pb：`edge_words` · `agent_view` · `replaced_by` · `docs_only` …）。
 * 替身只吐那一份、照几条命令改那一份、演 pb 的退出码（判据 `tests/shots/pb-stand-in.vitest.ts`）；数 · 排 · 短词 · 时刻字一律是真后端算的。
 */
import type { MachineDisk } from "./index";

/** 插件根（相对家目录）：配置根 `.claude` 底下插件缓存里的一个版本。 */
export const PB_ROOT = ".claude/plugins/cache/shots/planned-build/0.2.0";
/** 入口脚本（相对家目录）。 */
export const PB_ENTRY = `${PB_ROOT}/skills/planned-build/tools/entry.py`;

/** 替身怎么演：`dumpRc` ⇒ `dump` 直接以它退（4 ＝ 没有 dump 的旧 pb）· `refuse` ⇒ 那几条命令照 pb 拒（退 2、stderr 那一句）· `brokenAfterFirst` ⇒ 头一次之后那一片读不成。 */
export interface PbStandIn {
  dumpRc?: number;
  refuse?: Record<string, string>;
  brokenAfterFirst?: { slice: string; said: string };
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
d = os.getcwd()
while not os.path.isdir(os.path.join(d, ".planned-build")):
    up = os.path.dirname(d)
    if up == d:
        sys.stderr.write("读不成：这里不是一个 pb 工作区\n"); sys.exit(3)
    d = up
f = os.path.join(d, ".planned-build", "dump.json")
doc = json.load(open(f))
def save():
    tmp = f + ".tmp"
    open(tmp, "w").write(json.dumps(doc, ensure_ascii=False))
    os.replace(tmp, f)
if verb in cfg.get("refuse", {}):
    sys.stderr.write(cfg["refuse"][verb] + "\n"); sys.exit(2)
if verb == "dump":
    n = os.path.join(root, "dumps.n")
    k = int(open(n).read()) + 1 if os.path.exists(n) else 1
    open(n, "w").write(str(k))
    b = cfg.get("brokenAfterFirst")
    if b and k > 1:
        doc["slices"] = [{"name": s["name"], "domain": s.get("domain"), "error": b["said"]} if s["name"] == b["slice"] else s for s in doc["slices"]]
    sys.stdout.write(json.dumps(doc, ensure_ascii=False)); sys.exit(0)
if verb in ("continue", "pause"):
    doc["auto"] = verb == "continue"; save(); sys.exit(0)
if verb == "repo" and len(sys.argv) > 2:
    doc["repo"] = sys.argv[2]; save(); sys.exit(0)
if verb == "take":
    sl = next(s for s in doc["slices"] if s["name"] == doc["repo"])
    sl["docs_only"] = False; save()
    print("「%s」的仓库接过来了（%s）：从现在起照规矩守" % (sl["name"], sl.get("path", sl["name"]))); sys.exit(0)
if verb == "view":
    print("写了 /tmp/pb-读图-0000.html"); sys.exit(0)
sys.stderr.write("用法：认不得 %s\n" % verb); sys.exit(4)
`;

/** 装上 pb 替身。 */
export function putPb(d: MachineDisk, cfg: PbStandIn = {}): void {
  d.files[`${PB_ROOT}/.claude-plugin/plugin.json`] = JSON.stringify({ name: "planned-build", version: "0.2.0" }) + "\n";
  d.files[PB_ENTRY] = ENTRY_PY;
  d.files[`${PB_ROOT}/stand-in.json`] = JSON.stringify(cfg) + "\n";
}

/** dump 在工作区里的那一份（相对家目录）：场景开页之后改它（`writeFile`）就是 pb 那一侧变了。 */
export const dumpFile = (ws: string): string => `${ws}/.planned-build/dump.json`;

/** 一个工作区（`ws` 相对家目录）：计划仓 `.planned-build/` ＋ 工作区根的 `.env` ＋ 替身吐的那一份 dump。 */
export function putWorkspace(d: MachineDisk, ws: string, doc: unknown): void {
  d.files[dumpFile(ws)] = JSON.stringify(doc);
  d.files[`${ws}/.env`] = JSON.stringify({ 仓库: (doc as { repo?: string }).repo ?? null }) + "\n";
}

// ── dump 里的一格（pb 原样那一形）──

export interface CellIn {
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
  signer?: string;
  owner?: string;
  /** 站在这一格时 pb 印给 agent 的那一段；`false` ⇒ 这一格没有。缺 ⇒ 照编号拼一段。 */
  view?: string | false;
}

/** 话里提到的编号的位置（pb 给的 `refs`：按字算的起止）。 */
function refsIn(text: string, ids: ReadonlySet<string>): { start: number; end: number; ids: string[] }[] {
  const out = [];
  for (const m of text.matchAll(/[A-Z]\d+(?:-\d+)*/g)) {
    if (!ids.has(m[0])) continue;
    const start = Array.from(text.slice(0, m.index)).length;
    out.push({ start, end: start + Array.from(m[0]).length, ids: [m[0]] });
  }
  return out;
}

const agentView = (c: CellIn): string => `── ${c.id} ${c.title}（${c.kind} · ${c.status}）\n> id: ${c.id} | kind: ${c.kind}\n\n要做成什么样：见上。\n下一步：pb sign ${c.id} <理由>`;

/** 一片的格（pb 原样）：父格照 `children` 倒出来，正文与签收里的编号标出位置。 */
export function dumpCells(cells: CellIn[]): Record<string, unknown>[] {
  const ids = new Set(cells.map((c) => c.id));
  const parentOf = new Map<string, string>();
  for (const c of cells) for (const k of c.children ?? []) parentOf.set(k, c.id);
  return cells.map((c) => ({
    id: c.id,
    title: c.title,
    kind: c.kind,
    body: c.body ?? null,
    parent: parentOf.get(c.id) ?? null,
    children: c.children ?? [],
    edges: { to: [], with: [], after: [], replaces: [], ...c.edges },
    files: c.files ?? [],
    status: c.status,
    why: c.why ?? null,
    signs: (c.signs ?? []).map((g) => ({ ...g, refs: refsIn(g.reason, ids) })),
    owner: c.owner ?? null,
    ...(c.signer ? { signer: c.signer } : {}),
    refs: { title: [], body: refsIn(c.body ?? "", ids) },
    ...(c.view === false ? {} : { agent_view: c.view ?? agentView(c) }),
  }));
}
