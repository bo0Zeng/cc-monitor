#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R6 摸底量具：把 D7 §5.2 那四张网做成可跑的探针，量它们的【人群大小】与【噪声】。

住址（唯一）: backend-consolidation/evidence/kr6-net-probe.py
被测对象     : 由 --tree 显式给出（默认 .claude/worktrees/k-r6）+ --plan 计划仓目录
本工具只读，不写任何被测文件。

它判什么 / 不判什么：
  判：某条网的【触发人群有多大】、【其中多少条带了分母/锚点标记】。
  不判：命中的那一条到底是不是真阳 —— 真阳率必须人工抽样判，本工具只把样本打印出来。

口径分岔（本件的核心读数）：
  A-D7  = D7 §5.2 原口径：头注里含【全称/否定全称词】的句子
  A-件  = 件文件 §1 改写后的口径：含【N 处 / N 支 / N 个】这类数量断言的句子
  两者是两个不同的人群，本工具各量一遍。
"""
import argparse
import os
import re
import sys

# ---- D7 §5.2 网 A 原口径的词表（逐字抄自 audits/K-G2-D7.md 的网 A 行）----
UNIV_WORDS = ["任何", "全部", "所有", "只有", "唯一", "都", "一律", "穷举", "没有一个", "再没有"]

# ---- 件文件 §1 改写后网 A 的口径：数量断言 ----
NUMCLAIM_RE = re.compile(r"[0-9０-９]+\s*(处|支|个|条|格|次|份|行|组|轮|种|句|张)")

# ---- 「带了分母/量法」的宽松标记：宁可多认，让命中数偏小（诚实边界见 §报告）----
DENOM_MARKS = ["分母", "我量过的", "量于", "现打", "口径", "grep -c", "wc -l", "git ls-files",
               "这几形", "样本", "= ", "共 ", "总"]

# ---- 网 B：指路 ----
PTR_RE = re.compile(r"§\s*[0-9]+(?:\.[0-9]+)*|audits/[\w\-.]+\.md#[\w\-]+|`[\w:./\-]+\.(?:rs|md|sh|py|ts|toml|json)`")

# ---- 网 C：数判据自己的东西的数（中文数词 + 阿拉伯数字 + 量词）----
SELFNUM_RE = re.compile(r"[0-9０-９一二三四五六七八九十]+\s*(组|格|支|条|维|个|处)")

# ---- 网 D：声称「会叫你回来」----
ALARM_PATS = ["会红", "会把你叫回来", "叫你回来", "会有人叫", "会当场", "会拦住", "会打红",
              "当场把", "必定逮", "会报", "就会失败", "会 FAIL"]
# 锚点标记：句里点了报文/assert
ANCHOR_MARKS = ["assert", "panicked", "报文", "逐字", "「", "FAILED", "探针"]

SENT_SPLIT = re.compile(r"[。！？；\n]|(?<=\))\s*⇒")


def doc_lines(path):
    """返回 .rs 头注（/// 与 //! 行）的 (行号, 文本)。"""
    out = []
    with open(path, encoding="utf-8") as fh:
        for i, ln in enumerate(fh, 1):
            s = ln.strip()
            if s.startswith("///") or s.startswith("//!"):
                out.append((i, s.lstrip("/").strip()))
    return out


def md_lines(path):
    out = []
    with open(path, encoding="utf-8") as fh:
        for i, ln in enumerate(fh, 1):
            out.append((i, ln.rstrip("\n")))
    return out


def sentences(pairs):
    """把 (行号,文本) 摊成 (行号, 句子)。跨行不合并 —— 口径写死：一行内切句。"""
    for lineno, text in pairs:
        for s in SENT_SPLIT.split(text):
            if s and s.strip():
                yield lineno, s.strip()


def has(s, marks):
    return any(m in s for m in marks)


def net_a(pairs, words, label):
    trig, ok, hits = 0, 0, []
    for lineno, s in sentences(pairs):
        if has(s, words) if isinstance(words, list) else words.search(s):
            trig += 1
            if has(s, DENOM_MARKS):
                ok += 1
            else:
                hits.append((lineno, s))
    return {"label": label, "trigger": trig, "带标记": ok, "命中": len(hits), "样本": hits}


def net_b(pairs):
    refs = []
    for lineno, text in pairs:
        for m in PTR_RE.finditer(text):
            refs.append((lineno, m.group(0)))
    return refs


def net_c(pairs):
    hits = []
    for lineno, s in sentences(pairs):
        for m in SELFNUM_RE.finditer(s):
            hits.append((lineno, m.group(0), s[:90]))
    return hits


def net_d(pairs):
    trig, anchored, hits = 0, 0, []
    for lineno, s in sentences(pairs):
        if has(s, ALARM_PATS):
            trig += 1
            if has(s, ANCHOR_MARKS):
                anchored += 1
            else:
                hits.append((lineno, s))
    return {"trigger": trig, "带锚点": anchored, "命中": len(hits), "样本": hits}


def report_a(r, n_sample):
    print(f"  [网A/{r['label']}] 触发人群 {r['trigger']} 句 · 其中带分母标记 {r['带标记']} 句 "
          f"⇒ 命中(要人看) {r['命中']} 句")
    for lineno, s in r["样本"][:n_sample]:
        print(f"      :{lineno}  {s[:110]}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default="/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r6")
    ap.add_argument("--plan", default="/home/zbl/文档/claudecode-frontend/.claude/planned-build/backend-consolidation")
    ap.add_argument("--sample", type=int, default=12)
    a = ap.parse_args()

    print("=" * 78)
    print("K-R6 四张网探针 · 量具 evidence/kr6-net-probe.py")
    print(f"被测树   : {a.tree}")
    print(f"计划仓   : {a.plan}")
    print("=" * 78)

    rs = os.path.join(a.tree, "src-tauri/src/capability_registry.rs")
    if not os.path.exists(rs):
        print(f"!! 找不到 {rs}", file=sys.stderr)
        return 2
    hz = doc_lines(rs)
    total = sum(1 for _ in open(rs, encoding="utf-8"))
    print(f"\n### 人群一 · D7 声明的人群 = 头注（{rs} 的 /// 行）")
    print(f"    文件 {total} 行 · 头注 {len(hz)} 行 · 头注里的句子 {sum(1 for _ in sentences(hz))} 句")

    print("\n-- 网 A 两个口径各量一遍 --")
    report_a(net_a(hz, UNIV_WORDS, "D7原口径·全称词"), a.sample)
    report_a(net_a(hz, NUMCLAIM_RE, "件文件口径·N处/N支/N个"), a.sample)

    refs = net_b(hz)
    print(f"\n-- 网 B --\n  [网B] 头注里的指路引用总数 {len(refs)}")
    kinds = {}
    for _, r in refs:
        k = "§节号" if r.startswith("§") else ("audits#id" if "audits/" in r else "文件路径")
        kinds[k] = kinds.get(k, 0) + 1
    print(f"      按形状: {kinds}")
    for lineno, r in refs[:a.sample]:
        print(f"      :{lineno}  {r}")

    c = net_c(hz)
    print(f"\n-- 网 C --\n  [网C] 头注里「数词+量词」出现 {len(c)} 处（= 要逐处判『是不是数判据自己的东西』的人群）")
    for lineno, tok, ctx in c[:a.sample]:
        print(f"      :{lineno}  {tok}   | {ctx}")

    d = net_d(hz)
    print(f"\n-- 网 D --\n  [网D] 触发人群 {d['trigger']} 句 · 带锚点标记 {d['带锚点']} 句 ⇒ 命中 {d['命中']} 句")
    for lineno, s in d["样本"][:a.sample]:
        print(f"      :{lineno}  {s[:110]}")

    # ---- 人群二：件文件 §1 把人群从「头注」放宽到「报文/文档」之后 ----
    print("\n" + "=" * 78)
    print("### 人群二 · 件文件 §1 改口径后的人群 = 计划仓 md（features/ + audits/）")
    feats = os.path.join(a.plan, "features")
    auds = os.path.join(a.plan, "audits")
    files = []
    for d0 in (feats, auds):
        if os.path.isdir(d0):
            files += [os.path.join(d0, x) for x in sorted(os.listdir(d0)) if x.endswith(".md")]
    # ⚠ 分母只许数 .md —— 上一版这里用 len(os.listdir(...)) 数的是【全部条目】，
    #   今天两个目录恰好只有 .md ⇒ 两个口径同值。那是巧合不是正确（K-R6 §4.10 自逮）。
    n_f = len([x for x in os.listdir(feats) if x.endswith(".md")]) if os.path.isdir(feats) else 0
    n_a = len([x for x in os.listdir(auds) if x.endswith(".md")]) if os.path.isdir(auds) else 0
    print(f"    分母 = {len(files)} 份 md（features {n_f} · audits {n_a}；口径：只数 .md）")

    agg = {"A-D7": 0, "A-件": 0, "B": 0, "C": 0, "D": 0}
    lines_total = 0
    for f in files:
        p = md_lines(f)
        lines_total += len(p)
        agg["A-D7"] += net_a(p, UNIV_WORDS, "")["命中"]
        agg["A-件"] += net_a(p, NUMCLAIM_RE, "")["命中"]
        agg["B"] += len(net_b(p))
        agg["C"] += len(net_c(p))
        agg["D"] += net_d(p)["命中"]
    print(f"    总行数 {lines_total}")
    print(f"    [网A/D7原口径] 命中 {agg['A-D7']} 句")
    print(f"    [网A/件文件口径] 命中 {agg['A-件']} 句")
    print(f"    [网B] 指路引用总数 {agg['B']} 处（每一处都要真去解析）")
    print(f"    [网C] 数词+量词 {agg['C']} 处")
    print(f"    [网D] 命中 {agg['D']} 句")
    return 0


if __name__ == "__main__":
    sys.exit(main())
