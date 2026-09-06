#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R31 D1②/D2 的量具：**剥掉注释之后**，「在 fork 与 exec 之间插一段代码」的写法还剩几处。

# 它是什么、不是什么

- **它是一次性读数的量具**，不是判据。判据是 Rust 侧那条
  `nothing_in_the_production_path_runs_code_between_fork_and_exec`
  （住 `src-tauri/src/backend/control/local_backend.rs` 的测试段），
  它才是每轮门禁会跑的那个。
- 本脚本存在的理由有两个：
  ① `D1②` 要一个**不是 PM 那把有洞的尺子**打出来的数（PM 用的
     `grep -v ':[0-9]*://'` 只剥**行首顶格**的 `//`，缩进的注释行剥不掉、块注释一个字都剥不掉）；
  ② 给 Rust 那条判据一个**独立的第二实现**做对拍 —— 两把尺子各自写、答案对得上，
     才不是「一个实现自己跟自己说对」。
  ⇒ **两把尺子的分工写死在这里**：Rust 那把是权威（门禁跑它），本脚本是对照（一次性）。

# 单位：本脚本一律**同时**印三个数，别再留 `K-R30` 那个歧义

`K-R30` 头注写的是「**1 处**」，而按行数是 **3**（同一段 `///` 里连着的三行）。
⇒ 每一格都印：**命中行数** · **命中块数**（相邻命中行合成一块）· **命中文件数**。

# 剥法（与 `guard_core` 那把是两份独立实现，刻意的）

单趟词法扫描，状态 = 码 / 普通串 / 原始串(`r"` `r#"…"#` `b"` `br#"…"#`) / 字符字面量 /
行注释(`//`，**不论缩进**) / 块注释(`/* */`，**带深度**，Rust 允许嵌套)。
注释内容抹成**等长空格**、换行保留 ⇒ **行号与原文一一对应**。

⚠ 它剥不掉的（写出来，别读成「注释都剥干净了」）：
宏里拼出来的注释、`include!` 进来的文本、以及**字符串字面量里**写的那个词
（那本来就该算命中 —— 字符串里的 `pre_exec` 可以是真的传给 shell 的东西）。

# 怎么跑

    python3 evidence/K-R31-fork-exec-forms.py            # 默认量本脚本所在的那棵树
    python3 evidence/K-R31-fork-exec-forms.py --root <树根>

被测对象 = `--root` 指到的那棵树（默认 = 本文件的上级目录）。**读数只对那棵树成立。**
脚本先跑一组自检（阳性/阴性对照 + 行数不变 + 扫描面非空），自检不过**直接非零退出**，
不出读数 —— 「命令没跑」和「跑了结果是空」在终端上一模一样。
"""

import argparse
import os
import re
import sys

# ── 形态表（**枚举，不是全称**）────────────────────────────────────────────
#
# 每一条都写清它为什么在这张表里。表外的写法（自己 `clone(2)` · 换一个装 fd 的
# crate · 走 `nix`）本脚本**看不见** —— 报数时分母就是这张表。
FORMS = [
    ("pre_exec", "std `CommandExt::pre_exec`：闭包在孩子里、`execve` 之前跑"),
    ("before_exec", "同一件事的旧名（已弃用，仍编得过）"),
    ("libc::fork", "手写 fork+exec ⇒ 中间那段全归调用方"),
    ("libc::vfork", "同上，且它连地址空间都不换"),
    ("command_fds", "那个 crate 装 fd 用的就是 `pre_exec`"),
    ("CommandFdExt", "同上，trait 名那一半"),
    ("libc::posix_spawn", "手写 file actions：同一段窗口，只是搬进了 libc"),
]

SKIP_DIRS = {".git", "target", "node_modules", "dist", "coverage", ".vite"}

_RAW_OPEN = re.compile(r'(?:br|b|r)(?P<h>#*)"')
_CHAR = re.compile(r"'(?:\\u\{[0-9a-fA-F]{1,6}\}|\\.|[^\\'\n])'")
_IDENT = re.compile(r"[0-9A-Za-z_]")


def strip_comments(src: str) -> str:
    """把注释抹成等长空格（换行保留 ⇒ 行数与行号都不变）。"""
    out = []
    i, n = 0, len(src)
    depth = 0
    while i < n:
        if depth:
            if src.startswith("/*", i):
                depth += 1
                out.append("  ")
                i += 2
                continue
            if src.startswith("*/", i):
                depth -= 1
                out.append("  ")
                i += 2
                continue
            out.append("\n" if src[i] == "\n" else " ")
            i += 1
            continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
            continue
        if src.startswith("/*", i):
            depth = 1
            out.append("  ")
            i += 2
            continue
        # 原始/字节串：`r#type` 那种原始标识符不算（本正则要求井号之后紧跟引号）。
        if not (i and _IDENT.match(src[i - 1])):
            m = _RAW_OPEN.match(src, i)
            if m:
                close = '"' + (m.group("h") or "")
                j = src.find(close, m.end())
                j = n if j < 0 else j + len(close)
                out.append(src[i:j])
                i = j
                continue
        if src[i] == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(src[i:j])
            i = j
            continue
        if src[i] == "'":
            m = _CHAR.match(src, i)
            if m:  # 字符字面量；生命周期 `'a` 匹配不上 ⇒ 按普通字符走下去
                out.append(m.group(0))
                i = m.end()
                continue
        out.append(src[i])
        i += 1
    return "".join(out)


def strip_test_modules(src: str) -> str:
    """剥掉带花括号体的 `#[cfg(test)] mod X { … }`（口径照 `guard_core::production_source`）。

    ⚠ 这一份是**独立复写**，不是抄过来的 —— 两份实现对拍才有意义。
    与那一份同样的两条：`cfg` 必须只含 `test`；下一行必须是以 `{` 收尾的 `mod X`。
    """
    out = []
    i, n = 0, len(src)
    while True:
        rel = src.find("\n#[cfg(", i)
        if rel < 0:
            out.append(src[i:])
            return "".join(out)
        attr_start = rel + 1
        attr_end = src.find("\n", attr_start)
        attr_end = n if attr_end < 0 else attr_end
        attr = src[attr_start:attr_end]
        mod_start = min(attr_end + 1, n)
        mod_end = src.find("\n", mod_start)
        mod_end = n if mod_end < 0 else mod_end
        mod_line = src[mod_start:mod_end].strip()
        cfg_test_only = set(re.findall(r"[0-9A-Za-z_]+", attr)) == {"cfg", "test"}
        if not (cfg_test_only and mod_line.startswith("mod ") and mod_line.endswith("{")):
            out.append(src[i:attr_end])
            i = attr_end
            continue
        out.append(src[i:rel])
        end = src.find("\n}", rel)
        if end < 0:
            return "".join(out)
        i = end + 2


def hits(text: str):
    """命中的 (行号 1 基, 形态, 该行 strip 之后的内容)。"""
    out = []
    for k, line in enumerate(text.split("\n"), 1):
        for form, _why in FORMS:
            if form in line:
                out.append((k, form, line.strip()))
                break
    return out


def blocks(hs) -> int:
    """相邻命中行合成一块 —— 「处」那个单位的一种可判定读法。"""
    n, prev = 0, None
    for line_no, _f, _t in hs:
        if prev is None or line_no != prev + 1:
            n += 1
        prev = line_no
    return n


def rs_files(root: str):
    for d, dirs, files in os.walk(root):
        dirs[:] = sorted(x for x in dirs if x not in SKIP_DIRS)
        for f in sorted(files):
            if f.endswith(".rs"):
                yield os.path.join(d, f)


def self_check() -> None:
    """量具自检。不过就非零退出，**不出读数**。"""
    live = 'unsafe { cmd.pre_exec(|| Ok(())) };'
    assert len(hits(strip_comments(live))) == 1, "阳性对照没被逮到 —— 形态表或剥法坏了"

    negatives = [
        "// unsafe { cmd.pre_exec(|| Ok(())) };",
        "        /// 那一档靠的是本仓一处 pre_exec 都没有",  # ← 缩进注释，PM 那把尺子剥不掉
        "    // x\n        //! pre_exec\n",
        "/*\ncmd.pre_exec(|| Ok(()));\n*/",
        "/* /* cmd.pre_exec() */ */",  # 嵌套块注释
        "let x = 1; // cmd.pre_exec()",  # 行尾注释
    ]
    for i, neg in enumerate(negatives):
        got = hits(strip_comments(neg))
        assert not got, f"阴性对照 #{i} 被数成了命中：{got!r} —— 剥注释那一步没在做"

    keep = [
        ('let s = "pre_exec";', "字符串字面量里的**该算**命中：它可能真的被递给 shell"),
        ('let s = r"http://x"; cmd.pre_exec();', "原始串里的 `//` 不许把后面整行吃掉"),
    ]
    for src, why in keep:
        assert hits(strip_comments(src)), f"假阴性：{why} —— {src!r}"

    # 行数与行号必须不变（诊断里要报行号）。
    sample = "a\n/*\nb\n*/\nc // d\ne\n"
    assert strip_comments(sample).count("\n") == sample.count("\n"), "剥法改了行数 ⇒ 行号全错"

    # `#[cfg(test)]` 剥法：带体的剥掉，无体的声明**不许**剥（剥了会吞掉后面全部生产码）。
    with_body = "fn p() {}\n#[cfg(test)]\nmod t {\n    fn q() { let _ = 1; }\n}\nfn r() {}\n"
    assert "fn q()" not in strip_test_modules(with_body), "带体的测试模块没剥掉"
    assert "fn r()" in strip_test_modules(with_body), "剥过头了，吞掉了后面的生产码"
    no_body = "fn p() {}\n#[cfg(test)]\nmod guard_support;\nfn r() {}\n"
    assert "fn r()" in strip_test_modules(no_body), "无花括号体的声明被当成了模块体 ⇒ 吞掉生产码"


def report(title: str, per_file, files_scanned: int, bytes_scanned: int, unit: str):
    """印一格，返回 `(行, 块, 文件)` 三个数 —— **三个都返回**，别只返回行。

    ⚠ 初版这里只返回「行」，于是末尾那句总结把「块」写成了**硬编码的 2**：
    在立本件的那棵树上恰好对，换一棵树当场变成假话。**报一个数就得从这一趟算出来。**
    """
    total = sum(len(h) for _f, h in per_file)
    hit_files = [(f, h) for f, h in per_file if h]
    blk = sum(blocks(h) for _f, h in hit_files)
    print(f"## {title}")
    print(f"   扫描面：{files_scanned} 份 `.rs` · {bytes_scanned} 字节（{unit}）")
    print(f"   命中：**{total} 行** · **{blk} 块** · **{len(hit_files)} 份文件**")
    for f, h in hit_files:
        for line_no, form, text in h:
            print(f"     {f}:{line_no}  〔{form}〕 {text[:110]}")
    print()
    return total, blk, len(hit_files)


def prod_text(src: str) -> str:
    """生产段的**语义文本**：剥测试段 + 剥注释 + 把所有空白归一。

    为什么归一空白：剥注释是**等长抹空格**（为了保住行号），于是加一行注释会让
    字节数变而语义没变。归一之后剩下的就是「这份文件的生产代码到底是什么」。
    """
    return " ".join(strip_comments(strip_test_modules(src)).split())


def prod_diff(rev: str, paths: list[str]) -> int:
    """`rev` 与**工作树**之间，逐份文件的生产段有没有变。

    🔴 正常结论是「四份全同」——**那是一个恒同格**，所以同一次输出里必须有非零对照：
    ① 整文件 md5 的差（改了注释就该不同）；② 一次**阳性自检**（人造一处真改动，
    比较器必须认得出）。两者若一起是「无差别」，说明比较器根本没在比。
    """
    import hashlib
    import subprocess

    # 阳性自检：比较器认得出一处**生产段**的真改动，也认得出「只加注释」不算改动。
    a = "fn f() { let x = 1; }\n#[cfg(test)]\nmod t {\n    fn q() { let _ = 9; }\n}\n"
    b = "fn f() { let x = 2; }\n#[cfg(test)]\nmod t {\n    fn q() { let _ = 9; }\n}\n"
    c = "// 新注释\nfn f() { let x = 1; }\n#[cfg(test)]\nmod t {\n    fn q() { let _ = 8; }\n}\n"
    assert prod_text(a) != prod_text(b), "比较器认不出生产段的真改动 —— 本模式在空转"
    assert prod_text(a) == prod_text(c), "比较器把「只加注释 / 只改测试段」当成了生产改动"
    print("比较器自检：过（真改动认得出 · 只加注释不算 · 只改测试段不算）")
    print(f"对拍：{rev} → 工作树\n")

    changed = 0
    md5_differs = 0
    for p in paths:
        old = subprocess.run(
            ["git", "show", f"{rev}:{p}"], capture_output=True, check=True
        ).stdout.decode("utf-8")
        with open(p, "r", encoding="utf-8") as fh:
            new = fh.read()
        same = prod_text(old) == prod_text(new)
        ma = hashlib.md5(old.encode()).hexdigest()[:12]
        mb = hashlib.md5(new.encode()).hexdigest()[:12]
        if ma != mb:
            md5_differs += 1
        if not same:
            changed += 1
        print(f"── {p}")
        print(f"   生产段（剥测试段+剥注释+归一空白）：{'**同**' if same else '**变了**'}")
        print(f"   非空对照 · 整文件 md5：{ma} → {mb}  {'**不同**' if ma != mb else '相同'}")
    print()
    print(f"生产段变了的文件：{changed} 份 / {len(paths)} 份")
    print(f"非空对照（整文件 md5 不同的）：{md5_differs} 份 —— 这一格非零，上面的「全同」才是比出来的")
    if md5_differs == 0:
        print("!! 一格非零对照都没有 —— 这一趟没比到东西")
        return 4
    return 0


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(here))
    ap.add_argument("--prod-diff", metavar="REV",
                    help="改成「生产段对拍」模式：把 REV 与工作树逐份比，看生产代码变没变")
    ap.add_argument("paths", nargs="*", help="--prod-diff 模式下要比的文件（相对仓根）")
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    if args.prod_diff:
        if not args.paths:
            print("--prod-diff 要点名文件（相对仓根），不给就没有分母")
            return 2
        return prod_diff(args.prod_diff, args.paths)

    self_check()
    print(f"量具自检：过（阳性 1 · 阴性 6 · 假阴性对照 2 · 行数不变 · 测试段剥法 3）")
    print(f"被测对象：{root}")
    print(f"形态表（枚举，**不是全称**，表外的写法本脚本看不见）：")
    for form, why in FORMS:
        print(f"  - `{form}` —— {why}")
    print()

    raw, stripped, prod = [], [], []
    files_n = 0
    raw_bytes = strip_bytes = prod_bytes = 0
    for path in rs_files(root):
        rel = os.path.relpath(path, root)
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            src = fh.read()
        files_n += 1
        s = strip_comments(src)
        p = strip_comments(strip_test_modules(src))
        raw_bytes += len(src)
        strip_bytes += len(s)
        prod_bytes += len(p)
        raw.append((rel, hits(src)))
        stripped.append((rel, hits(s)))
        prod.append((rel, hits(p)))

    if files_n < 100:
        print(f"!! 只扫到 {files_n} 份 `.rs` —— 遍历坏了或 --root 指错了，读数不作数")
        return 2

    a = report("① 剥前（整棵树，含测试段与注释）—— 这就是 `K-R30` 那把尺子的分母",
               raw, files_n, raw_bytes, "原文")
    b = report("② 只剥注释（整棵树，仍含测试段）—— `D1②` 要的那一格",
               stripped, files_n, strip_bytes, "剥注释后，等长抹空格 ⇒ 与原文同长")
    c = report("③ 剥注释 + 剥测试段（= 生产段）—— Rust 那条判据的扫描面口径",
               prod, files_n, prod_bytes, "剥注释 + 剥测试段后")

    print("## 一句话")
    print(f"   剥前 {a[0]} 行 · 只剥注释 {b[0]} 行 · 生产段 {c[0]} 行"
          f"（分母：{files_n} 份 `.rs`，{root}）")
    print(f"   ⚠ 「行 / 块 / 处」是三个数，别混：本趟剥前那 {a[0]} 行落在 **{a[1]}** 个相邻块、"
          f"**{a[2]}** 份文件里，")
    print("     而「一段文档注释算 1 处」是第三种读法 —— `K-R30` 写的「1 处」就是那一种。")
    print("     ⇒ Rust 那条判据把单位钉死为**行**，并把三个数一起印。")
    print(f"   ⚠ ② 与 ③ 的差（{b[0]} → {c[0]}）就是**测试段里的那些**：判据自己的形态表与")
    print("     对照臂夹具都逐字写着这些词 ⇒ **只剥注释不够，必须连测试段一起剥**，")
    print("     否则那条判据一写完就红自己（`scanning_guard_registry` 治的正是这一族）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
