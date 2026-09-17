#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R18 摸底量具：判「一把尺子枚举的集合，是不是它标签说的那个集合」。

本件只摸底 —— 这份量具**只读**，不写任何被测文件、不跑任何测试、不改任何判据。

═══════════════════════════════════════════════════════════════════════════
🔴 尺子作用域（本仓最高频的一类错就是这一格没写，见件文件 §1 语料一）
═══════════════════════════════════════════════════════════════════════════

被测面由 `--tree` + `--kind` 显式给出，**本文件里没有写死任何树路径**
（`K-R17-PM-粗尺子.py` 写死了主树，照住址在工作树上复跑会量到另一棵树）。

  --kind plan   分母 = `git ls-files '*.md'`（可再用 --sub 收窄到某个工作区目录）
  --kind rs     分母 = `git ls-files '*.rs'`，排 `vendor/` 与 `code-picture-core`

⚠ **不用 shell 的 `grep`**：本机 `grep` 是一个包着 ugrep 的 shell 函数
（`ugrep 7.8.4` + `--ignore-files --hidden --exclude-dir=.git`）——
`--ignore-files` 会去读 `.gitignore`，所以「同一条 `grep -r` 命令」在本机和在
别处枚举的**不是同一个集合**。本量具一律走 `git ls-files` + Python 读文件。

═══════════════════════════════════════════════════════════════════════════
三把候选尺子（各自的人群与「红」的定义写死在下面，别在报数时改口径）
═══════════════════════════════════════════════════════════════════════════

R1 「同行对拍」（件文件 §3 KR18D1 方向①）
    人群：**一行**里同时出现 ① 一个**计数惯用法**（它枚举的单位机器已知）
          与 ② 一个 `数字 + 量词` 标签。
    红  ：惯用法枚举的单位类 ≠ 标签量词的单位类。
    🔴 它需要两张手写表（IDIOM_UNIT 与 UNIT_CLASS）—— **这正是 KR18D2 那条红线的形状**，
       所以本量具把两张表的规模也打印出来，让 PM 自己判它是不是「一份要人维护的词表」。

R2 「引文逐字对拍」（K25d ⑤ 逐字写了这个动作，而 K-R6 那四张网一张都没实现它）
    人群：一段 `「…」` 引文，且**同一行**点名了一个仓里解析得到的 `*.rs` / `*.md` 文件。
    红  ：那段引文在被点名的文件里**逐字找不到**。
    ★ 这一把**不需要任何词表** —— 它判的是「这串字节在不在那个文件里」。

R3 「时态标记」判别（件文件 §1 语料五指定的第一试）
    人群：一个给定的针（`--needle`）在被测面上的每一处命中。
    绿  ：命中所在的**注释块**里带时态标记（TENSE_MARKERS）⇒ 判为历史引用。
    红  ：不带 ⇒ 判为「声称现状」。
    🔴 TENSE_MARKERS 是一张**要人维护的词表**，本仓已有一次实测说它走不通
       （`src-tauri/src/frame_cadence_guard.rs::forbidden` 的 `///`：
       「试跑那条规则今天**误红 7 处**，全是合法文本」）。本尺子跑它是为了**复打**那个结论，
       不是为了装它。

跑法：
    python3 evidence/K-R18-摸底-尺子标签对拍.py --tree <树的绝对路径> --ruler r1 --kind plan
    python3 evidence/K-R18-摸底-尺子标签对拍.py --tree <树> --ruler r2 --kind rs
    python3 evidence/K-R18-摸底-尺子标签对拍.py --tree <树> --ruler r3 --kind rs \
        --needle launch_identity_prefix --rev 4cf4301
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import sys

# ═══════════════════════════════════════════════════════════════════════
# R1 的两张手写表 —— 🔴 规模会被打印出来，它是「这条路是不是词表」的证据
# ═══════════════════════════════════════════════════════════════════════

# 计数惯用法 → 它**真正枚举**的单位类。键按长度倒序匹配（长的先）。
IDIOM_UNIT: dict[str, str] = {
    "grep -c": "LINE",  # 含命中的行数，一行多命中只算 1
    "grep -rc": "LINE",
    "git grep -c": "LINE",
    "grep -oc": "LINE",  # -c 压过 -o
    "grep -l": "FILE",
    "grep -rl": "FILE",
    "git grep -l": "FILE",
    "grep -o": "OCC",  # 只有再管道到 wc -l 才是处；单独 -o 是逐行打印
    "grep -roh": "OCC",
    "grep -oh": "OCC",
    "grep -oE": "OCC",
    "wc -l": "LINE",
    "ls -1": "DIRENT",
    "os.listdir": "DIRENT",
    "find ": "PATH",
    "git ls-files": "PATH",
}

# 量词 → 它**标签上说**的单位类。
UNIT_CLASS: dict[str, str] = {
    "行": "LINE",
    "处": "OCC",
    "次": "OCC",
    "句": "OCC",
    "个": "OCC",
    "件": "FILE",
    "份": "FILE",
    "文件": "FILE",
    "条": "ITEM",  # 🔴 本仓方言里 `条` 既能指「判据条数」也能指「命中条数」——歧义
    "支": "ITEM",
    "张": "ITEM",
    "格": "ITEM",
    "组": "ITEM",
}

# 单位类之间「算不算同一个集合」。ITEM 与谁都不判（歧义 ⇒ 弃权，不猜）。
COMPATIBLE = {
    ("LINE", "LINE"),
    ("OCC", "OCC"),
    ("FILE", "FILE"),
    ("PATH", "FILE"),
    ("PATH", "PATH"),
    ("DIRENT", "DIRENT"),
}

# R3 的时态标记词表 —— 🔴 就是 frame_cadence_guard 实测走不通的那一形，复打用
TENSE_MARKERS = [
    "上一版",
    "先前",
    "原写",
    "原先",
    "此前",
    "改名之后",
    "改名为",
    "已改名",
    "已删",
    "之前是",
    "曾",
    "旧名",
    "历史",
    "墓碑",
    "反面教材",
    "当时",
]

IDIOM_KEYS = sorted(IDIOM_UNIT, key=len, reverse=True)
LABEL_RE = re.compile(r"([0-9０-９]+)\s*(" + "|".join(sorted(UNIT_CLASS, key=len, reverse=True)) + ")")
# `「…」` 引文；本仓引文一律用全角直角引号
QUOTE_RE = re.compile(r"「([^「」]{8,400})」")
FILE_RE = re.compile(r"[A-Za-z0-9_./-]+\.(?:rs|md|py|ts|sh|yml)")


def sh(tree: str, *args: str) -> str:
    """跑一条 git。

    🔴🔴 `-c core.quotePath=false` 不是可选项，是本量具**自己踩过的那个坑**：
    `git ls-tree -r --name-only` 默认把非 ASCII 文件名**八进制转义并加引号**输出
    （`"backend-consolidation/features/K-A1-\\351\\211\\264….md"`）⇒ 下游任何
    `endswith(".md")` / `grep '\\.md$'` 都会**静默漏掉每一个中文名文件**。
    现打（`planned-build` @ `208051c`，613 条跟踪条目）：
      · `git ls-files '*.md' | wc -l`                       ⇒ **606**（git 自己的 pathspec 匹配器）
      · `git ls-tree -r --name-only HEAD | grep -c '\\.md$'` ⇒ **387**（下游字符串匹配器）
    **两份清单逐条相同（`diff` 空），两个数差 219** —— 差的全是中文名，
    而本仓的件文件**全部**是中文名。
    ⇒ 这正是本件要判的那个病：尺子枚举的是「名字全 ASCII 的 .md」，标签说的是「.md」。
    ⇒ 它是被 **KR18D1 方向②（两把尺子对拍）** 逮到的：87 vs 132 对不上才发现。
    """
    # errors="replace"：树里有二进制（png 等），`--kind all` 会读到它们。
    return subprocess.run(
        ["git", "-C", tree, "-c", "core.quotePath=false", *args],
        capture_output=True,
        text=True,
        errors="replace",
        check=True,
    ).stdout


def corpus(tree: str, kind: str, sub: str | None, rev: str | None) -> list[str]:
    """分母。**这一步就是本件要治的那个病最容易长出来的地方**，所以口径写死在这里。"""
    if rev:
        out = sh(tree, "ls-tree", "-r", "--name-only", rev)
    else:
        out = sh(tree, "ls-files")
    files = [f for f in out.splitlines() if f]
    if kind == "plan":
        files = [f for f in files if f.endswith(".md")]
    elif kind == "rs":
        files = [
            f
            for f in files
            if f.endswith(".rs") and "vendor/" not in f and "code-picture-core" not in f
        ]
    elif kind == "all":
        pass
    else:
        raise SystemExit(f"未知 --kind {kind}")
    if sub:
        files = [f for f in files if f.startswith(sub)]
    files = sorted(files)

    # ── KR18D1 方向② 的活体样品：拿第二把尺子对拍自己的分母 ──────────────
    # 尺子甲（上面）：git 出清单 → Python 用 `endswith` 筛。
    # 尺子乙（这里）：把筛选交给 **git 自己的 pathspec 匹配器**，不经过下游字符串。
    # 两把尺子量的该是同一个集合；不等就是「枚举的 ≠ 标签说的」，当场喊出来。
    # 🔴 这条自检不是装饰：它就是逮到本量具那个 `quotePath` 漏 219 份的东西。
    if kind in ("plan", "rs") and not rev:
        ext = "*.md" if kind == "plan" else "*.rs"
        pathspec = f"{sub}{ext}" if sub else ext
        b = [f for f in sh(tree, "ls-files", pathspec).splitlines() if f]
        if kind == "rs":
            b = [f for f in b if "vendor/" not in f and "code-picture-core" not in f]
        if len(b) != len(files):
            print(
                f"🔴 分母对拍不等：endswith 口径 {len(files)} 份 vs "
                f"git pathspec 口径 {len(b)} 份 —— 尺子枚举的不是标签说的那个集合",
                file=sys.stderr,
            )
    return files


def read(tree: str, path: str, rev: str | None) -> str:
    if rev:
        try:
            return sh(tree, "show", f"{rev}:{path}")
        except subprocess.CalledProcessError:
            return ""
    p = os.path.join(tree, path)
    try:
        with open(p, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


# ═══════════════════════════════════════════════════════════════════════
# R1
# ═══════════════════════════════════════════════════════════════════════


def ruler_r1(tree, files, rev, dump):
    hits = []  # (path, lineno, idiom, idiom_unit, label, label_unit, verdict, text)
    idiom_occ = 0
    idiom_lines = 0
    for path in files:
        text = read(tree, path, rev)
        for i, line in enumerate(text.splitlines(), 1):
            found = None
            for k in IDIOM_KEYS:
                if k in line:
                    found = k
                    break
            if not found:
                continue
            idiom_lines += 1
            idiom_occ += sum(line.count(k) for k in ("grep -c",))
            labels = LABEL_RE.findall(line)
            if not labels:
                continue
            iu = IDIOM_UNIT[found]
            for num, unit in labels:
                lu = UNIT_CLASS[unit]
                if lu == "ITEM" or iu == "ITEM":
                    verdict = "弃权"
                elif (iu, lu) in COMPATIBLE or (lu, iu) in COMPATIBLE:
                    verdict = "绿"
                else:
                    verdict = "红"
                hits.append((path, i, found, iu, f"{num}{unit}", lu, verdict, line.strip()))
    print(f"[R1] 分母：{len(files)} 份文件")
    print(f"[R1] 含计数惯用法的**行**：{idiom_lines}（行口径，一行多个惯用法只算 1）")
    print(f"[R1] `grep -c` 的**处**数（子集，逐处累加）：{idiom_occ}")
    print(f"[R1] 同行带 `数字+量词` 标签的**对**：{len(hits)}（一行多个标签会记多次）")
    for v in ("红", "绿", "弃权"):
        n = sum(1 for h in hits if h[6] == v)
        print(f"[R1]   {v}：{n}")
    print(f"[R1] 手写表规模：IDIOM_UNIT {len(IDIOM_UNIT)} 条 · UNIT_CLASS {len(UNIT_CLASS)} 条")
    if dump:
        for h in hits:
            if dump == "all" or h[6] == dump:
                print(f"  {h[6]}  {h[0]}:{h[1]}  [{h[2]}⇒{h[3]}] vs [{h[4]}⇒{h[5]}]")
                print(f"        {h[7][:300]}")
    return hits


# ═══════════════════════════════════════════════════════════════════════
# R2
# ═══════════════════════════════════════════════════════════════════════


def norm(s: str) -> str:
    """比对前的归一。**这一步每加一条都在放宽判决，所以逐条写出来。**

    ① 去 markdown 强调（`**` / `*`）与反引号 —— 引的时候常加粗，原文没加粗；
    ② 去注释前缀（`///` `//!` `//` `#`）—— 被引原文若是跨行 doc 注释，
       每一行都带前缀，而引文里没有；
    ③ 去全部空白 —— 换行位置在引与被引两侧几乎不可能一致。
    🔴 这三条一起，把「逐字」放宽成了「去掉这些之后逐字」。**它买不到真正的逐字。**
    """
    s = s.replace("**", "").replace("`", "").replace("*", "")
    s = re.sub(r"(?m)^\s*(///|//!|//|#)\s?", "", s)
    s = s.replace("///", "").replace("//!", "")
    return re.sub(r"\s+", "", s)


def ruler_r2(tree, files, rev, dump, verbatim_only=False):
    """R2b（`--verbatim`）：只收**写的人自己声明「逐字」**的那些引文。

    🔴 为什么要有这一档：R2 原样跑下来 113 判得了 / 77 判红，逐条读过之后
    绝大多数是**转述**，不是逐字引用 —— `「…」` 在本仓方言里既当逐字引号
    **也**当概念引号。⇒ **R2 自己就犯了本件要判的病**：
    标签说「引文」，枚举的是「任何一对 `「」`」。
    ⇒ R2b 把人群换成**由作者声明**的那一档（同行出现「逐字」二字），
    这正是 `K-R17` 那条出路的形状：**不猜，让写的人声明**。
    """
    index: dict[str, str] = {}
    all_files = corpus(tree, "all", None, rev)
    by_base: dict[str, list[str]] = {}
    for f in all_files:
        by_base.setdefault(os.path.basename(f), []).append(f)

    total = 0
    named = 0
    resolved = 0
    ok = 0
    bad = []
    ambiguous = 0
    for path in files:
        text = read(tree, path, rev)
        for i, line in enumerate(text.splitlines(), 1):
            if verbatim_only and "逐字" not in line:
                continue
            for q in QUOTE_RE.findall(line):
                total += 1
                cands = FILE_RE.findall(line)
                cands = [c for c in cands if os.path.basename(c) in by_base]
                if not cands:
                    continue
                named += 1
                base = os.path.basename(cands[0])
                paths = by_base[base]
                if len(paths) > 1:
                    ambiguous += 1
                    continue
                target = paths[0]
                if target == path:
                    continue
                resolved += 1
                body = index.get(target)
                if body is None:
                    body = norm(read(tree, target, rev))
                    index[target] = body
                if norm(q) in body:
                    ok += 1
                else:
                    bad.append((path, i, target, q[:160]))
    print(f"[R2] 分母：{len(files)} 份文件")
    print(f"[R2] `「…」` 引文总数：{total}")
    print(f"[R2]   其中同行点名了一个仓里存在的文件：{named}")
    print(f"[R2]   其中同名多份（歧义，弃权不判）：{ambiguous}")
    print(f"[R2]   **可判人群**（解析得到、且不是自引）：{resolved}")
    print(f"[R2]   逐字对上：{ok}")
    print(f"[R2]   🔴 逐字对不上：{len(bad)}")
    if dump:
        for b in bad:
            print(f"  红  {b[0]}:{b[1]}  ⇒ {b[2]}")
            print(f"        「{b[3]}」")
    return bad


# ═══════════════════════════════════════════════════════════════════════
# R3
# ═══════════════════════════════════════════════════════════════════════


def block_of(lines: list[str], i: int, width: int) -> str:
    lo = max(0, i - width)
    hi = min(len(lines), i + width + 1)
    return "\n".join(lines[lo:hi])


def ruler_r3(tree, files, rev, needle, width, dump):
    hits = []
    for path in files:
        text = read(tree, path, rev)
        lines = text.splitlines()
        for i, line in enumerate(lines):
            if needle not in line:
                continue
            ctx = block_of(lines, i, width)
            marks = [m for m in TENSE_MARKERS if m in ctx]
            hits.append((path, i + 1, bool(marks), marks, line.strip()))
    red = [h for h in hits if not h[2]]
    print(f"[R3] 针：`{needle}`  窗口：±{width} 行  分母：{len(files)} 份文件")
    print(f"[R3] 针命中：{len(hits)} 行")
    print(f"[R3] 带时态标记（判绿 = 历史引用）：{len(hits) - len(red)}")
    print(f"[R3] 🔴 不带（判红 = 声称现状）：{len(red)}")
    print(f"[R3] 词表规模：TENSE_MARKERS {len(TENSE_MARKERS)} 条")
    if dump:
        for h in hits:
            tag = "绿" if h[2] else "红"
            print(f"  {tag}  {h[0]}:{h[1]}  标记={h[3]}")
            print(f"        {h[4][:220]}")
    return hits


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", required=True)
    ap.add_argument("--ruler", required=True, choices=["r1", "r2", "r3"])
    ap.add_argument("--kind", default="plan", choices=["plan", "rs", "all"])
    ap.add_argument("--sub", default=None, help="收窄到某个子目录前缀")
    ap.add_argument("--rev", default=None, help="量于哪个提交（不给 = 工作树现状）")
    ap.add_argument("--needle", default=None)
    ap.add_argument("--width", type=int, default=0, help="R3 的上下文窗口半径（行）")
    ap.add_argument("--dump", default=None, help="all / 红 / 绿 / 弃权")
    ap.add_argument("--verbatim", action="store_true", help="R2 只收同行声明「逐字」的引文")
    a = ap.parse_args()

    files = corpus(a.tree, a.kind, a.sub, a.rev)
    head = sh(a.tree, "rev-parse", "HEAD").strip()
    print(f"# 被测树：{a.tree}")
    print(f"# 树尖：{head}   量于：{a.rev or '工作树现状'}")
    print(f"# 分母口径：--kind {a.kind}" + (f" --sub {a.sub}" if a.sub else ""))
    digest = hashlib.md5("\n".join(files).encode()).hexdigest()[:12]
    print(f"# 分母清单 md5：{digest}（{len(files)} 份）")
    print()

    if a.ruler == "r1":
        ruler_r1(a.tree, files, a.rev, a.dump)
    elif a.ruler == "r2":
        ruler_r2(a.tree, files, a.rev, a.dump, a.verbatim)
    else:
        if not a.needle:
            raise SystemExit("--ruler r3 要 --needle")
        ruler_r3(a.tree, files, a.rev, a.needle, a.width, a.dump)
    return 0


if __name__ == "__main__":
    sys.exit(main())
