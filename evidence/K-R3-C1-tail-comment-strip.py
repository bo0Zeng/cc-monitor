#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R3-C1 · 行尾注释那个洞：**先量射程，再动 `production_code`**。

住址（唯一）：<工作树>/evidence/K-R3-C1-tail-comment-strip.py
被测对象：本文件所在工作树（`WT` 由 `__file__` 推出，不接受外部路径 ⇒ 复跑不会指到另一棵树）
量于：输出头部打印 `git rev-parse HEAD`。

它做三件事：
  ① 复刻**今天**的 `guard_core::production_code`（只剔整行 `//`）；
  ② 复刻**拟改后**的剥法（整行 `//` + **行尾 `//`**，字符串字面量安全、保守放行）；
  ③ 把两者跑遍整棵树的 `.rs`，逐行报差 —— 这就是这一刀的**射程**与 `D4③` 的非空对照面。

保守放行的三条（宁可留洞，不许造假红 —— 铁律 18）：
  · 行内在字符串里的 `//`（`"http://x"`）不剪；
  · 行里有 raw string 开头（`r"` / `r#"`）⇒ 整行不动；
  · 跨行字符串**里面**的行 ⇒ 整行不动（按未转义双引号的奇偶跨行带状态）。
字符字面量（`'"'` / `'\\''`）先按等长掩码抹掉再判，免得一个 `'"'` 把后面全带进「字符串里」。
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()
WT = HERE.parent.parent

sys.path.insert(0, str(HERE.parent))
from importlib import import_module  # noqa: E402

_sim = import_module("K-R3-C1-verdict-sim")
production_source = _sim.production_source

CHAR_LIT = re.compile(r"'(?:\\.|[^'\\\n])'")


def _mask_char_literals(line: str) -> str:
    """把 `'x'` / `'\\n'` 换成等长占位 ⇒ 下标不变，而里面的引号不再干扰状态机。"""
    return CHAR_LIT.sub(lambda m: "_" * len(m.group(0)), line)


def strip_tail_comments(src: str) -> str:
    """拟改后的剥法。返回逐行处理后的文本（行数不变）。"""
    out = []
    in_str = False  # 跨行沿用：一行扫完还在字符串里 ⇒ 下一行整行不动
    for raw in src.split("\n"):
        masked = _mask_char_literals(raw)
        if "r\"" in masked or "r#" in masked or "b\"" in masked:
            # raw / byte string：这份保守剥法不碰它，且状态不可信 ⇒ 归零重来
            out.append(raw)
            in_str = False
            continue
        if in_str:
            # 上一行没收口 ⇒ 本行在字符串里；扫完它，看这一行收不收口
            i, n = 0, len(masked)
            while i < n:
                c = masked[i]
                if c == "\\":
                    i += 2
                    continue
                if c == '"':
                    in_str = False
                    break
                i += 1
            out.append(raw)
            continue
        i, n, cut = 0, len(masked), None
        while i < n:
            c = masked[i]
            if in_str:
                if c == "\\":
                    i += 2
                    continue
                if c == '"':
                    in_str = False
                i += 1
                continue
            if c == '"':
                in_str = True
                i += 1
                continue
            if c == "/" and i + 1 < n and masked[i + 1] == "/":
                cut = i
                break
            i += 1
        out.append(raw if cut is None else raw[:cut])
    return "\n".join(out)


def production_code_old(src: str) -> str:
    return "\n".join(l for l in production_source(src).split("\n")
                     if not l.lstrip().startswith("//"))


def production_code_new(src: str) -> str:
    kept = "\n".join(l for l in production_source(src).split("\n")
                     if not l.lstrip().startswith("//"))
    return strip_tail_comments(kept)


def rs_files():
    for root in ("src-tauri/src", "src-tauri/crates", "remote-daemon-proto/src"):
        d = WT / root
        if not d.is_dir():
            continue
        for p in sorted(d.rglob("*.rs")):
            yield p


def main() -> int:
    head = subprocess.run(["git", "-C", str(WT), "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    print(f"# 工作树 = {WT}")
    print(f"# HEAD = {head}")
    verbose = "-v" in sys.argv
    files = 0
    changed_files = 0
    changed_lines = 0
    samples = []
    for p in rs_files():
        src = p.read_text(encoding="utf-8", errors="replace")
        files += 1
        a, b = production_code_old(src), production_code_new(src)
        if a == b:
            continue
        changed_files += 1
        al, bl = a.split("\n"), b.split("\n")
        assert len(al) == len(bl), f"行数变了：{p}"
        for i, (x, y) in enumerate(zip(al, bl)):
            if x != y:
                changed_lines += 1
                if len(samples) < 400:
                    samples.append((str(p.relative_to(WT)), i + 1, x.strip(), y.strip()))
    print(f"# 扫到 .rs 文件 = {files}（分母：src-tauri/src + src-tauri/crates + remote-daemon-proto/src 下全部 *.rs，含测试文件；剥的是它们的生产段）")
    print(f"# 生产段被剪掉行尾注释的文件 = {changed_files}")
    print(f"# 被剪的行 = {changed_lines}")
    if verbose:
        for f, ln, x, y in samples:
            print(f"  {f}:{ln}\n    - {x}\n    + {y}")
    print(f"# md5(本量具) = {hashlib.md5(HERE.read_bytes()).hexdigest()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
