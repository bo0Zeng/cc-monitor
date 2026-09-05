#!/usr/bin/env python3
"""K-P3 量具：把 `structural_scan.rs` 的「散文里点名了一个代码里不存在的名字」那条判据
在宿主上**预打一遍**，只针对本件改过的三份文件。

为什么要它：那条判据住在 `cargo test --lib` 里，而本件在宿主上一条测试都不跑
（本波纪律）。它的判红条件是纯句法的，可以在宿主上用同一口径复现 ⇒
先在这儿把「新写的散文里有没有死名」筛一遍，省一趟沙箱门禁。

口径**逐条抄自** `src-tauri/src/structural_scan.rs`（量于工作树 k-p3）：
  · 形状：全小写 snake_case，`_` 分段数 >= min_us + 1，min_us = DEAD_NAME_MIN_UNDERSCORES = 2
  · 语料：src-tauri/src · src-tauri/crates · remote-daemon-proto/src · src · doc · e2e
          + src-tauri/build.rs，**全部文件**（无扩展名过滤）
  · 代码侧 / 散文侧的切法：`.rs`/`.ts`/… 按行剥注释，剥掉的那半算散文
  · 散文侧只认**反引号跨度**里的裸符号；`a::b` 取 `b`，去掉结尾的 `()` / `!`

⚠ 它**不是**那条判据的第二个真相源，也不比它权威：
  · 本量具的剥注释是一个近似（按 `//` 与 `/* */` 切，不解析字符串字面量）；
  · 登记表（INVENTORY / TOMBSTONED）本量具**不读** ⇒ 已登记的存量会被它报出来。
  ⇒ 读法是「新增文件里出现的名字，逐个自己核」，不是「它绿就等于门禁绿」。

用法（在工作树根跑）：
    python3 evidence/K-P3-dead-name-precheck.py
"""

import os
import re
import sys

MIN_US = 2
ROOTS = [
    "src-tauri/src",
    "src-tauri/crates",
    "remote-daemon-proto/src",
    "src",
    "doc",
    "e2e",
]
EXTRA = ["src-tauri/build.rs"]
# 本件改过的三份 + 预批的那一份（没建就跳过）
MINE = [
    "src-tauri/src/daemon_policy.rs",
    "src/daemon-policy.ts",
    "remote-daemon-proto/src/platform/pidwatch/mod.rs",
]

CODEY = {".rs", ".ts", ".tsx", ".mts", ".mjs", ".js", ".cjs"}


def is_dead_name_shape(w):
    segs = w.split("_")
    if len(segs) < MIN_US + 1:
        return False
    if not (segs[0][:1].islower() and segs[0][:1].isalpha()):
        return False
    return all(s and all(c.islower() or c.isdigit() for c in s) and s.isascii() for s in segs)


IDENT = re.compile(r"[A-Za-z0-9_]+")


def split_code_prose(path, text):
    """(代码侧文本, 散文侧逐行) —— 近似 `dead_name_split`。"""
    ext = os.path.splitext(path)[1]
    if ext == ".md":
        return "", text.splitlines()
    if ext not in CODEY:
        return text, []
    code, prose = [], []
    in_block = False
    for raw in text.splitlines():
        line, cmt = raw, ""
        if in_block:
            end = line.find("*/")
            if end < 0:
                prose.append(line)
                continue
            cmt, line, in_block = line[:end + 2], line[end + 2:], False
        # 行注释 / 块注释开头（不解析字符串字面量 —— 这就是上面说的那个近似）
        i_line = line.find("//")
        i_blk = line.find("/*")
        cut = min([x for x in (i_line, i_blk) if x >= 0], default=-1)
        if cut >= 0:
            cmt += line[cut:]
            line = line[:cut]
            if cut == i_blk and "*/" not in raw[cut:]:
                in_block = True
        code.append(line)
        if cmt:
            prose.append(cmt)
    return "\n".join(code), prose


def backtick_spans(line):
    parts = line.split("`")
    return [parts[i] for i in range(1, len(parts), 2)]


def bare_symbol(span):
    s = span.strip()
    for suf in ("()", "!"):
        if s.endswith(suf):
            s = s[: -len(suf)]
            break
    k = s.rfind("::")
    if k >= 0:
        pre, post = s[:k], s[k + 2:]
        if not pre or not all(c.isalnum() or c in "_./-" for c in pre):
            return None
        s = post
    return s if is_dead_name_shape(s) else None


def main():
    files = []
    for root in ROOTS:
        for dirpath, _, names in os.walk(root):
            for n in names:
                files.append(os.path.join(dirpath, n))
    files += [p for p in EXTRA if os.path.exists(p)]
    if len(files) < 550:
        print(f"❌ 语料面只收到 {len(files)} 份 —— 尺子作用域对不上（判据自检要 >= 550）")
        return 3

    in_code = set()
    prose_by_file = {}
    for p in files:
        try:
            text = open(p, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        code, prose = split_code_prose(p, text)
        for w in IDENT.findall(code):
            if is_dead_name_shape(w):
                in_code.add(w)
        hits = {}
        for line in prose:
            for span in backtick_spans(line):
                name = bare_symbol(span)
                if name:
                    hits[name] = hits.get(name, 0) + 1
        if hits:
            prose_by_file[p] = hits

    print(f"· 语料 {len(files)} 份 · 代码侧 snake_case 名字 {len(in_code)} 个")
    dead_all = {
        (p, n): c for p, h in prose_by_file.items() for n, c in h.items() if n not in in_code
    }
    print(f"· 全语料「只活在散文里」的名字：{len(dead_all)} 处（判据自检要 >= 60）")

    bad = {k: v for k, v in dead_all.items() if k[0] in MINE}
    if bad:
        print("\n❌ 本件改过的文件里有死名（登记表本量具不读 ⇒ 逐个自己核）：")
        for (p, n), c in sorted(bad.items()):
            print(f"    {p}  `{n}`  {c} 处")
        return 1
    print("\n✅ 本件三份文件里的散文没有点名任何代码里不存在的 snake_case 名字")
    return 0


if __name__ == "__main__":
    sys.exit(main())
