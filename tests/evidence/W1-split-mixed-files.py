#!/usr/bin/env python3
"""设计/99 §4 步 7b —— 仓库重组第 3 批：src/bridge/ 混合文件剖分。

把 `src/bridge/` 里 `#[cfg(test)] mod X { … }` 这种**内联测试模块块**整块搬到
`<repo>/tests/bridge/…` 下，原地只留 `设计/16 §3.1` 那三行：

    #[cfg(test)]
    #[path = "../../../tests/bridge/<…>.rs"]
    mod X;

🔴 `设计/16 §6.1` 那条纪律在这里是**硬的**：对每个文件，剖分后 src 段 + test 段
**拼回去必须与原文件逐字节相同**，不满足就跳过该文件并记账。理由（§4.1）：
括号配平在字符串字面量上会偏，而这次偏的后果不是假阳性，是**丢代码**。

⇒ 本脚本的括号配平走一个**真词法器**（跳过 `//` `/* */`（可嵌套）、`"…"`、`r#"…"#`、
`'c'`、`b"…"`、生命周期 `'a`），而不是数字符；配平之外再加一道**逐字节回拼对账**：
    前缀 + 属性行 + "mod X {\n" + 缩进还原(测试体) + "}\n" + 后缀  ==  原文件
不相等 ⇒ 跳过。

用法：
    python3 tests/evidence/W1-split-mixed-files.py --survey        # 只看，不动
    python3 tests/evidence/W1-split-mixed-files.py --apply         # 真移动
    python3 tests/evidence/W1-split-mixed-files.py --apply --only <文件…>
"""
import argparse
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


# ───────────────────────── 词法器 ─────────────────────────
def _lex(src: str, start: int, stop_at_depth0_close: bool, in_lit_lines=None, line_base: int = 0):
    """从 src[start] 开始走词法。

    跳过注释与各种字面量 —— 这一条就是 §6.1 点名的那个坑（`strip_cfg_test`
    的括号配平不识别字符串里的大括号，已知造成过 4 次事故）。

    stop_at_depth0_close=True  ⇒ src[start] 必须是 '{'，返回配对 '}' 的下标（找不到返回 -1）。
    in_lit_lines is not None   ⇒ 顺带把「行首落在字符串/字符字面量内部」的行号记进去。
    """
    i, n, depth = start, len(src), 0

    def mark(a: int, b: int):
        """src[a:b] 是一段字面量；它跨过的每个换行，其**下一行**行首在字面量里。"""
        if in_lit_lines is None:
            return
        j = src.find("\n", a)
        while 0 <= j < b - 1:
            in_lit_lines.add(src.count("\n", 0, j + 1) + line_base)
            j = src.find("\n", j + 1)

    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":          # 行注释
            j = src.find("\n", i)
            i = n if j < 0 else j + 1
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":          # 块注释（可嵌套）
            i += 2
            d = 1
            while i < n and d:
                if src.startswith("/*", i):
                    d += 1; i += 2
                elif src.startswith("*/", i):
                    d -= 1; i += 2
                else:
                    i += 1
            continue
        m = re.match(r'(b?r)(#*)"', src[i:])                       # 原始字符串 r"…" / br#"…"#
        if m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            close = '"' + m.group(2)
            j = src.find(close, i + len(m.group(0)))
            if j < 0:
                return -1
            mark(i, j + len(close))
            i = j + len(close)
            continue
        if c == '"' or (c == "b" and i + 1 < n and src[i + 1] == '"'):   # 普通字符串 "…" / b"…"
            a = i
            i += 1 if c == '"' else 2
            while i < n:
                if src[i] == "\\":
                    i += 2; continue
                if src[i] == '"':
                    i += 1; break
                i += 1
            mark(a, i)
            continue
        if c == "'":                                               # 'c' 是字面量，'a 是生命周期
            m = re.match(r"'(\\.[^']*|[^'\\])'", src[i:])
            i += len(m.group(0)) if m else 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0 and stop_at_depth0_close:
                return i
        i += 1
    return -1


def scan_block(src: str, open_idx: int) -> int:
    assert src[open_idx] == "{", src[open_idx : open_idx + 20]
    return _lex(src, open_idx, True)


def literal_line_starts(text: str):
    """行首落在**多行字面量内部**的那些行（0 基）。这些行的缩进属于字面量的值，一个字节都不许动。"""
    s = set()
    _lex(text, 0, False, s)
    return s


# ───────────────────────── 识别顶层 `#[cfg(test)] mod X { … }` ─────────────────────────
ATTR = re.compile(r"^#\[[^\n]*\]$")
CFG_TEST = re.compile(r"^#\[cfg\(test\)\]$")
MOD_OPEN = re.compile(r"^(pub(\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{\s*$")


def find_blocks(text: str):
    """返回 [(attr_start_line, mod_line, name, end_line)]，全部是**列 0**（顶层）的块。"""
    lines = text.split("\n")
    out = []
    i = 0
    while i < len(lines):
        if CFG_TEST.match(lines[i]):
            start = i
            j = i + 1
            while j < len(lines) and ATTR.match(lines[j]):
                j += 1
            m = MOD_OPEN.match(lines[j]) if j < len(lines) else None
            if m:
                # 定位这一行的 '{' 在整篇里的字节下标
                off = sum(len(x) + 1 for x in lines[:j]) + lines[j].rindex("{")
                close = scan_block(text, off)
                if close >= 0:
                    end_line = text.count("\n", 0, close)
                    # 配对的 } 必须自己独占一行、且在列 0
                    if lines[end_line].rstrip() == "}" and lines[end_line].startswith("}"):
                        out.append((start, j, m.group(3), end_line))
                        i = end_line + 1
                        continue
        i += 1
    return out


# ───────────────── `include_str!` 一族：路径跟着**声明它的文件**走 ─────────────────
# `设计/16 §5.4a` 那三条人群规则：`include_str!` 与 `include_bytes!` 是同一个人群
# （按语义划，不按宏名）；路径也住在 `#[path]` 里。测试体一搬家，这些**相对**字面量
# 全部指空 —— 而它们不是静默失效，是编不过（`§5.4a` 表里第 2 行同一形）。
PATHLIT = re.compile(r'((?:include_str|include_bytes)!\s*\(\s*|#\[path = )"([^"\\\n]*)"')


def retarget(text: str, from_rel: str, to_rel: str):
    """把 text 里所有**相对**的 include 路径，从「相对 from_rel」改写成「相对 to_rel」。

    绝对路径与带转义的字面量不碰。返回 (新文本, 改写处数)。
    """
    a = os.path.dirname(os.path.join(REPO, from_rel))
    b = os.path.dirname(os.path.join(REPO, to_rel))
    n = 0

    def sub(m):
        nonlocal n
        lit = m.group(2)
        if not lit or lit.startswith("/"):
            return m.group(0)
        new = os.path.relpath(os.path.normpath(os.path.join(a, lit)), b).replace(os.sep, "/")
        n += 1
        return f'{m.group(1)}"{new}"'

    return PATHLIT.sub(sub, text), n


# ───────────────────────── 缩进：去一层 / 还原一层，必须可逆 ─────────────────────────
def dedent(body_lines):
    """去掉一层（4 空格）缩进。

    🔴 **行首在多行字面量里的行一个字节都不动** —— 那几个空格是字符串的值，不是缩进
    （`设计/16 §4.1` 说的就是这一类：括号配平/文本处理在字符串字面量上会偏）。
    其余非空行必须是 4 空格起头，否则返回 (False, …) ⇒ 调用方跳过该文件。
    """
    lit = literal_line_starts("\n".join(body_lines))
    out = []
    for i, l in enumerate(body_lines):
        if i in lit or l == "":
            out.append(l)
        elif l.startswith("    "):
            out.append(l[4:])
        else:
            return False, body_lines
    return True, out


def reindent(body_lines):
    """dedent 的逆。判断「哪行不动」时**只看磁盘上那份**（＝ dedent 的产物），
    所以这个还原是任何人拿着 tests/ 那个文件就能独立重跑的 —— §6.1 要的正是这个。
    """
    lit = literal_line_starts("\n".join(body_lines))
    return [l if (i in lit or l == "") else "    " + l for i, l in enumerate(body_lines)]


def rel_up(from_file: str, to_file: str) -> str:
    return os.path.relpath(os.path.join(REPO, to_file), os.path.dirname(os.path.join(REPO, from_file)))


def dest_for(src_rel: str, mod_name: str, single: bool) -> str:
    """src/bridge/src/foo.rs           → tests/bridge/foo_tests.rs
    src/bridge/src/a/b.rs              → tests/bridge/a/b_tests.rs
    src/bridge/crates/x-core/src/l.rs  → tests/bridge/crates/x-core/l_tests.rs
    同一文件有多个测试模块时用模块名消歧：<stem>_<mod>.rs
    """
    p = src_rel
    assert p.startswith("src/bridge/")
    rest = p[len("src/bridge/"):]
    if rest.startswith("src/"):
        rest = rest[len("src/"):]
    else:
        m = re.match(r"^(crates/[^/]+)/src/(.*)$", rest)
        if m:
            rest = m.group(1) + "/" + m.group(2)
    d, base = os.path.split(rest)
    stem = base[:-3]
    if single and mod_name == "tests":
        name = f"{stem}_tests.rs"
    elif mod_name == "tests":
        name = f"{stem}_tests.rs"
    else:
        name = f"{stem}_{mod_name}.rs"
    return os.path.join("tests/bridge", d, name)


def split_file(src_rel: str):
    """返回 (新的 src 文本, [(目标路径, 测试文本)], 跳过原因 or None)。"""
    path = os.path.join(REPO, src_rel)
    orig = open(path, "rb").read().decode("utf-8")
    blocks = find_blocks(orig)
    if not blocks:
        return None, None, "没有顶层 `#[cfg(test)] mod X { … }` 内联块"
    lines = orig.split("\n")
    single = len(blocks) == 1
    dests = {}
    for _, _, name, _ in blocks:
        d = dest_for(src_rel, name, single)
        if d in dests:
            return None, None, f"目标路径撞车：{d}"
        dests[d] = name
    new_lines = []
    moved = []
    prev = 0
    for attr_start, mod_line, name, end_line in blocks:
        new_lines.extend(lines[prev:attr_start])
        body = lines[mod_line + 1 : end_line]
        ok, ded = dedent(body)
        if not ok:
            return None, None, f"mod {name}：测试体里有非 4 空格起头的行（缩进不可逆，多半是列 0 的多行字面量）"
        # 回拼对账：还原一层缩进后必须与原文逐字节相同
        if reindent(ded) != body:
            return None, None, f"mod {name}：去缩进→还原缩进不是恒等（逐字节对账失败）"
        dest = dest_for(src_rel, name, single)
        body, _ = retarget("\n".join(ded), src_rel, dest)
        moved.append((dest, body))
        new_lines.extend(lines[attr_start:mod_line])  # #[cfg(test)] 与其它属性行
        new_lines.append(f'#[path = "{rel_up(src_rel, dest)}"]')
        new_lines.append(re.sub(r"\s*\{\s*$", ";", lines[mod_line]))
        prev = end_line + 1
    new_lines.extend(lines[prev:])
    return "\n".join(new_lines), moved, None


# ───────────────────────── 逐字节回拼对账（§6.1 那条纪律） ─────────────────────────
STUB = re.compile(r'^#\[path = "[^"]*"\]$')


def verify(src_rel: str, new_src: str, moved) -> str:
    """把 src 段与 test 段拼回去，必须与原文件逐字节相同。不同就返回原因。"""
    orig = open(os.path.join(REPO, src_rel), "rb").read().decode("utf-8")
    body_of = {d: t for d, t in moved}
    out = []
    lines = new_src.split("\n")
    i = 0
    while i < len(lines):
        if CFG_TEST.match(lines[i]):
            j = i + 1
            while j < len(lines) and ATTR.match(lines[j]) and not STUB.match(lines[j]):
                j += 1
            if j < len(lines) and STUB.match(lines[j]):
                dest = re.search(r'"([^"]*)"', lines[j]).group(1)
                dest = os.path.relpath(
                    os.path.normpath(os.path.join(os.path.dirname(os.path.join(REPO, src_rel)), dest)), REPO
                )
                decl = lines[j + 1]
                m = re.match(r"^((pub(\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*)\s*;$", decl)
                if m and dest in body_of:
                    out.extend(lines[i:j])
                    out.append(m.group(1) + " {")
                    back, _ = retarget(body_of[dest], dest, src_rel)
                    out.extend(reindent(back.split("\n")))
                    out.append("}")
                    i = j + 2
                    continue
        out.append(lines[i])
        i += 1
    rebuilt = "\n".join(out)
    if rebuilt == orig:
        return ""
    # 指出第一处差异，方便人工处理
    for k, (a, b) in enumerate(zip(rebuilt.split("\n"), orig.split("\n"))):
        if a != b:
            return f"回拼与原文第 {k + 1} 行起不同：{a[:60]!r} != {b[:60]!r}"
    return f"回拼长度不同：{len(rebuilt)} != {len(orig)}"


# ───────────────────────── 人群：`真相源/00`「混合文件（唯一住址）」的口径 ─────────────────────────
PROD = re.compile(r"^\s*(pub\s+(fn|struct|enum|trait|const|static)\s|fn\s|struct\s|enum\s|impl\b|trait\s|const\s|static\s)")
TESTMARK = re.compile(r"#!?\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]|#\s*\[\s*test\s*\]")
DECL_FORM = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(mod|use)\s+[A-Za-z_][A-Za-z0-9_]*\s*;")


def is_mixed(text: str) -> bool:
    """混合文件 ＝ src/ 下的 .rs，同一文件里既有生产项又有测试段。

    ⚠ `#[cfg(test)] … mod x;` 这个**分号形式**不算测试段（`设计/16 §4.1`：剖分之后
    src/ 里的 cfg(test) 只剩这一形）。在 2026-09-18 那份基线上，算不算它，
    194/106 这两个数**一模一样** —— 所以这不是重新定义口径，是把口径写到能量出变化。
    """
    lines = text.split("\n")
    first = None
    for i, l in enumerate(lines):
        if not TESTMARK.search(l):
            continue
        if re.match(r"^#\[cfg\(test\)\]\s*$", l.strip()):
            j = i + 1
            while j < len(lines) and re.match(r"^\s*#\[", lines[j]):
                j += 1
            if j < len(lines) and DECL_FORM.match(lines[j]):
                continue
        elif re.match(r"^#\[cfg\(test\)\]\s*(pub(\([^)]*\))?\s+)?(mod|use)\s+[A-Za-z_][A-Za-z0-9_]*\s*;", l.strip()):
            continue
        first = i
        break
    if first is None:
        return False
    return any(PROD.match(l) for l in lines[:first])


def census(root=REPO):
    files = subprocess.run(
        ["find", "src", "-name", "*.rs", "-not", "-path", "*/vendor/*"], cwd=root, capture_output=True, text=True
    ).stdout.split()
    mixed, total_lines = [], 0
    for f in sorted(files):
        data = open(os.path.join(root, f), "rb").read().decode("utf-8", errors="replace")
        if is_mixed(data):
            mixed.append(f)
            total_lines += data.count("\n")
    return len(files), mixed, total_lines


# ───────────────────────── 反空真：证明那条对账能红 ─────────────────────────
def selfcheck(files):
    """§16 §5.2「恒绿看起来和真绿一模一样」——对账本身也要被证明能红。

    对每个样本做三种变异（删一行测试体 / 吃掉一行缩进 / 删一行 src 段），
    三种都必须被 verify() 逮住。有一种没红，这条对账就不携带信息。
    """
    bad = 0
    for f in files:
        new_src, moved, why = split_file(f)
        if why:
            print(f"  ?  {f} —— 剖不开（{why}），跳过自检")
            continue
        if verify(f, new_src, moved) != "":
            print(f"  🔴 {f} —— 未变异就对不上")
            bad += 1
            continue
        dest, body = moved[0]
        lines = body.split("\n")
        k = len(lines) // 2
        m1 = verify(f, new_src, [(dest, "\n".join(lines[:k] + lines[k + 1:]))] + moved[1:])
        l2 = list(lines)
        for i, l in enumerate(l2):
            if l.startswith("    ") and l.strip():
                l2[i] = l.lstrip()
                break
        m2 = verify(f, new_src, [(dest, "\n".join(l2))] + moved[1:])
        nl = new_src.split("\n")
        m3 = verify(f, "\n".join(nl[:len(nl) // 2] + nl[len(nl) // 2 + 1:]), moved)
        ok = bool(m1) and bool(m2) and bool(m3)
        print(f"  {'✔' if ok else '🔴'} {f}  删测试行={'红' if m1 else '没抓到'} "
              f"吃缩进={'红' if m2 else '没抓到'} 删src行={'红' if m3 else '没抓到'}")
        bad += 0 if ok else 1
    print(f"自检：{len(files)} 个样本，{bad} 个没能让对账变红")
    return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--survey", action="store_true")
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--census", action="store_true")
    ap.add_argument("--selfcheck", action="store_true")
    ap.add_argument("--only", nargs="*")
    a = ap.parse_args()

    if a.census:
        n, mixed, lines = census()
        print(f"src/ 下 .rs（不含 vendor/）：{n}")
        print(f"其中混合：{len(mixed)}   合计行数：{lines}")
        for f in mixed:
            print("   ", f)
        return

    if a.selfcheck:
        sample = a.only or [
            "src/bridge/src/ssh_source.rs",
            "src/bridge/src/cc_bus.rs",
            "src/bridge/src/profile_installer.rs",
            "src/bridge/src/adapter.rs",
            "src/bridge/src/utils.rs",
        ]
        return 1 if selfcheck(sample) else 0

    files = a.only or sorted(
        subprocess.run(
            ["find", "src/bridge", "-name", "*.rs", "-not", "-path", "*/vendor/*"],
            cwd=REPO, capture_output=True, text=True,
        ).stdout.split()
    )
    done, skipped = [], []
    for f in files:
        text = open(os.path.join(REPO, f), "rb").read().decode("utf-8")
        if not is_mixed(text):
            continue
        new_src, moved, why = split_file(f)
        if why:
            skipped.append((f, why))
            continue
        why = verify(f, new_src, moved)
        if why:
            skipped.append((f, why))
            continue
        done.append((f, new_src, moved))
        if a.apply:
            for dest, body in moved:
                os.makedirs(os.path.dirname(os.path.join(REPO, dest)), exist_ok=True)
                with open(os.path.join(REPO, dest), "w", encoding="utf-8") as fh:
                    fh.write(body if body.endswith("\n") else body + "\n")
            with open(os.path.join(REPO, f), "w", encoding="utf-8") as fh:
                fh.write(new_src)

    print(f"处理 {len(done)} 份 · 跳过 {len(skipped)} 份")
    for f, _, moved in done:
        print(f"  ✔ {f}  →  {', '.join(d for d, _ in moved)}")
    for f, why in skipped:
        print(f"  ✘ {f}  —— {why}")
    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
