#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""KR61D3 的尺子 —— 「两棵 Rust 树的注释行里，反引号括起来的仓内路径，今天还在不在盘上」。

住址：本文件（scratchpad 下，名字带 `kr61-` 前缀，独属本轮 —— brief 12④「量具住址要能唯一定位」）。
被测对象：由 argv[1] 给（本轮量的是 /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61）。
用法：python3 kr61-dangling-ruler.py <树的绝对路径>

与 §0d 那把尺子的差别（逐条，这就是 KR61D3 点名要修的三处 + 我自己加的两处）：
  ① 补上 `src-tauri/crates` 这个根（§0d 抽验时自陈漏掉的那个）；
  ② **放开无后缀路径** —— §0d 的 EXT 白名单把 `shared/ccm` 这一形整个滤掉了，
     而那正是本件在治的样本（「尺子逮不到自己要量的东西」）；
  ③ 剥掉通配/占位与计划目录；
  ④ 〔我加的〕**候选根加上「引用它的那个文件自己的目录 + 它的上级」** ——
     Rust 注释里点兄弟模块用的是模块相对路径（`common/paths.rs` 写在 `agents/claudecode/paths.rs` 里），
     少了这两个根会把一堆活的引用误报成悬空；
  ⑤ 〔我加的〕**分桶，不把「带 `/` 的串」一律当路径声称**。判「它是不是在声称一个仓内路径」用
     两条可复核的判据（见 `classify`），够不上的单列进「不是路径声称」桶，**不进 ★ 分母**。

自检（跑完必看）：★ 名单里必须有 `shared/ccm`（已知样本）—— 没有就是尺子坏了，读数作废。
"""
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(sys.argv[1])

TREES = ["src-tauri/src", "remote-daemon-proto/src"]

# 全局候选根：§0d 那七个 + `src-tauri/crates`（第 8 个）
GLOBAL_BASES = [
    "", "src-tauri", "remote-daemon-proto",
    "src-tauri/src", "remote-daemon-proto/src",
    "..", "src", "src-tauri/crates",
]

TOKEN = re.compile(r"`([^`\n]{1,200})`")
PATHISH = re.compile(r"^[A-Za-z0-9_.][A-Za-z0-9_./+-]*$")
LINENO = re.compile(r":\d+(?:-\d+)?$")
SRC_EXT = (".rs", ".ts", ".tsx", ".sh", ".md", ".json", ".toml", ".tsv", ".py", ".yml", ".yaml", ".html", ".css")

PLAN_PREFIXES = ("audits/", "features/", "planned-build/", ".claude/", "references/", "profiles/", "plan/")
SCRATCH_HINT = ("scratchpad/", "/tmp/")


def comment_lines(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        for i, line in enumerate(f, 1):
            if line.lstrip().startswith("//"):
                yield i, line.rstrip("\n")


def raw_candidates(text):
    out = []
    for m in TOKEN.finditer(text):
        raw = m.group(1).strip()
        val = LINENO.sub("", raw)
        if not val or "://" in val or " " in val or "/" not in val:
            continue
        if any(c in val for c in "*?<>{}"):          # 通配 / 占位
            continue
        if val.startswith(("/", "~", "./", "@")):
            continue
        if not PATHISH.match(val):
            continue
        out.append(val)
    return out


def bases_for(relfile):
    d = os.path.dirname(relfile)
    return GLOBAL_BASES + [d, os.path.dirname(d)]


def resolve(val, bases):
    for b in bases:
        p = os.path.join(ROOT, b, val) if b else os.path.join(ROOT, val)
        if os.path.exists(p):
            return b or "<仓根>"
    return None


def prefix_depth(val, bases):
    """在某个根下，这个值的**祖先前缀**最多能落实几段（0 = 连第一段都不是真目录）。"""
    segs = [s for s in val.split("/") if s]
    best = 0
    for b in bases:
        base = os.path.join(ROOT, b) if b else ROOT
        cur = base
        n = 0
        for s in segs[:-1]:
            cur = os.path.join(cur, s)
            if os.path.isdir(cur):
                n += 1
            else:
                break
        best = max(best, n)
    return best


PLACEHOLDER_SEG = re.compile(r"^(x+|x+\..*|\.\.\.)$")


def classify(val, bases):
    """它在不在「声称一个仓内路径」。两条判据，够上任一条就算：
       (a) 带已知源码后缀；(b) 祖先前缀在盘上落实得了至少一段（⇒ 它确实指进这棵树）。
       都够不上 ⇒ 「不是路径声称」（`if/else` · `u64/usize` · `origin/main` 这一族）。"""
    if any(PLACEHOLDER_SEG.match(s) for s in val.split("/")):
        # 占位的另一形：不写 `*` 而写 `xxx` / `x` / `...`（§0d 的通配桶只逮得到 `*`）
        return "占位"
    if val.endswith("/"):
        return "目录形"
    if val.startswith(PLAN_PREFIXES) or any(s in val for s in SCRATCH_HINT):
        return "计划目录"
    if val.endswith(SRC_EXT):
        return "路径声称"
    if prefix_depth(val, bases) >= 1:
        return "路径声称"
    return "不是路径声称"


hits = {}           # value -> [(relfile, lineno)]
for tree in TREES:
    for dirpath, _dirnames, filenames in os.walk(os.path.join(ROOT, tree)):
        for fn in filenames:
            if not fn.endswith(".rs"):
                continue
            full = os.path.join(dirpath, fn)
            rel = os.path.relpath(full, ROOT)
            for ln, line in comment_lines(full):
                for v in raw_candidates(line):
                    hits.setdefault(v, []).append((rel, ln))

buckets = {"路径声称": {}, "目录形": {}, "计划目录": {}, "不是路径声称": {}, "占位": {}}
for v, places in hits.items():
    bases = bases_for(places[0][0])
    buckets[classify(v, bases)][v] = places

dangling, live = {}, {}
for v, places in buckets["路径声称"].items():
    # 逐处判：同一个值在不同文件里根不同（模块相对），有一处解得开就算活的
    if any(resolve(v, bases_for(f)) for f, _ in places):
        live[v] = places
    else:
        dangling[v] = places


def n(d):
    return f"{len(d)} 值 / {sum(len(v) for v in d.values())} 处"


head = subprocess.run(["git", "-C", ROOT, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
print(f"# 被测对象：{ROOT}")
print(f"# 提交：{head}")
print(f"# 树：{TREES} · 全局根 {len(GLOBAL_BASES)} 个（含 src-tauri/crates）+ 每处 2 个模块相对根")
print()
print(f"总命中（反引号里带 `/` 的串）      {n(hits)}   ← 分母")
print(f"├ 占位（xxx / x / ... 这一形）      {n(buckets['占位'])}")
print(f"├ 不是路径声称（if/else 这一族）   {n(buckets['不是路径声称'])}")
print(f"├ 目录形（以 `/` 收尾）             {n(buckets['目录形'])}")
print(f"├ 计划目录 / scratchpad             {n(buckets['计划目录'])}")
print(f"├ 路径声称·解得开                   {n(live)}")
print(f"└ ★ 路径声称·**解不开 = 真悬空**    {n(dangling)}")
print()
print("== ★ 真悬空 · 完整名单（值 ⇥ 处数 ⇥ 逐处住址）==")
for v in sorted(dangling, key=lambda k: (-len(dangling[k]), k)):
    ps = dangling[v]
    print(f"{v}\t{len(ps)}\t{' , '.join(f'{f}:{l}' for f, l in ps)}")
print()
print("== 尺子自检：已知样本 `shared/ccm` 在不在 ★ 名单里 ==")
print("命中 ✓" if "shared/ccm" in dangling else "🔴 没命中 —— 尺子坏了，上面的读数作废")
print()
print("== 参考：被判「不是路径声称」的全部值（复核用，别当结论）==")
print(" · ".join(sorted(buckets["不是路径声称"])))
print()
print("== 参考：被判「目录形」的全部值（复核用）==")
print(" · ".join(sorted(buckets["目录形"])))
