#!/usr/bin/env python3
# K-R25 · D1 量具 —— 「剥法的输入单位」普查
#
# 住址（唯一，只属于本道）：<代码树>/evidence/K-R25-D1-strip-input-unit-census.py
# 被测对象：**跑它时它自己所在的那棵 git 工作树**（`repo_root()` 从本文件位置往上找 `.git`，
#           不读环境变量、不认相对 cwd）⇒ 换棵树跑就是换个被测对象；输出头一行印出
#           被测对象路径与 `HEAD` sha，报数时连它一起写。
#
# ── 它答的是哪一个问题 ─────────────────────────────────────────────
#   `guard_core` 那几个剥法（`production_code` / `strip_comment_lines` / …）内部的
#   `try_strip_block_comments` 带一条**静默兜底**：词法与交进去的那段文本对不上
#   （扫完 `depth != 0` / 还停在串里）⇒ **一个字都不剥**、原样交回。
#   而看门判据 `assert_block_comment_model_holds` 遍历一棵树、**按整份文件**喂进去
#   ⇒ 它只看得见「整份文件自己不配平」那一档。
#   ⇒ 凡是**交进剥法的那段文本不是「整份文件的原文」**的调用点，看门判据在它上面是**瞎的**。
#   本量具数的就是这个人群，并把「为什么算 / 不算」逐条印出来。
#
# ── 分母怎么切（逐条写明，别只看数） ─────────────────────────────
#   ① 采集面 = `git ls-files '*.rs'`，**排掉** `src-tauri/vendor/`（vendor 不归本仓管）
#      与 `src-tauri/crates/guard-core/src/lib.rs`（剥法本体：它内部的自调用是实现，不是调用点）。
#   ② 命中 = `PRIMS` 里那几个名字（**前面必须是非标识符字符** ⇒ `assert_block_comment_model_holds`
#      不会被算成 `block_comment_model_holds` 的调用）后面紧跟 `(` 的每一处文本出现，再排掉三类：
#      · `trim_start()` 之后以 `//` 打头的行（纯注释行）
#      · `fn <名字>(`（那是**定义**，不是调用点）
#      · 参数是一个**字面量**（`"fn a() {}\n"` 这种自检语料，不涉及任何文件）
#   ③ **输入单位**按参数表达式的文本形态判，四档：
#      · `FILE`     整份文件的原文：`include_str!(…)` · `read_to_string(…)` · `scan_tree!(…)` ·
#                   `read_repo_file(…)` / `must_read(…)` / `source_of(…)` 这类「读一份文件」
#      · `PRESTRIP` 参数解析到一个**已经过过一遍文件级剥法**的文本（例如
#                   `let src = production_code(<整份文件>); … strip_comment_lines(&src[a..b])`）
#                   ⇒ 块注释在文件级那一趟已被抹成等长空格 ⇒ **单位再小也不会新掉进兜底**
#      · `SUBUNIT`  交进去的是**原文的一小块**：按 `#[test]` 切出的块 · 函数体窗口
#                   （`body_of` / `brace_block` / `braced_block`）· 切片 `x[a..b]` ·
#                   `.lines()…join` 派生的行窗口
#      · `PARAM`    参数是本 `fn` 的形参 / 闭包参数 ⇒ 单位由**调用方**决定，
#                   本量具跟不到 ⇒ **它不是「没事」，是「本量具判不了」**，输出里逐条列出来手核。
#   ④ 标识符解析只在**同一个 `fn` 体内**、调用点之前往上找 `let <名> =` / `for <名> in`；
#      找不到就落 `PARAM`。⇒ 本量具**不是**语义分析，是一把说得清射程的文本尺子。
#   ⑤ 另单列 `WATCH` = `assert_block_comment_model_holds(` 的调用点（看门判据本体）。
#
# 用法：python3 evidence/K-R25-D1-strip-input-unit-census.py [--verbose]

import re
import subprocess
import sys
from pathlib import Path

PRIMS = [
    "production_code",
    "strip_comment_lines",
    "strip_block_comments",
    "strip_trailing_comments",
    "production_source",
    "test_source",
    "block_comment_model_holds",
]

WATCH_PRIM = "assert_block_comment_model_holds"

# 「读一份文件 / 一棵树的原文」这一族（判 FILE）。逐条列名，不用通配 ——
# 通配会把 `read_dir` / `read_to_end` 这类一起吃进来。
FILE_READERS = [
    "include_str!",
    "read_to_string",
    "read_repo_file",
    "must_read",
    "source_of",
    "read_rel",
    "scan_tree!",
    "scan_tree_excluding_self",
]

# 「把原文切小」这一族（判 SUBUNIT）。
SUBUNIT_CALLS = [
    "body_of(",
    "brace_block(",
    "braced_block(",
    "chunks_of(",
    ".split(",
    ".splitn(",
    ".split_once(",
]
SLICE_RE = re.compile(r"\[[^\]]*\.\.[^\]]*\]")
LINEWIN_RE = re.compile(r"\.lines\(\)[\s\S]*\.join\(")

EXCLUDE_PREFIX = ("src-tauri/vendor/",)
EXCLUDE_FILES = ("src-tauri/crates/guard-core/src/lib.rs",)


def repo_root() -> Path:
    p = Path(__file__).resolve()
    for up in p.parents:
        if (up / ".git").exists():
            return up
    raise SystemExit("找不到 git 工作树根 —— 本量具靠自身位置定位被测对象")


def head(root: Path) -> str:
    return subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()


def dirty(root: Path) -> list:
    """工作树上被改过的**已跟踪**文件（未跟踪的不算：它们不影响被测内容）。

    ⚠ **为什么要印它**〔09-04 现打，自抓〕：本量具读的是**盘上的文件**，而它印的 sha 是 `HEAD`。
    两者可以不一致 —— 我自己就这么用过一次：把三份源码 `git restore --source=<基点>` 到工作树上
    量「改之前」，而那一趟输出的 sha 是**出货尖**。**输出长得一模一样，那是一次静默的假读数。**
    ⇒ 现在盘上与 `HEAD` 不一致就在头两行喊出来，并逐份点名。
    """
    out = subprocess.run(
        ["git", "-C", str(root), "status", "--porcelain", "--untracked-files=no"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return [l for l in out.split("\n") if l]


def rs_files(root: Path):
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "*.rs"],
        capture_output=True, text=True, check=True,
    ).stdout.split("\n")
    for rel in out:
        if not rel or not rel.endswith(".rs"):
            continue
        if rel.startswith(EXCLUDE_PREFIX) or rel in EXCLUDE_FILES:
            continue
        yield rel


def arg_text(src: str, open_paren: int) -> str:
    """从 `(` 起取到配平的 `)`，返回里面那一段。

    ⚠ **跳过字符串字面量与字符字面量里的括号** —— 少了这一条，
    `body_of(X, "pub async fn f(")` 里那个引号内的 `(` 会把配平算错、
    参数文本一路吃到下一条语句（本量具第一版就是这么错的）。
    """
    depth = 0
    i = open_paren
    n = min(len(src), open_paren + 4000)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    break
                i += 1
            i += 1
            continue
        if c == "'" and i + 2 < n and (src[i + 2] == "'" or src[i + 1] == "\\"):
            j = src.find("'", i + 1)
            i = (j + 1) if j != -1 else (i + 1)
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return src[open_paren + 1 : i]
        i += 1
    return src[open_paren + 1 : open_paren + 201]


FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)", re.M)
IDENT_RE = re.compile(r"^[&*\s]*([A-Za-z_][A-Za-z0-9_]*)\s*$")
LIT_RE = re.compile(r'^[&\s]*(?:b|r|br)?"')


def enclosing_fn(src: str, pos: int) -> tuple[str, int]:
    best = ("<顶层>", 0)
    for m in FN_RE.finditer(src, 0, pos):
        best = (m.group(1), m.start())
    return best


def subunit_base(e: str):
    """一个「切小」表达式的**底座文本**是谁。

    切片 / 窗口本身不说明底座配不配平 —— 底座若是**已经过过文件级剥法**的文本，
    块注释早被抹成空格，切多小都不会新掉进兜底。⇒ 判 SUBUNIT 之前先看底座。
    """
    for c in SUBUNIT_CALLS:
        if c in e:
            op = e.index("(", e.index(c))
            inner = arg_text(e, op)
            return inner.split(",")[0].strip()
    m = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*\[[^\]]*\.\.", e)
    if m:
        return m.group(1)
    m = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*\.lines\(\)", e)
    if m:
        return m.group(1)
    return None


def shape_of(e: str):
    for r in FILE_READERS:
        if r in e:
            return "FILE", f"参数里有 `{r}` ⇒ 整份文件（或整棵树逐份）的原文"
    for c in SUBUNIT_CALLS:
        if c in e:
            return "SUBUNIT", f"参数里有 `{c}` ⇒ 原文切出来的一段"
    if LINEWIN_RE.search(e):
        return "SUBUNIT", "参数是 `.lines()…join(` 派生的**行窗口** ⇒ 原文的一小块"
    if SLICE_RE.search(e):
        return "SUBUNIT", "参数是 `[..]` **切片** ⇒ 原文的一小块"
    return None, ""


def classify(expr: str, src: str, call_pos: int, fn_start: int, depth: int = 0):
    e = " ".join(expr.split())
    if depth > 6:
        return "PARAM", f"解析层数超限，手核：{e[:70]}"
    shape, why = shape_of(e)
    if shape == "SUBUNIT":
        base = subunit_base(e)
        if base:
            bshape, bwhy = classify(base, src, call_pos, fn_start, depth + 1)
            if bshape == "PRESTRIP":
                return "PRESTRIP", f"切小了，但底座 `{base}` 已过过文件级剥法 ⇒ {bwhy}"
        return "SUBUNIT", why + (f"（底座 `{base}` 是原文）" if base else "")
    if shape:
        return shape, why
    m = IDENT_RE.match(e)
    if not m:
        # 方法链（`sources.iter().find(..).map(..).unwrap_or_else(..)` 这一族）：
        # 上面几条「切小」的形态都没命中 ⇒ 它只是**从一堆里挑一份 / 解引用 / 克隆**
        # ⇒ 单位跟着**链头那个名字**走。链头逐字印在判据栏里，便于手核。
        mc = re.match(r"^[&*\s]*([A-Za-z_][A-Za-z0-9_]*)\s*\.", e)
        if mc:
            s3, w3 = classify(mc.group(1), src, call_pos, fn_start, depth + 1)
            return s3, f"方法链 `{e[:60]}` 的链头是 `{mc.group(1)}` ⇒ {w3}"
        return "PARAM", f"既不是读文件、也不是可解析的名字，手核：{e[:70]}"
    name = m.group(1)
    body = src[fn_start:call_pos]
    pats = [
        re.compile(r"\blet\s+(?:mut\s+)?" + re.escape(name) + r"\s*(?::[^=;]*)?=\s*([^;]*);", re.S),
        re.compile(r"\bfor\s+" + re.escape(name) + r"\s+in\s+([^{]*)\{", re.S),
        re.compile(r"\bfor\s*\(\s*[A-Za-z0-9_]+\s*,\s*" + re.escape(name) + r"\s*\)\s+in\s+([^{]*)\{", re.S),
    ]
    hits = []
    for p in pats:
        hits += [(m2.start(), m2.group(1)) for m2 in p.finditer(body)]
    if hits:
        hits.sort()
        rhs = " ".join(hits[-1][1].split())
        # 已经过过一遍**文件级**剥法的文本 ⇒ 块注释早被抹平，单位再小也不新掉兜底。
        hit_prim = next(
            (p for p in PRIMS if re.search(r"(?<![A-Za-z0-9_])" + p + r"\s*\(", rhs)), None
        )
        if hit_prim:
            op = rhs.index("(", rhs.index(hit_prim))
            inner = arg_text(rhs, op)
            sub, _ = classify(inner, src, call_pos, fn_start, depth + 1)
            if sub in ("FILE", "PRESTRIP"):
                return "PRESTRIP", f"`{name}` = 文件级剥法的产物（{rhs[:70]}）"
            return "SUBUNIT", f"`{name}` = {rhs[:70]}（**块级**剥法的产物）"
        return classify(rhs, src, call_pos, fn_start, depth + 1)
    # ── 找不到 `let` / `for` 来历，再试三条路 ─────────────────────────
    # ① 整份文件里的 `const <名>: &str = …;`（`MONITOR_TMUX` / `FALLBACK` 这一族）
    m2 = re.search(r"\bconst\s+" + re.escape(name) + r"\s*:[^=]*=\s*([^;]*);", src, re.S)
    if m2:
        return classify(" ".join(m2.group(1).split()), src, call_pos, fn_start, depth + 1)
    # ② 闭包参数：`|(p, s)|` / `|s|` —— 底座看它挂在哪条语句上（那条语句里有读文件就算 FILE）
    for cm in re.finditer(r"\|[^|\n]*\b" + re.escape(name) + r"\b[^|\n]*\|", body):
        seg_start = body.rfind("let ", 0, cm.start())
        seg = body[max(0, seg_start if seg_start != -1 else cm.start() - 400) : cm.end() + 400]
        s2, w2 = shape_of(" ".join(seg.split()))
        if s2 == "FILE":
            return "FILE", f"`{name}` 是闭包参数，挂在一条读文件的语句上 ⇒ {w2}"
    # ③ 本 fn 的形参 ⇒ 单位由调用方定：把同一份文件里对本 fn 的调用逐个判，全 FILE 才算 FILE
    fname_m = FN_RE.match(src[fn_start:])
    if fname_m:
        fname = fname_m.group(1)
        sig_end = src.find(")", fn_start)
        sig = src[fn_start:sig_end] if sig_end != -1 else ""
        if re.search(r"\b" + re.escape(name) + r"\s*:", sig):
            verdicts = []
            for cm in re.finditer(r"(?<![A-Za-z0-9_])" + re.escape(fname) + r"\s*\(", src):
                if cm.start() == fn_start or src[max(0, cm.start() - 4) : cm.start()].endswith("fn "):
                    continue
                op = src.index("(", cm.start())
                cfn, cfs = enclosing_fn(src, cm.start())
                verdicts.append(classify(arg_text(src, op), src, cm.start(), cfs, depth + 1)[0])
            if verdicts and set(verdicts) <= {"FILE"}:
                return "FILE", f"`{name}` 是 `{fname}` 的形参；本文件里 {len(verdicts)} 处调用全是整份文件"
            if verdicts:
                return "PARAM", f"`{name}` 是 `{fname}` 的形参；调用方判出 {sorted(set(verdicts))} ⇒ 手核"
    return "PARAM", f"`{name}` 在本 fn 体内找不到 `let` / `for` / `const` / 闭包来历 ⇒ 手核"


def main():
    verbose = "--verbose" in sys.argv
    root = repo_root()
    sha = head(root)
    buckets = {"FILE": [], "PRESTRIP": [], "SUBUNIT": [], "PARAM": [], "WATCH": []}
    per_prim = {}
    dropped = {"注释行": 0, "fn 定义": 0, "字面量语料": 0}
    for rel in rs_files(root):
        src = (root / rel).read_text(encoding="utf-8", errors="replace")
        line_starts = [0]
        for i, ch in enumerate(src):
            if ch == "\n":
                line_starts.append(i + 1)

        def lineno(p):
            lo, hi = 0, len(line_starts) - 1
            while lo < hi:
                mid = (lo + hi + 1) // 2
                if line_starts[mid] <= p:
                    lo = mid
                else:
                    hi = mid - 1
            return lo + 1

        for prim in PRIMS + [WATCH_PRIM]:
            for m in re.finditer(r"(?<![A-Za-z0-9_])" + re.escape(prim) + r"\s*\(", src):
                pos = m.start()
                ln = lineno(pos)
                nxt = line_starts[ln] if ln < len(line_starts) else len(src)
                line = src[line_starts[ln - 1] : nxt]
                if line.lstrip().startswith("//"):
                    dropped["注释行"] += 1
                    continue
                if src[max(0, pos - 4) : pos].endswith("fn "):
                    dropped["fn 定义"] += 1
                    continue
                op = src.index("(", pos)
                expr = arg_text(src, op)
                fname, fstart = enclosing_fn(src, pos)
                if prim == WATCH_PRIM:
                    bucket, why = "WATCH", "看门判据本体（遍历一棵树、按整份文件喂）"
                elif LIT_RE.match(" ".join(expr.split())):
                    dropped["字面量语料"] += 1
                    continue
                else:
                    bucket, why = classify(expr, src, pos, fstart)
                rec = (f"{rel}:{ln}", prim, fname, " ".join(expr.split())[:100], why)
                buckets[bucket].append(rec)
                per_prim.setdefault(prim, {}).setdefault(bucket, 0)
                per_prim[prim][bucket] += 1

    total = sum(len(v) for v in buckets.values())
    print(f"【K-R25 D1 · 剥法输入单位普查】被测对象 = {root}  @ {sha}")
    d = dirty(root)
    if d:
        print(f"🔴 **盘上与 `HEAD` 不一致：{len(d)} 份已跟踪文件被改过** ⇒ 上面那个 sha "
              f"**不是**本趟真正量的内容，报数时要连这几份一起写：")
        for line in d:
            print(f"     {line}")
    else:
        print("盘上与 `HEAD` 一致（已跟踪文件零改动）⇒ 上面那个 sha 就是本趟量的内容")
    print("采集面：`git ls-files '*.rs'`，排掉 vendor 与 guard-core 本体")
    print(f"命中总数 {total}（另排掉：" + " · ".join(f"{k} {v} 处" for k, v in dropped.items()) + "）")
    print()
    for b in ("FILE", "PRESTRIP", "SUBUNIT", "PARAM", "WATCH"):
        print(f"  {b:<11}{len(buckets[b]):>4}")
    print()
    print("按剥法逐个：")
    for prim in PRIMS + [WATCH_PRIM]:
        d = per_prim.get(prim, {})
        if d:
            print(f"  {prim:<34}" + "  ".join(f"{k}={v}" for k, v in sorted(d.items())))
    print()
    for b in ("SUBUNIT", "PARAM", "WATCH"):
        print(f"── {b}（{len(buckets[b])} 处）逐条 ──")
        for rec in sorted(buckets[b]):
            print(f"  {rec[0]:<52} {rec[1]:<24} fn {rec[2]}")
            print(f"      参数：{rec[3]}")
            print(f"      判据：{rec[4]}")
        print()
    if verbose:
        print("── FILE / PRESTRIP 逐条 ──")
        for b in ("FILE", "PRESTRIP"):
            for rec in sorted(buckets[b]):
                print(f"  [{b}] {rec[0]:<48} {rec[1]:<22} {rec[3][:64]}")


if __name__ == "__main__":
    main()
