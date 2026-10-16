#!/usr/bin/env python3
"""gate-affected 的挑格那一半：读改动路径（stdin 一行一条）＋ 映射表 tests/scripts/gate-affected/map ＋ 仓里现状，印出计划。

用法：git diff --name-status -M <基>...HEAD | python3 tests/scripts/gate-affected/plan.py <仓根> [--quiet] [--trust-ci]
  --trust-ci：CI（推候选分支 next 后）会跑的格这一趟不跑，只跑恒跑的便宜格 ＋ 被改到的、CI 上没有的格
              ＋ @keep-local 那几格 ＋ 被改到被测对象的 e2e 套件（逐套算，见现算规则 ②）。
输出：逐条路径 → 组 · 理由；末尾两行机读：
  PLAN-MODE: affected | full | none
  PLAN-CELLS: <空格分隔的格名>      （mode=full 时为空：调用方不带 GATE_ONLY）
退出码：0 出了计划；2 映射表与 gate.sh 对不上（表过期，停下来修表，不猜）。
"""
import os, re, sys, fnmatch, subprocess
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
MAP = os.path.join(HERE, "map")
root = sys.argv[1]
quiet = "--quiet" in sys.argv
trust_ci = "--trust-ci" in sys.argv
os.chdir(root)

def out(s=""):
    if not quiet:
        print(s)

# ── 读表 ──
groups, rules = {}, []
for ln, line in enumerate(open(MAP, encoding="utf-8"), 1):
    line = line.rstrip("\n")
    if not line.strip() or line.lstrip().startswith("#"):
        continue
    if line.startswith("@"):
        name, *cells = line[1:].split()
        groups[name] = cells
        continue
    parts = line.split(None, 2)
    if len(parts) < 2:
        print(f"gate-affected/map 第 {ln} 行认不出来：{line!r}", file=sys.stderr); sys.exit(2)
    pat, gs = parts[0], parts[1]
    why = parts[2].strip() if len(parts) > 2 else ""
    rules.append((pat, [] if gs == "-" else gs.split(","), why, ln))

def glob_re(p):
    i, r = 0, ""
    while i < len(p):
        if p.startswith("**/", i): r += "(?:.*/)?"; i += 3
        elif p.startswith("**", i): r += ".*"; i += 2
        elif p[i] == "*": r += "[^/]*"; i += 1
        elif p[i] == "?": r += "[^/]"; i += 1
        else: r += re.escape(p[i]); i += 1
    return re.compile(r + r"\Z")
rules = [(glob_re(p), p, gs, why, ln) for p, gs, why, ln in rules]

# ── 与 gate.sh 对账：表里每个格名都得是盘上真有的格；盘上有、表里没提的格按恒跑 ──
gate = open("tests/scripts/gate.sh", encoding="utf-8").read()
cells_on_disk = []
for m in re.finditer(r"^(?:run_gate|run_gate_sum|run_e2e)\s+([A-Za-z0-9_-]+)|^if gate_wants ([A-Za-z0-9_-]+)", gate, re.M):
    n = m.group(1) or m.group(2)
    if n not in cells_on_disk:
        cells_on_disk.append(n)
known = set(cells_on_disk)
for g, cs in groups.items():
    for c in cs:
        if c not in known:
            print(f"gate-affected/map：组 @{g} 里的 `{c}` 不是 gate.sh 盘上的格 —— 表过期了，先改表", file=sys.stderr); sys.exit(2)
for _, p, gs, _, ln in rules:
    for g in gs:
        if g != "FULL" and g not in groups:
            print(f"gate-affected/map 第 {ln} 行：组 `{g}` 没有 @ 定义", file=sys.stderr); sys.exit(2)
META = {"ci-windows", "keep-local"}   # 不是「改了跑哪几格」的组，是标注
for need in ("cheap", "suites", "ci-windows"):
    if need not in groups:
        print(f"gate-affected/map 缺 @{need}", file=sys.stderr); sys.exit(2)
mentioned = {c for g, cs in groups.items() if g not in META for c in cs}
unmapped = [c for c in cells_on_disk if c not in mentioned]
# CI 跑哪几格：Linux job 的 GATE_ONLY 从 ci.yml 现读（`e2e` 展开成全部套件）＋ 表里登记的 Windows 等价物
ci_yml = open(".github/workflows/ci.yml", encoding="utf-8").read()
ci_cells = set(groups["ci-windows"])
all_suites = re.findall(r"^run_e2e ([A-Za-z0-9_-]+)$", gate, re.M)
for m in re.finditer(r"^\s*GATE_ONLY:\s*(.+)$", ci_yml, re.M):
    for tok in m.group(1).split():
        if tok == "e2e":
            ci_cells |= set(all_suites)
        elif tok in known:
            ci_cells.add(tok)
local_only = [c for c in cells_on_disk if c not in ci_cells]
always = list(groups["cheap"]) + ([] if trust_ci else list(groups["suites"])) + unmapped

# ── 现算规则 ① ④：Rust 的 include / #[path] 归属 ──
ls = subprocess.run(["git", "-c", "core.quotepath=false", "ls-files"], capture_output=True, text=True).stdout.split("\n")
tracked = set(f for f in ls if f)
WS = {
    "shellrs": ("src/frontend/shell/", "src/frontend/filewin/", "src/common/", "src/comms/",
                "tests/frontend/shell/", "tests/frontend/filewin/", "tests/common/", "tests/comms/"),
    "backendrs": ("src/backend/", "src/vendor/", "src/comms/outward/", "src/common/", "tests/backend/", "tests/benches/"),
}
owners = defaultdict(set)
for f in tracked:
    if f.endswith(".rs") or f.endswith("Cargo.toml") or f.endswith("Cargo.lock"):
        for g, pre in WS.items():
            if f.startswith(pre):
                owners[f].add(g)
inc_re = re.compile(r'include_(?:str|bytes)!\s*\(\s*"([^"]+)"')
man_re = re.compile(r'include_(?:str|bytes)!\s*\(\s*concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*"([^"]+)"')
path_re = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
def norm(p):
    return os.path.normpath(p).replace(os.sep, "/")
def manifest_dir(f):
    d = os.path.dirname(f)
    while d and not os.path.exists(os.path.join(d, "Cargo.toml")):
        d = os.path.dirname(d)
    return d
edges_path, edges_inc = [], []   # (includer, target)
for f in tracked:
    if not f.endswith(".rs"):
        continue
    try:
        src = open(f, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    d = os.path.dirname(f)
    for m in path_re.finditer(src):
        stem = os.path.splitext(os.path.basename(f))[0]
        for cand in (norm(os.path.join(d, m.group(1))), norm(os.path.join(d, stem, m.group(1)))):
            if cand in tracked:
                edges_path.append((f, cand))
    for m in inc_re.finditer(src):
        edges_inc.append((f, norm(os.path.join(d, m.group(1)))))
    for m in man_re.finditer(src):
        edges_inc.append((f, norm(os.path.join(manifest_dir(f), m.group(1).lstrip("/")))))
changed_any = True
while changed_any:   # #[path] 引进来的文件归引它的那棵（传递）
    changed_any = False
    for a, b in edges_path:
        if not owners[a] <= owners[b]:
            owners[b] |= owners[a]; changed_any = True
inc_owner = defaultdict(set)
for a, b in edges_inc:
    inc_owner[b] |= owners[a]

# ── 现算规则 ②：e2e 套件从入口顺着摸得到的文件（逐套算：--trust-ci 下被改到被测对象的那几套本机照跑）──
pkg = open("package.json", encoding="utf-8").read()
suites = [c for c in groups.get("e2e", []) if re.search(r"^run_e2e " + re.escape(c) + r"$", gate, re.M)]
HARNESS = {"tests/e2e/assert-pass-floor.sh", "tests/e2e/sandbox-env.sh"}   # 每一套都经它们起、判
# 说明文档（README.md 等）不执行、却点名每一套脚本 ⇒ 不进闭包（进了，一份 README 就把每套连成全部套件）。
e2e_tree = sorted(f for f in tracked if f.startswith("tests/e2e/") and not f.startswith("tests/e2e/weak-net/") and not f.endswith(".md"))
# 前面紧挨着 `/` 也认（`"$REPO/src/shared/cc-bus/scripts"` 那一形）：多认只会多跑一套，少认会漏跑。
tok_re = re.compile(r"(?<![A-Za-z0-9_.-])((?:src|tests)/[A-Za-z0-9_.\-/]+)")
imp_re = re.compile(r"""(?:from|import)\s*\(?\s*["'](\.{1,2}/[^"']+)["']""")
BACKEND_BIN = re.compile(r"cc-monitor-backend")
BACKEND_TEST = re.compile(r"src/backend\b[^\n]*(?:\n[^\n]*){0,6}?\bcargo test\b")
SHELL_TEST = re.compile(r"src/frontend/shell\b[^\n]*(?:\n[^\n]*){0,6}?\bcargo test\b")
def suite_closure(entry):
    """一套 e2e 的入口文件 → (它摸得到的文件与字面引到的路径, 它跑的是哪几样编出来的东西)。"""
    refs, seen, todo = set(), set(), list(entry)
    ws = set()
    while todo:
        f = todo.pop()
        if f in seen or not os.path.isfile(f):
            continue
        seen.add(f); refs.add(f)
        try:
            txt = open(f, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if f.startswith("tests/e2e/"):
            for t in e2e_tree:   # 套件脚本按名字调别的 e2e 文件（source / bash / node）
                if t not in seen and os.path.basename(t) in txt:
                    todo.append(t)
            for m in tok_re.finditer(txt):
                t = m.group(1).rstrip("./")
                # 只点到 e2e 目录本身（散文里「见 tests/e2e」）不算引用它下面的每一份；说明文档也不算。
                if t and not t.endswith(".md") and t not in ("tests/e2e", "tests", "src"):
                    refs.add(t)
            # 被测对象里编出来的那几样：起后端二进制 ⇒ 后端那棵的生产源（tests/ 下的不进二进制）；
            #   在后端 / 壳那棵里 cargo test（取渲染器输出、local-backend 的 --ignored 测试）⇒ 那棵的全部源（含 tests/ 下 #[path] 引进 lib 的）。
            if BACKEND_BIN.search(txt): ws.add("backendrs-bin")
            if BACKEND_TEST.search(txt): ws.add("backendrs")
            if SHELL_TEST.search(txt): ws.add("shellrs")
        if f.endswith((".ts", ".mts", ".tsx", ".js", ".mjs")):
            for m in imp_re.finditer(txt):
                t = norm(os.path.join(os.path.dirname(f), m.group(1)))
                for cand in (t, t + ".ts", t + ".mts", t + "/index.ts"):
                    if os.path.isfile(cand):
                        todo.append(cand); refs.add(cand); break
    return refs, ws
suite_refs, suite_ws = {}, {}
for s in suites:
    entry = set(HARNESS)
    m = re.search(r'"test:' + re.escape(s) + r'"\s*:\s*"([^"]+)"', pkg)
    if m:
        for tok in re.findall(r"tests/e2e/[A-Za-z0-9_./-]+", m.group(1)):
            entry.add(norm(tok))
    suite_refs[s], suite_ws[s] = suite_closure(entry)
e2e_refs = set().union(*suite_refs.values()) if suite_refs else set()
def suites_touched_by(p):
    """改了 p 会动到哪几套 e2e 的被测对象：字面引到 / 摸得到它，或它进了那套起的二进制（Rust 源与 include_str! 引进去的文本）。"""
    ws = set(owners.get(p, ())) | set(inc_owner.get(p, ()))
    if "backendrs" in ws and not p.startswith("tests/"):
        ws.add("backendrs-bin")
    hit = []
    for s in suites:
        if any(p == t or p.startswith(t + "/") for t in suite_refs[s]) or (ws & suite_ws[s]):
            hit.append(s)
    return hit

# ── 现算规则 ③：shellcheck 的人群 ──
m = re.search(r"^GATE_SHELLCHECK_GLOBS='([^']*)'", gate, re.M)
sc_globs = m.group(1).split() if m else []

# ── 逐条判 ──
# 输入：`git diff --name-status -M` 的行（状态\t路径[\t新路径]），也收裸路径（按「改了内容」算）。
paths, gone = [], set()   # gone：删掉的、或改名改走的旧路径
for line in sys.stdin:
    f = line.rstrip("\n").split("\t")
    if not f[0].strip():
        continue
    if len(f) == 1:
        paths.append(f[0].strip()); continue
    st = f[0][:1]
    if st == "R" and len(f) >= 3:
        paths += [f[1], f[2]]; gone.add(f[1])
    else:
        paths.append(f[1])
        if st == "D":
            gone.add(f[1])
paths = list(dict.fromkeys(paths))
want = set()
full_why = []
e2e_hit = set()   # 被改到被测对象的 e2e 套件（--trust-ci 下本机照跑这几套，不只是 @keep-local 写死的那几套）
for p in paths:
    gs, whys = set(), []
    hit = False
    for rx, pat, rgs, why, ln in rules:
        if rx.match(p):
            hit = True
            gs |= set(rgs)
            whys.append(f"{pat}")
    for g in sorted(owners.get(p, ())):
        gs.add(g); whys.append(f"Rust 源 ⇒ {g}")
    # 被 include_str!/include_bytes! 引的文件：内容变了编译不会因此红（引进来的是一段 &str / &[u8]，全仓没有在编译期解析它的 const 求值），
    # 读它的判据在单测里（suites）；只有删掉或改名走了，引它的那棵才会编不过。
    if p in inc_owner and inc_owner[p] and p in gone:
        gs |= inc_owner[p]; whys.append("被 include_str!/include_bytes! 引着却删了 / 改名了 ⇒ " + "/".join(sorted(inc_owner[p])))
    if any(p == t or p.startswith(t + "/") for t in e2e_refs):
        gs.add("e2e"); whys.append("e2e 套件读它 ⇒ e2e")
    hit_suites = suites_touched_by(p)
    if hit_suites:
        e2e_hit.update(hit_suites)
        whys.append(f"{len(hit_suites)} 套 e2e 的被测对象（{' '.join(hit_suites[:4])}{' …' if len(hit_suites) > 4 else ''}）")
    if p.endswith(".sh") or any(fnmatch.fnmatchcase(p, g) for g in sc_globs):
        gs.add("shellcheck"); whys.append("shellcheck 人群")
    if not hit:
        gs.add("FULL"); whys.append("映射表不认识这条路径")
    if "FULL" in gs:
        full_why.append(p)
    want |= gs - {"FULL"}
    out(f"  {p}\n      → {', '.join(sorted(gs)) or '只恒跑'}   （{'；'.join(whys)}）")

if unmapped:
    out(f"  ·  gate.sh 里有、表里没提的格按恒跑：{' '.join(unmapped)}")
if not paths:
    out("  ·  与基相比没有改动 ⇒ 只跑恒跑那组")

if full_why and not trust_ci:
    out(f"\n  ⇒ 全量：{len(full_why)} 条路径要全量（{' · '.join(full_why[:6])}{' …' if len(full_why) > 6 else ''}）")
    print("PLAN-MODE: full"); print("PLAN-CELLS: "); sys.exit(0)
if full_why:   # --trust-ci 下不认识的路径：本机才有的格全跑（CI 那半推了看 CI）
    out(f"\n  ⇒ {len(full_why)} 条路径要全量，--trust-ci 下按「本机才有的格全跑」处理（{' · '.join(full_why[:6])}）")
    want = {g for g in groups if g not in META}
    e2e_hit = set(suites)   # 不认识的路径：哪几套的被测对象动了判不了 ⇒ 全部套件本机照跑
sel = set(always)
for g in want:
    sel |= set(groups[g]) - ((ci_cells - set(groups.get("keep-local", []))) if trust_ci else set())
if trust_ci and e2e_hit:
    sel |= e2e_hit
ordered = [c for c in cells_on_disk if c in sel]
skipped = [c for c in cells_on_disk if c not in sel]
out(f"\n  本机才有、CI 上没有的格：{' '.join(local_only)}")
if trust_ci:
    out("  --trust-ci：被改到但 CI 会跑的格这一趟不跑（推候选分支 next 后看 CI）；例外是被改到被测对象的 e2e 套件，本机照跑")
    out(f"  被改到被测对象的 e2e 套件（{len(e2e_hit)}/{len(suites)}）：{' '.join(c for c in suites if c in e2e_hit) or '（无）'}")
out(f"  ⇒ 跑 {len(ordered)} 格，挡掉 {len(skipped)} 格：{' '.join(skipped) or '（无）'}")
print("PLAN-MODE: " + ("affected" if paths else "none"))
print("PLAN-CELLS: " + " ".join(ordered))
