#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R20 尺：**零定义的名字被当现状说**。

判据（零词表、零语义）：一个 `全小写 snake_case + >=N 个下划线` 的名字，
在**注释**里以 `` `名字` ``（反引号包住）出现过，而在**非注释文本**里全语料一次都没出现过。

⚠ 本尺**逐字复刻** `guard_core::strip_comment_lines` 的剥法（整行注释 + 字符串安全的
行尾注释），因为闸自己就是拿它切的 —— 尺子与判据不同源，报出来的数就不是闸看到的数。

⚠ 量具事实（`K-R19` 交回时点名的两条，本尺照收）：
  · 不用本机 `grep`（它是包 ugrep 的 shell 函数、默认读 `.gitignore`）；
  · 不走 `git ls-files`（git 默认八进制转义非 ASCII 路径，本仓大量中文名会被静默漏掉）。
  ⇒ 语料面一律 `os.walk`，与判据自己的 `scan_tree!` 同源。

用法：
    python3 evidence/K-R20-C-deadname-census.py <worktree> [--min-underscores N] [--face A|G]
"""
import os
import re
import sys

EXCLUDE_DIRS = {"node_modules", ".git", "target", "dist", "vendor", "coverage", ".vite"}

# 闸真正扫的那一面（Rust 侧逐根对应）。
# ⚠ `src-tauri/build.rs` **非收不可**（`addr_corpus()` 也是单独把它捞进来的）：
#   `emit_daemon_capabilities` 真的定义在那儿，漏掉它就是一处假阳。
# ⚠ `evidence/` **刻意不收**：那是量具与记录，它的散文里逐字写着一堆死名
#   （`local_tmux_names` / `launch_identity_prefix` 都在），收进来会把代码侧喂饱
#   ⇒ 旗舰活体当场消失。这一族本仓的说法是「判据被自己的散文喂饱」。
ROOTS_G = ["src-tauri/src", "src-tauri/crates", "remote-daemon-proto/src", "src", "doc", "e2e"]
FILES_G = ["src-tauri/build.rs"]

SLASH_EXT = {"rs", "ts", "tsx", "js", "mjs", "cjs", "mts"}   # 走 guard_core 那份剥法
PROSE_EXT = {"md"}                                     # 整份都是散文 ⇒ 全算注释、不供代码侧
# 其余一律**整份算代码**（最保守：代码侧越宽，假红越少）。

# 🔴 判据自己那份**整份摘掉**（`scan_tree!` 按构造做的那一刀）。
# 非摘不可的理由：登记表里每个死名都是一个**字符串字面量** = 代码
# ⇒ 不摘的话，凡是登记过的名字全都「在代码里出现过」⇒ 人群当场塌成空集。
SELF = "src-tauri/src/structural_scan.rs"

NAME_RE_TMPL = r"(?<![A-Za-z0-9_])[a-z][a-z0-9]*(?:_[a-z0-9]+){%d,}(?![A-Za-z0-9_])"
# 注释侧只认**反引号跨度里**的名字：`` `…` `` 之内才是「在点一个代码符号」。
# 不加这一条，人群会被英文散文里的普通词组撑大一个量级（现打 >=2：71 处 → 370 处）。
# ⚠ 跨度**整个就是一个裸符号引用**（可带 `路径::` 前缀、`()` / `!` 后缀）才算数。
#   ① 不要求「跨度逐字等于名字」——否则 `` `local_tmux_names()` ``（带括号）与
#      `` `a.rs::foo_bar` ``（带路径）都会漏掉，而那正是订正段最常见的写法；
#   ② 也不放宽成「跨度里出现过」——那会把整句散文的跨度收进来（现打差 27 处）。
SPAN_RE = re.compile(r"`([^`\n]+)`")
BARE_TMPL = (r"^(?:[A-Za-z0-9_./-]+::)?"
             r"([a-z][a-z0-9]*(?:_[a-z0-9]+){%d,})"
             r"(?:\(\)|!)?$")

TOMBSTONE = "〔散文墓碑〕"


def is_text(path):
    try:
        with open(path, "rb") as f:
            return b"\0" not in f.read(4096)
    except OSError:
        return False


def walk(root, subs=None):
    out = []
    for base in ([os.path.join(root, s) for s in subs] if subs else [root]):
        if not os.path.isdir(base):
            continue
        for d, dirs, files in os.walk(base):
            dirs[:] = [x for x in dirs if x not in EXCLUDE_DIRS]
            for fn in files:
                p = os.path.join(d, fn)
                if not os.path.islink(p) and is_text(p):
                    out.append(p)
    return sorted(set(out))


def mask_char_literals(line):
    """`guard_core::mask_char_literals` 的等价物：把 `'c'` / `'\\n'` 整个替成 `_`。"""
    out = list(line)
    i = 0
    n = len(line)
    while i < n:
        if line[i] == "'":
            if i + 3 < n and line[i + 1] == "\\":
                k = line.find("'", i + 3)
                if k != -1:
                    for j in range(i, k + 1):
                        out[j] = "_"
                    i = k + 1
                    continue
            if i + 1 < n and line[i + 1] not in ("'", "\\") and i + 2 < n and line[i + 2] == "'":
                out[i] = out[i + 1] = out[i + 2] = "_"
                i += 3
                continue
        i += 1
    return "".join(out)


def strip_trailing_comments(lines):
    """`guard_core::strip_trailing_comments` 的等价物（含它那三条保守边界）。"""
    out = []
    in_str = False
    for raw in lines:
        masked = mask_char_literals(raw)
        if 'r"' in masked or "r#" in masked or 'b"' in masked:
            out.append(raw)
            in_str = False
            continue
        if in_str:
            i = 0
            while i < len(masked):
                if masked[i] == "\\":
                    i += 2
                    continue
                if masked[i] == '"':
                    in_str = False
                    break
                i += 1
            out.append(raw)
            continue
        i = 0
        cut = None
        while i < len(masked):
            if in_str:
                if masked[i] == "\\":
                    i += 2
                    continue
                if masked[i] == '"':
                    in_str = False
                i += 1
                continue
            if masked[i] == '"':
                in_str = True
                i += 1
                continue
            if masked[i] == "/" and i + 1 < len(masked) and masked[i + 1] == "/":
                cut = i
                break
            i += 1
        out.append(raw[:cut] if cut is not None else raw)
    return out


def split_file(path, text):
    """返回 [(行号1基, 代码段, 注释段)]。注释段 = 原行减去代码段的那一头。"""
    ext = path.rsplit(".", 1)[-1].lower() if "." in os.path.basename(path) else ""
    lines = text.split("\n")
    if ext in PROSE_EXT:
        return [(i, "", l) for i, l in enumerate(lines, 1)]
    if ext not in SLASH_EXT:
        return [(i, l, "") for i, l in enumerate(lines, 1)]
    blanked = ["" if l.lstrip().startswith(("//", "*", "/*")) else l for l in lines]
    code = strip_trailing_comments(blanked)
    out = []
    for i, (raw, c) in enumerate(zip(lines, code), 1):
        out.append((i, c, raw[len(c):] if raw.startswith(c) else raw))
    return out


def census(wt, minu, face="G"):
    files = walk(wt, ROOTS_G if face == "G" else None)
    if face == "G":
        files += [os.path.join(wt, f) for f in FILES_G if os.path.isfile(os.path.join(wt, f))]
    nre = re.compile(NAME_RE_TMPL % minu)
    bre = re.compile(BARE_TMPL % minu)
    in_code = set()
    in_comment = {}
    for p in files:
        try:
            text = open(p, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        rel = os.path.relpath(p, wt)
        if rel.replace(os.sep, "/") == SELF:
            continue
        for ln, c, k in split_file(p, text):
            for m in nre.finditer(c):
                in_code.add(m.group(0))
            for span in SPAN_RE.finditer(k):
                m = bre.match(span.group(1).strip())
                if m:
                    in_comment.setdefault(m.group(1), []).append(
                        (rel, ln, k.strip(), TOMBSTONE in k))
    dead = {n: v for n, v in in_comment.items() if n not in in_code}
    return files, in_code, in_comment, dead


def main():
    wt = os.path.abspath(sys.argv[1])
    args = sys.argv[2:]
    minu = 2
    face = "G"
    for k, a in enumerate(args):
        if a == "--min-underscores":
            minu = int(args[k + 1])
        if a == "--face":
            face = args[k + 1]
    files, in_code, in_comment, dead = census(wt, minu, face)
    live = {n: v for n, v in dead.items() if not all(t for *_, t in v)}
    occ = sum(len(v) for v in dead.values())
    tomb = sum(1 for v in dead.values() for *_, t in v if t)
    print(f"# 面 {face} · >={minu} 下划线 · 量于工作树 {wt}")
    print(f"语料 {len(files)} 份 · 代码侧名字 {len(in_code)} 个 · 注释侧名字 {len(in_comment)} 个")
    print(f"⇒ 人群（只活在注释里）：**{len(dead)} 个名字 / {occ} 处提及**"
          f"（其中带墓碑 {tomb} 处 ⇒ 未声明的 {len(live)} 个名字 / {occ - tomb} 处）")
    print()
    for n in sorted(dead):
        print(f"## {n}  （{len(dead[n])} 处）")
        for rel, ln, line, t in dead[n]:
            print(f"   {'墓' if t else ' '} {rel}:{ln}  {line[:150]}")


if __name__ == "__main__":
    main()
