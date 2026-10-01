#!/usr/bin/env python3
""" —— 仓库重组第 3 批：src/frontend/shell/ 混合文件剖分。

把 `src/frontend/shell/` 里 `#[cfg(test)] mod X { … }` 这种**内联测试模块块**整块搬到
`<repo>/tests/frontend/shell/…` 下，原地只留那三行：

    #[cfg(test)]
    #[path = "../../../tests/frontend/shell/<…>.rs"]
    mod X;

🔴 那条纪律在这里是**硬的**：对每个文件，剖分后 src 段 + test 段
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

# 🔴 步 7c 加 `all(test, …)` 那一支。盘上一共 5 种含 `test` 的 cfg 形，**两族含义相反**：
#   · `#[cfg(test)]` 280 处 · `#[cfg(all(test, target_os = "linux"))]` 4 处
#     · `#[cfg(all(test, not(windows)))]` 4 处      ⇒ **只在 test 下存在** ⇒ 是测试段，搬。
#   · `#[cfg(any(windows, test))]` 6 处 · `#[cfg(any(not(windows), test))]` 1 处
#     ⇒ **生产下也存在**（测试只是额外的一个开关）⇒ 是生产项，**不许搬**。
# 只认 `all(test, …)`，不认 `any(…, test)` —— 这条区分是语义的，不是形状的。
CFG_TEST = re.compile(r"^#\[cfg\((?:test|all\(test\s*,[^\n]*\))\)\]$")
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
# 那三条人群规则：`include_str!` 与 `include_bytes!` 是同一个人群
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
    （说的就是这一类：括号配平/文本处理在字符串字面量上会偏）。
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


# 两棵生产树 → 两棵测试树。`src/backend` 这一格是步 7c 加的（第 2 批
# 只搬了目录与 19 份纯测试文件，**剖分从来没排进那三批**）。
TREES = {"src/frontend/shell": "tests/frontend/shell", "src/backend": "tests/backend"}


def tree_of(src_rel: str) -> str:
    for t in TREES:
        if src_rel == t or src_rel.startswith(t + "/"):
            return t
    raise AssertionError(f"不在任何一棵生产树里：{src_rel}")


def dest_for(src_rel: str, mod_name: str, single: bool) -> str:
    """src/frontend/shell/src/foo.rs           → tests/frontend/shell/foo_tests.rs
    src/frontend/shell/src/a/b.rs              → tests/frontend/shell/a/b_tests.rs
    src/common/x-core/src/l.rs  → tests/common/x-core/l_tests.rs
    src/backend/foo.rs                 → tests/backend/foo_tests.rs
    src/backend/a/b.rs                 → tests/backend/a/b_tests.rs
    同一文件有多个测试模块时用模块名消歧：<stem>_<mod>.rs

    🔴 `mod.rs` 特例：`a/mod.rs` 的 stem 是字面的 `mod`，直接拼会得到
    `a/mod_tests.rs` —— 那个名字不携带信息，而且同一棵树下每个目录都会出一个同名文件
    （只靠目录区分）。改用**目录名**当 stem 并上移一层：`a/mod.rs → a_tests.rs`。
    """
    tree = tree_of(src_rel)
    rest = src_rel[len(tree) + 1:]
    if tree == "src/frontend/shell":
        if rest.startswith("src/"):
            rest = rest[len("src/"):]
        else:
            m = re.match(r"^(crates/[^/]+)/src/(.*)$", rest)
            if m:
                rest = m.group(1) + "/" + m.group(2)
    d, base = os.path.split(rest)
    stem = base[:-3]
    if stem == "mod" and d:
        d, stem = os.path.split(d)
    name = f"{stem}_tests.rs" if mod_name == "tests" else f"{stem}_{mod_name}.rs"
    return os.path.join(TREES[tree], d, name)


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


# ───────────────────────── 人群：「混合文件（唯一住址）」的口径 ─────────────────────────
PROD = re.compile(r"^\s*(pub\s+(fn|struct|enum|trait|const|static)\s|fn\s|struct\s|enum\s|impl\b|trait\s|const\s|static\s)")
TESTMARK = re.compile(r"#!?\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]|#\s*\[\s*test\s*\]")
DECL_FORM = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(mod|use)\s+[A-Za-z_][A-Za-z0-9_]*\s*;")


def is_mixed(text: str) -> bool:
    """混合文件 ＝ src/ 下的 .rs，同一文件里既有生产项又有测试段。

    ⚠ `#[cfg(test)] … mod x;` 这个**分号形式**不算测试段（剖分之后
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

    六种变异，每一种都必须被 verify() 逮住：

    | # | 变异 | 它治的失效 |
    |---|---|---|
    | 1 | 删测试体中间一行 | 回拼漏掉测试段内容 |
    | 2 | 吃掉测试体一行的缩进 | `reindent` 不是 `dedent` 的逆 |
    | 3 | 删 src 段一行 | 回拼漏掉生产段内容 |
    | 4 | 改**最后一块**的测试体 | 多块文件里只对账了第一块（步 7c 新加）|
    | 5 | 篡改测试体里的 `include_str!`/`#[path]` 字面量 | `retarget` 的来回改写不是对合（步 7c 新加）|
    | 6 | 在测试体**末尾追加**一行 | 变异 1 删中间，逮不到「末尾多/少一行」（步 7c 新加）|

    🔴 变异 4/5/6 是步 7c 为 `src/backend` 补的：那棵树的形状与 `src/frontend/shell` 不同 ——
    多块文件更多（`main.rs` 4 块 · `observe/history_query.rs` 4 块）、`mod.rs` 占比更高
    （目标名要用目录名，见 `dest_for`）、`include_str!` 更密。**变异 4 是其中最要紧的一条**：
    原来的三种全部只动 `moved[0]`，一个「只对账第一块」的 bug 会安静地绿。

    🔴 **反空真：真被验到的样本数为 0 ⇒ 硬错。**〔步 7c 现打撞上〕
    原来的默认样本是步 7b 已经剖完的那 5 份文件 —— 它们今天已经没有内联块了，
    于是六种变异一个都没跑，而末行照样打印「0 个没能让对账变红」。
    那正是 `§5.2` 说的「扫空集 ⇒ 恒绿」，且它**看起来和真绿一模一样**。
    """
    bad = 0
    exercised = 0
    for f in files:
        new_src, moved, why = split_file(f)
        if why:
            print(f"  ?  {f} —— 剖不开（{why}），跳过自检")
            continue
        if verify(f, new_src, moved) != "":
            print(f"  🔴 {f} —— 未变异就对不上")
            bad += 1
            continue
        exercised += 1
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

        # ④ 动**最后一块**（单块文件时 moved[-1] is moved[0]，这一格退化成变异 1 的同义重复，
        #    但对多块文件它是唯一能逮住「只对账第一块」的那一条）
        ldest, lbody = moved[-1]
        llines = lbody.split("\n")
        lk = len(llines) // 2
        m4 = verify(f, new_src, moved[:-1] + [(ldest, "\n".join(llines[:lk] + llines[lk + 1:]))])

        # ⑤ 篡改测试体里的 include 路径字面量（有就改，没有就记 n/a）
        m5 = None
        for idx, (d, b) in enumerate(moved):
            hit = PATHLIT.search(b)
            if hit:
                tampered = b[:hit.start(2)] + "ZZ_tampered/" + b[hit.start(2):]
                m5 = verify(f, new_src, moved[:idx] + [(d, tampered)] + moved[idx + 1:])
                break

        # ⑥ 末尾追加一行（变异 1 删的是中间，逮不到末尾那一格）
        m6 = verify(f, new_src, [(dest, body + "\nfn zz_appended_by_selfcheck() {}")] + moved[1:])

        checks = {"删测试行": m1, "吃缩进": m2, "删src行": m3, "动末块": m4, "篡改include": m5, "尾部加行": m6}
        ok = all(bool(v) for v in checks.values() if v is not None)
        got = " ".join(
            f"{k}={'n/a' if v is None else ('红' if v else '没抓到')}" for k, v in checks.items()
        )
        print(f"  {'✔' if ok else '🔴'} {f}  ({len(moved)} 块)  {got}")
        bad += 0 if ok else 1

    print(f"自检：{len(files)} 个样本，真验到 {exercised} 个，{bad} 个没能让对账变红")
    if exercised == 0:
        print("  🔴 **反空真触发**：一个样本都没真被验到 ⇒ 上面那句「0 个没能让对账变红」"
              "不携带信息。给 --only 指几份**还有内联块**的文件。")
        return max(bad, 1)
    return bad


# ───────────────────── `--selfcheck` 的合成夹具 ─────────────────────
#
# 🔴 剖分做完之后盘上**没有**「还有内联块」的文件了 —— 那意味着上面那条反空真
# 会永远触发，而量具从此**证不了自己有牙**。⇒ 没给 `--only` 时现造这一份。
#
# 它覆盖六种变异各自要咬的那一格：
#   · **两个**内联块（变异④「动末块」只有多块文件才认真）
#   · 一段**多行原始字面量**，且**行首落在字面量里**（`dedent` 一个字节都不许动那几个空格）
#   · 一处 `include_*!` 相对路径（变异⑤「篡改 include」）
#   · 生产段有真项（`pub fn`），不然它不是「混合文件」那一形
SELFCHECK_FIXTURE = '//! `--selfcheck` 的合成夹具。**跑完即删**，不该出现在任何一次提交里。\n\npub fn produce() -> usize {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    use super::produce;\n\n    /// 多行原始字面量：**下面那几行的行首空格是字符串的值**，去缩进不许动它们。\n    const SHAPE: &str = r#"\n    line with four leading spaces\n        line with eight\n"#;\n\n    const EMBEDDED: &str = include_str!("../../../README.md");\n\n    #[test]\n    fn the_production_item_is_reachable() {\n        assert_eq!(produce(), 1);\n        assert!(SHAPE.contains("four leading"));\n        assert!(!EMBEDDED.is_empty());\n    }\n}\n\n#[cfg(test)]\nmod more_tests {\n    #[test]\n    fn the_last_block_is_also_accounted_for() {\n        assert!(1 + 1 == 2);\n    }\n}\n'

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--survey", action="store_true")
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--census", action="store_true")
    ap.add_argument("--selfcheck", action="store_true")
    ap.add_argument("--only", nargs="*")
    ap.add_argument("--tree", nargs="*", choices=list(TREES))
    a = ap.parse_args()

    if a.census:
        n, mixed, lines = census()
        print(f"src/ 下 .rs（不含 vendor/）：{n}")
        print(f"其中混合：{len(mixed)}   合计行数：{lines}")
        for f in mixed:
            print("   ", f)
        return

    if a.selfcheck:
        # 没给 `--only` ⇒ 现造合成夹具（理由见 `SELFCHECK_FIXTURE` 的头注），跑完**一定删掉**。
        # ⚠ 夹具落在 `src/frontend/shell/src/` 下是刻意的：`dest_for` / `rel_up` 都按树认住址。
        #   它只在这条路上短暂存在，且 `--selfcheck` **不写盘目标文件**。
        if not a.only:
            fx_rel = "src/frontend/shell/src/zz_selfcheck_fixture.rs"
            fx_abs = os.path.join(REPO, fx_rel)
            assert not os.path.exists(
                fx_abs
            ), f"{fx_rel} 已存在 —— 上一趟没清干净，先手工删掉"
            with open(fx_abs, "w", encoding="utf-8") as fh:
                fh.write(SELFCHECK_FIXTURE)
            try:
                print(f"〔合成夹具〕{fx_rel}（跑完即删）")
                bad = selfcheck([fx_rel])
            finally:
                os.remove(fx_abs)
            return 1 if bad else 0
        # 两棵树各挑：最大的 · 块最多的 · 带 include_str! 的 · mod.rs 那一形
        # （步 7c 换过一次：原来那 5 份是步 7b 已经剖完的文件，今天没有内联块 ⇒ 空转）
        sample = a.only or [
            "src/frontend/shell/src/tool_registry.rs",          # 3773 行 · 3 块 · include_str!
            "src/frontend/shell/src/lib.rs",                    # 5 块（本仓最多）
            "src/common/guard-core/src/lib.rs",  # crates/ 那一形
            "src/backend/observe/watcher.rs",           # 5306 行（本仓最大）
            "src/backend/main.rs",                      # 4 块 · include_str!
            "src/backend/observe/history_query.rs",     # 4 块
            "src/backend/dial/mod.rs",                  # mod.rs → 目录名那一形
            "src/backend/platform/pidwatch/mod.rs",     # mod.rs ＋ 2 块
        ]
        return 1 if selfcheck(sample) else 0

    trees = a.tree or list(TREES)
    files = a.only or sorted(
        subprocess.run(
            ["find", *trees, "-name", "*.rs", "-not", "-path", "*/vendor/*"],
            cwd=REPO, capture_output=True, text=True,
        ).stdout.split()
    )

    # 🔴 步 7c：**选片不再走 `is_mixed()`。**
    # `is_mixed()` 是「混合文件（唯一住址）」那个**报数**口径，
    # 它答的是「这份文件里生产项与测试段同住吗」；而 `--apply` 要答的是
    # 「这份文件里有没有剖得开的顶层 `#[cfg(test)] mod X { … }`」——**两个不同的问题**。
    # 步 7b 拿前者当后者用，于是**漏掉 42 份 bridge ＋ 18 份 backend**：
    # 那些文件（整份是判据的 registry 一族）在 `is_mixed()` 眼里不混合，
    # 因为它把「第一个测试标记」认在了头注里那句 `//! … `#[test]` …` 上
    # （`TESTMARK` 不跳注释），于是「生产项在它之前吗」恒为假。
    # ⇒ 选片改用 `find_blocks()` —— 它就是剖分器自己的判断，问的正是要做的那件事。
    done, skipped = [], []
    claimed = {}
    for f in files:
        text = open(os.path.join(REPO, f), "rb").read().decode("utf-8")
        if not find_blocks(text):
            continue
        new_src, moved, why = split_file(f)
        if why:
            skipped.append((f, why))
            continue
        why = verify(f, new_src, moved)
        if why:
            skipped.append((f, why))
            continue
        # 目标路径必须没人占：跨文件撞车 / 盘上已有同名文件，两种都不许静默覆盖
        clash = next((d for d, _ in moved if d in claimed), None)
        if clash:
            skipped.append((f, f"目标路径已被 {claimed[clash]} 占用：{clash}"))
            continue
        clash = next((d for d, _ in moved if os.path.exists(os.path.join(REPO, d))), None)
        if clash:
            skipped.append((f, f"目标路径盘上已存在：{clash}"))
            continue
        for d, _ in moved:
            claimed[d] = f
        done.append((f, new_src, moved))
        if a.apply:
            for dest, body in moved:
                os.makedirs(os.path.dirname(os.path.join(REPO, dest)), exist_ok=True)
                with open(os.path.join(REPO, dest), "w", encoding="utf-8") as fh:
                    fh.write(body if body.endswith("\n") else body + "\n")
            with open(os.path.join(REPO, f), "w", encoding="utf-8") as fh:
                fh.write(new_src)

    nblocks = sum(len(m) for _, _, m in done)
    print(f"处理 {len(done)} 份（{nblocks} 个测试模块） · 跳过 {len(skipped)} 份")
    for f, _, moved in done:
        print(f"  ✔ {f}  →  {', '.join(d for d, _ in moved)}")
    for f, why in skipped:
        print(f"  ✘ {f}  —— {why}")
    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
