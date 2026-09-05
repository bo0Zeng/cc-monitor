#!/usr/bin/env python3
"""N-F1c 改动面的量具 —— **逐个顶层条目**比两个提交上的 md5。

住址：<n-f1c 工作树>/evidence/N-F1c-item-md5.py
被测对象：本文件上一级目录那棵工作树（`--worktree` 可覆盖，但缺省不指向别处）。

# 为什么不是「`ast` 逐函数 md5」
brief 第四节要的是「哪几个函数变了，`ast` 逐函数 md5」。那条口径是给 `.py` 写的
（Python 的 `ast` 模块）。本件改的三份文件全是 **Rust**，仓里没有 Rust 的 AST 工具
（也不许为了一份读数去装依赖）⇒ 这里给的是**同职的等价物**，并把它买不到什么写清：

  · 切法：按**顶层**条目切（缩进 0 的 `fn` / `struct` / `enum` / `impl` / `mod` /
    `const` / `use`），用花括号配平找结尾；条目名 = 该行上的标识符。
  · 每个条目连**它头上的注释与属性**一起算 md5 —— 所以「只改了注释」也会显示成变了。
    这是**刻意**的：本件真的改了几处注释里的事实陈述，那是要交代的改动，不是噪音。
    ⚠ 反过来说，它**分不开**「改逻辑」与「改注释」；要那一维得另用去字面量口径。
  · 嵌套在 `impl` / `mod` 里的方法**不单列**，跟着外层条目一起动。
  · 花括号配平**不认字符串与注释里的花括号** ⇒ 遇到那种写法切点会漂；
    本脚本因此在末尾打一条自检（切出来的条目数与文件行数的比值）。

用法：
    python3 evidence/N-F1c-item-md5.py 12c72db HEAD
"""

import argparse
import hashlib
import pathlib
import subprocess
import sys

FILES = [
    "src-tauri/src/local_accounts.rs",
    "src-tauri/src/local_read_surface_registry.rs",
    "remote-daemon-proto/src/observe/accounts_query.rs",
]

STARTERS = ("fn ", "pub fn ", "pub(crate) fn ", "async fn ", "pub async fn ",
            "pub(crate) async fn ", "struct ", "pub struct ", "enum ", "pub enum ",
            "pub(crate) enum ", "impl ", "mod ", "pub mod ", "const ", "pub const ",
            "pub(crate) const ", "use ", "type ")


def head_of(line: str) -> str | None:
    if line[:1].isspace() or not line.strip():
        return None
    t = line.rstrip()
    for s in STARTERS:
        if t.startswith(s):
            rest = t[len(s):].lstrip()
            name = "".join(c for c in rest if c.isalnum() or c in "_:<>[]&' ").strip()
            return f"{s.strip()} {name.split('(')[0].split('=')[0].strip()[:60]}"
    return None


def split_items(src: str) -> list[tuple[str, str]]:
    """→ [(条目名, 条目原文（含头上的注释与属性）)]。"""
    lines = src.split("\n")
    out: list[tuple[str, str]] = []
    lead: list[str] = []          # 攒着的注释 / 属性
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if not line[:1].isspace() and (stripped.startswith("//") or stripped.startswith("#[")
                                       or stripped.startswith("#![")):
            lead.append(line)
            i += 1
            continue
        name = head_of(line)
        if name is None:
            if stripped:
                lead = []
            else:
                lead.append(line)
            i += 1
            continue
        body = lead + [line]
        depth = line.count("{") - line.count("}")
        ended = "{" in line and depth == 0
        if not ended and "{" not in line:
            ended = stripped.endswith(";")
        i += 1
        while not ended and i < len(lines):
            body.append(lines[i])
            depth += lines[i].count("{") - lines[i].count("}")
            if "{" in "".join(body) and depth <= 0:
                ended = True
            i += 1
        out.append((name, "\n".join(body)))
        lead = []
    return out


def digest(src: str) -> dict[str, str]:
    d: dict[str, str] = {}
    for n, body in split_items(src):
        key = n
        k = 2
        while key in d:
            key = f"{n} #{k}"
            k += 1
        d[key] = hashlib.md5(body.encode("utf-8")).hexdigest()[:10]
    return d


def show(wt: pathlib.Path, rev: str, rel: str) -> str:
    return subprocess.run(["git", "-C", str(wt), "show", f"{rev}:{rel}"],
                          check=True, capture_output=True, text=True).stdout


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("base")
    ap.add_argument("tip")
    ap.add_argument("--worktree", default=None)
    a = ap.parse_args()
    wt = pathlib.Path(a.worktree).resolve() if a.worktree else pathlib.Path(__file__).resolve().parent.parent
    print(f"树 {wt}\n基点 {a.base} → 尖 {a.tip}\n")
    total_changed = 0
    for rel in FILES:
        old, new = digest(show(wt, a.base, rel)), digest(show(wt, a.tip, rel))
        names = sorted(set(old) | set(new))
        rows = []
        for n in names:
            o, w = old.get(n), new.get(n)
            if o == w:
                continue
            rows.append((n, o or "（新增）", w or "（删除）"))
        total_changed += len(rows)
        print(f"── {rel}   顶层条目 {len(old)} → {len(new)}，其中变了 {len(rows)} 个")
        for n, o, w in rows:
            print(f"     {n:<46s} {o} → {w}")
        # 自检：切法塌了的话上面会假绿 / 假红。
        lines = len(show(wt, a.tip, rel).split("\n"))
        print(f"     〔自检〕{lines} 行切出 {len(new)} 个顶层条目"
              f"（切法坏掉时这个数会掉到个位数或炸上天）\n")
    print(f"合计变了 {total_changed} 个顶层条目。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
