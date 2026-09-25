#!/usr/bin/env python3
"""P27：一条判据点不点得出「它服务哪条业务要求」—— 三层口径，可复算。

为什么有这把尺子
================
`真相源/105`（换轴普查）给了一个数：**4 668 条 Rust+TS 里 3 874 条（82%）
一个业务要求住址都点不出**。`P20` 按那个数开工，而开工第一天量出一件事：
**那个数只算「判据自己那段散文」，不认本仓的头注约定。**

本仓的约定是明文写着的 —— `comm_boundary_registry_tests.rs` 第一行逐字：
「模块头注（这张表为什么存在 · 成员怎么认 · 买到什么买不到什么）住
`src/bridge/src/comm_boundary_registry.rs`，**不在这里抄第二份**」。
⇒ 判据文件由生产模块 `#[path]` 收留，住址按约定住生产侧。只读判据自己那几行
去问「它服务哪条要求」，对这一族**在构造上答不出**。

所以本尺子分三层各出一个数，**不合并**：
  ① 判据自己（紧邻注释 ＋ 函数体）里有住址
  ② 它所在判据文件的 `//!` 头注里有
  ③ 收留它的生产模块的 `//!` 头注里有
三层都没有 ⇒ 真的点不出。
〔JA1 2026-09-24〕另出第四个数 ④：判据文件头注标了 `〔缺址〕` 的族 —— 有人逐字核过原文、
**主住址缺**（该升格成条，或设计篇该补一节）。它从 ②③ 里摘出来单列，不算「点到」，
也不再混在「三层都点不出」里（那一栏只剩**没人核过**的）。

⚠ 它买到什么、买不到什么
========================
**买到**：一个每趟现算、可复算的四元组（①②③ ＋ 三层都没有）。
**买不到**：
  · 「有住址」不等于**住址对**。本尺子只认形状（`条 N` / `§N` / `F47` / `D1` /
    `铁律` / `INVARIANTS` / 用户裁决 `V51`），一个指错地方的住址它照样算命中 ——
    指得对不对要人逐条核原文（`设计/99 §4.10`/`§4.11` 就是那个动作）。
  · **③ 命中的指导力随族的大小衰减**：一个 80 条判据共用一个模块头注的文件，
    那个住址对其中任一条的指导力接近零。本尺子不度量这件事，只把族的大小印出来。
  · TS 侧**不在射程内**（`105` 的 4 668 含 TS）⇒ 本尺子的数与 `105` 的数
    **不可直接相减**，射程不同。

🔴 自检（它自己坏过两次，都在 2026-09-22 当天）
==============================================
一、`grep -rn '#[path = "'` 里的 `[` 会被当成**字符类** ⇒ 映射悄悄变成 0 对，
    而 ③ 会印一个看起来正常的 `0`。⇒ 本尺子用 `-F`，并且**映射为空就拒绝出数**。
二、扫「头注」时漏了 `F47` / `铁律` / `D1` 这几种住址写法 ⇒ 读数虚高 20 个百分点。
    ⇒ 住址形状写在 `PAT` 一处，改它就是改口径，不许在别处再拼一份。
三、〔JA1 2026-09-24 现打〕**判据全集本身数多了 95 条**：`TESTATTR` 原先在整行里找
    `#[test]`，于是注释与字符串里讲形状的那些字样也被数成一条判据
    （`local_backend_host_tests.rs` 报「族内 56」，真判据 33）。⇒ 只认行首（前面只许空白）。
    ⚠ 仍买不到：行首就是那几个字的原始字符串夹具（本仓今天现打没有，将来有了会再多数）。
四、〔同日〕形状巧合：`K-R7-D2` 这类审计标签里的 `D2` 被认成横切纪律 `D2`；
    「本条 11:08:09」里的「条 11」被认成条号。⇒ `D` 号前面不许紧跟 `-` 或字母数字，
    条号后面不许紧跟数字或冒号。⚠ 「铁律 12」这种过程纪律编号、`K-P2 D3` 这种空格隔开的
    审计标签**形状上分不出来**，照样算命中 —— 那一层只能靠人逐族核原文（`第四波记录/JA1.md`）。
"""
import io, os, re, subprocess, sys

# 住址形状的**唯一住址**。改这里就是改口径。
# 〔JA1 2026-09-24〕加 `V\d{2,3}`（用户裁决总表 `设计/99 §1.1` 的 V01–V108）：题面把「用户裁决 V 号」
# 列为合法住址，而此前这里不认它。只收两到三位 —— 仓里 `V1` / `V2` / `V7-2` 这种一位的是别的东西
# （变异编号 · 进程版本 · 审计条目），现打过。这一刀单独一拍落，它自己买的命中数记在 `第四波记录/JA1.md §1`。
PAT = re.compile(r'INVARIANTS|条 ?\d+(?![\d:])|§\d+|\bF\d+[a-z]?\b|铁律|(?<![-\w])D\d+\b|\bV\d{2,3}\b')
TESTATTR = re.compile(r'^\s*#\[(?:tokio::)?test\b')
# 〔JA1〕判据文件头注里写了这个记号 ⇒ 这一族**有人逐字核过原文、主住址缺**（候选升格 / 设计篇缺节）。
# 它不算「点到」：头注里为了说清缺口顺带提到的近邻住址会被 `PAT` 命中，不许借那一下把缺口算成有址。
GAP = "〔缺址〕"
FN = re.compile(r'\s*(?:async\s+)?fn\s+([a-zA-Z0-9_]+)')


def run(*args):
    return subprocess.run(args, capture_output=True, text=True).stdout


def homes():
    """判据文件 → 收留它的生产模块。⚠ 必须 `-F`，理由见模块头注自检一。"""
    out = {}
    for line in run("grep", "-rnF", '#[path = "', "src", "--include=*.rs").splitlines():
        if "tests/" not in line:
            continue
        prod, rest = line.split(":", 1)
        m = re.search(r'#\[path = "([^"]+)"', rest)
        if not m:
            continue
        t = os.path.normpath(os.path.join(os.path.dirname(prod), m.group(1)))
        out.setdefault(t, []).append(prod)
    return out


def header(p):
    try:
        s = io.open(p, encoding="utf-8").read()
    except OSError:
        return ""
    return "\n".join(l for l in s.split("\n") if l.startswith("//!"))


def prose_around(lines, i, j):
    """一条判据自己那几行：`#[test]` 往上的连续注释 ＋ 整个函数体。"""
    own, k = [], i - 1
    while k >= 0 and lines[k].lstrip().startswith(("///", "//", "#[")):
        own.append(lines[k])
        k -= 1
    depth, started, b = 0, False, j
    while b < len(lines):
        depth += lines[b].count("{") - lines[b].count("}")
        if "{" in lines[b]:
            started = True
        own.append(lines[b])
        if started and depth <= 0:
            break
        b += 1
    return "\n".join(own)


def main():
    pairs = homes()
    # 🔴 反空真：映射空了就是尺子坏了，**不许出数**。
    if len(pairs) < 100:
        sys.exit("FAIL 映射只有 %d 对 —— 尺子坏了（见头注自检一），拒绝出数" % len(pairs))

    files = run("find", "tests", "-name", "*.rs").split()
    if not files:
        sys.exit("FAIL 判据文件全集是空的 —— 拒绝出数")

    tally = {"own": 0, "gap": 0, "file": 0, "home": 0, "none": 0}
    none_by_file, gap_by_file, size_by_file = {}, {}, {}
    for t in sorted(files):
        lines = io.open(t, encoding="utf-8").read().split("\n")
        gap = GAP in header(t)
        fh = PAT.search(header(t)) is not None
        hh = any(PAT.search(header(p)) for p in pairs.get(t, []))
        for i, l in enumerate(lines):
            if not TESTATTR.search(l):
                continue
            j = i
            while j < len(lines) and not FN.match(lines[j]):
                j += 1
            if j >= len(lines):
                continue
            size_by_file[t] = size_by_file.get(t, 0) + 1
            if PAT.search(prose_around(lines, i, j)):
                tally["own"] += 1
            elif gap:
                tally["gap"] += 1
                gap_by_file[t] = gap_by_file.get(t, 0) + 1
            elif fh:
                tally["file"] += 1
            elif hh:
                tally["home"] += 1
            else:
                tally["none"] += 1
                none_by_file[t] = none_by_file.get(t, 0) + 1

    tot = sum(tally.values())
    if tot == 0:
        sys.exit("FAIL 一条判据都没数到 —— 抽取器坏了，拒绝出数")

    print("判据函数全集（Rust，TS 不在射程内）：%d" % tot)
    for k, label in (("own", "① 判据自己那几行"),
                     ("file", "② 判据文件的头注"),
                     ("home", "③ 生产侧模块的头注"),
                     ("gap", "④ 核过、主住址缺（〔缺址〕）"),
                     ("none", "── 三层都点不出")):
        print("  %s %5d  %4.1f%%" % (label, tally[k], 100.0 * tally[k] / tot))
    print()
    print("⚠ 与 `105` 那个 82% 的对照：`105` 只算 ①（且它的分母含 TS）")
    print("   本尺子 ① 这一层的「点不出」= %.0f%%" % (100.0 * (tot - tally["own"]) / tot))
    print()
    # 〔JA1〕族数也印出来：`110 §4.12.1` 那个「59 族」当初是手数的，没有住址 ⇒ 第二天就没法对拍。
    # `--all` 印全部族（逐族核住址要全量），默认仍只印前 10。
    everything = "--all" in sys.argv[1:]
    print("三层都点不出的族：%d 族" % len(none_by_file))
    print("三层都点不出，%s（括号里是那一族总条数 —— ③ 的指导力随它衰减）："
          % ("全部" if everything else "最集中的 10 份"))
    ranked = sorted(none_by_file.items(), key=lambda kv: (-kv[1], kv[0]))
    for f, n in (ranked if everything else ranked[:10]):
        print("  %4d 条 / 族内 %-4d  %s" % (n, size_by_file[f], f))
    print()
    print("核过、主住址缺的族（〔缺址〕，交主会话裁升格 / 补节）：%d 族" % len(gap_by_file))
    for f, n in sorted(gap_by_file.items(), key=lambda kv: (-kv[1], kv[0])):
        print("  %4d 条 / 族内 %-4d  %s" % (n, size_by_file[f], f))


if __name__ == "__main__":
    main()
