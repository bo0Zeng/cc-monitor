#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R52 D1 的量具：量「适配层那条线」到底漏在哪 —— **两族判据，一个人群**。

被测对象（写死在这里，免得这份量具被照住址复跑时量到别的树）：
    <本文件所在工作树>/remote-daemon-proto/src        （相对本文件：../remote-daemon-proto/src）
量于哪个提交：由 `--sha` 打印，调用方把它抄进报告。

# 为什么要有这一份（`platform/mod.rs` 头注自己写着的那一格）

> 「`platform/` 之外出现平台 cfg 就红」这条机检**是安慰剂** ——
> 编不过的头号错 `pidfd_open` **根本没有 cfg**。

⇒ 今天盘上那把尺子（`platform/fallback_guard.rs`）**口径是对的、人群不对**：
它只扫 `platform/` 一个目录，且只看**带 cfg** 的块。本量具把两件事一起补上：

- **A 族「无门的平台代码」**：一处平台原语，头上**没有任何平台 cfg**，
  它所在的文件也**不是**被 `#[cfg(平台)] mod x;` 整份选进来的 ⇒ 换个 target 就编不过。
  这一族**今天盘上零判据**，是本件的正题。
- **B 族「有门、但那扇门后面编了个乐观答案」**：`fallback_guard` 的性质原样搬来，
  人群从 `platform/` 一个目录扩到 `src/` 全树。

**回测样本（两个都必须逮到，逮不到就是尺子坏了）**：
  A 族 ⇒ `sidecars/codepicture/acquire.rs` 的 `use std::os::unix::fs::OpenOptionsExt;`
  B 族 ⇒ `plugin/discover.rs::is_executable` 的 `#[cfg(not(unix))] { true }`

# 剥注释的口径（不剥就会把散文数成命中）

这棵树的注释里**大量逐字写着代码片段**（`platform/mod.rs` 头注里就有 `pidfd_open`、
`fallback_guard.rs` 头注里有 `let _ = pid; true`）。本量具照
`guard-core::production_code` 的口径剥：① 逐个剥掉带花括号体的 `#[cfg(test)] mod X { … }`
② 剥块注释 ③ 删整行 `//` ④ 剥行尾 `//`。
与那份 Rust 实现的差别**只有一处、刻意的**：本量具把剥掉的字节**换成等量空格**而不是删掉，
这样字节偏移与行号一一对应，报得出住址。`--selfcheck` 会断言「剥完不含测试属性」
（`assert_no_test_code` 的同一条反向自检）。

用法：
    python3 K-R52-D1-platform-line-census.py            # 三堆 + 两族读数
    python3 K-R52-D1-platform-line-census.py --backtest  # 只跑两个已知样本的回测
    python3 K-R52-D1-platform-line-census.py --json      # 机读
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field

HERE = os.path.dirname(os.path.abspath(__file__))
TREE = os.path.abspath(os.path.join(HERE, "..", "remote-daemon-proto"))
SRC = os.path.join(TREE, "src")

# `--tree <路径>` 只改**被测对象**，不改口径。它存在的唯一理由是**回测要在基线上跑**：
# 两个已知样本 `K-R52` 当轮就修掉了，在修完的树上跑回测必然「没逮到」——
# 那不是尺子坏了，是病灶没了。⇒ 回测的正确跑法是指到 `d231e50` 的检出上。
# 🔴 每一次输出都把 `被测树` 与 `量于` 一起印出来，免得两份读数看起来一模一样而其实来自两棵树。


# ══════════════════════════════ 剥注释 / 剥测试段 ══════════════════════════════
# 口径逐条对齐 `src-tauri/crates/guard-core/src/lib.rs`，差别只有「换空格不删字节」。


def _blank(s: str) -> str:
    """把一段文本换成等长的空白，换行原样留着（⇒ 行号不变）。"""
    return "".join(c if c == "\n" else " " for c in s)


def _cfg_is_test_only(attr: str) -> bool:
    """属性里出现 `test` 这个**独立标识符**。口径抄 `guard_core::cfg_is_test_only`。"""
    for m in re.finditer("test", attr):
        k = m.start()
        before_ok = k == 0 or not (attr[k - 1].isalnum() or attr[k - 1] in "_-")
        after = k + 4
        after_ok = after >= len(attr) or not (attr[after].isalnum() or attr[after] in "_-")
        if before_ok and after_ok:
            return True
    return False


def _test_module_ranges(src: str) -> list[tuple[int, int]]:
    """带花括号体的 `#[cfg(test)] mod X { … }` 的字节区间。收尾判据 = 列 0 的 `}`。"""
    open_tok = "\n#[cfg("
    close_tok = "\n}"
    out: list[tuple[int, int]] = []
    i = 0
    while True:
        rel = src.find(open_tok, i)
        if rel < 0:
            return out
        j = rel

        def line_end(frm: int) -> int:
            k = src.find("\n", frm)
            return len(src) if k < 0 else k

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_end = line_end(mod_start)
        mod_line = src[mod_start:mod_end].strip()
        is_test_mod = (
            _cfg_is_test_only(src[attr_start:attr_end])
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        k = src.find(close_tok, j)
        if k < 0:
            out.append((j, len(src)))
            return out
        end = k + len(close_tok)
        out.append((j, end))
        i = end


def _strip_comments_keep_offsets(src: str) -> str:
    """剥块注释 + 行注释（含行尾），字符串 / 原始字符串安全；剥掉的换成空格。"""
    out = list(src)
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        # 原始字符串 r"..." / r#"..."#
        if c == "r" and i + 1 < n and src[i + 1] in '"#':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                i = n if k < 0 else k + len(term)
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            i = j
            continue
        if c == "'":
            # 字符字面量 vs 生命周期：只认 'x' / '\x' 这两形，其余原样跳过。
            m = re.match(r"'(\\.|[^\\'])'", src[i:])
            if m:
                i += m.end()
                continue
            i += 1
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = " "
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            depth = 1
            j = i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        i += 1
    return "".join(out)


def mask_string_literals(src: str) -> str:
    """把**字符串字面量的内容**换成等长空格（引号留着），偏移不变。

    这一份是回测第一版逼出来的：`posix-signal-name` 不分文本时，
    `observe/watcher.rs` 那句 `tracing::info!("… → SIGUSR1 → 立刻重探")` 被数成一处平台符号 ——
    **那是日志文案**。代码类信号一律在这份文本上找。
    """
    out = list(src)
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == "r" and i + 1 < n and src[i + 1] in '"#':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                end = n if k < 0 else k
                for q in range(j + 1, end):
                    if out[q] != "\n":
                        out[q] = " "
                i = n if k < 0 else k + len(term)
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    if out[j] != "\n":
                        out[j] = " "
                    if j + 1 < n and out[j + 1] != "\n":
                        out[j + 1] = " "
                    j += 2
                    continue
                if src[j] == '"':
                    break
                if out[j] != "\n":
                    out[j] = " "
                j += 1
            i = min(j + 1, n)
            continue
        i += 1
    return "".join(out)


def production_text(src: str) -> str:
    """`guard_core::production_code` 的等价物，**保偏移**版。"""
    buf = list(src)
    for a, b in _test_module_ranges(src):
        buf[a:b] = list(_blank(src[a:b]))
    return _strip_comments_keep_offsets("".join(buf))


# ══════════════════════════════ 平台 cfg 的门 ══════════════════════════════

# 一条 cfg 谓词里出现这些**独立标识符**就算「平台条件」。
PLATFORM_PREDS = (
    "unix",
    "windows",
    "target_os",
    "target_family",
    "target_env",
    "target_vendor",
    "target_arch",
    "target_pointer_width",
)

# 主分支（本 crate 今天的原生平台）。其余平台 cfg 一律按**回退臂**处理。
# 口径与 `platform/fallback_guard.rs::PRIMARY_CFGS` 同源，这里按谓词而不是整串认。
PRIMARY_SHAPES = (
    'target_os = "linux"',
    "unix",
)


def _is_platform_pred(pred: str) -> bool:
    for p in PLATFORM_PREDS:
        if re.search(r"(?<![A-Za-z0-9_])" + re.escape(p) + r"(?![A-Za-z0-9_])", pred):
            return True
    return False


def _match_bracket(text: str, i: int) -> int:
    """text[i] 是 ( [ { 之一，返回配对收尾字符的下标；找不到返回 -1。"""
    pairs = {"(": ")", "[": "]", "{": "}"}
    opens = "([{"
    closes = ")]}"
    stack = [pairs[text[i]]]
    j = i + 1
    n = len(text)
    while j < n and stack:
        c = text[j]
        if c in opens:
            stack.append(pairs[c])
        elif c in closes:
            if c == stack[-1]:
                stack.pop()
            else:
                return -1
        j += 1
    return j - 1 if not stack else -1


@dataclass
class Gate:
    """一处平台 cfg 属性 + 它罩住的那一段。"""

    path: str
    line: int
    attr: str  # `#[cfg(...)]` 原文
    pred: str  # 括号里那一串
    span: tuple[int, int]  # 罩住的字节区间（含属性本身）
    body: tuple[int, int]  # 紧跟的那个 item / 块（不含属性）
    primary: bool


def find_gates(path: str, prod: str) -> list[Gate]:
    """扫出一份源码里全部**平台** cfg 属性，以及各自罩住的那一段。

    罩住的那一段 = 属性之后（跳过后续属性与 `#[..]`）的**那一个 item 或块**：
    先到的是 depth-0 的 `{` ⇒ 罩到它的配对 `}`；先到的是 depth-0 的 `;` ⇒ 罩到那个 `;`。
    这两形覆盖了 `mod x;` · `use ...;` · `let x = ...;` · `fn f() {}` · 裸块 `{ ... }`
    · `if cond { }`（条件在 `{` 之前，一并罩住）。
    """
    out: list[Gate] = []
    for m in re.finditer(r"#\s*\[\s*cfg\s*\(", prod):
        lb = prod.find("[", m.start())
        rb = _match_bracket(prod, lb)
        if rb < 0:
            continue
        attr = prod[m.start() : rb + 1]
        lp = prod.find("(", lb)
        rp = _match_bracket(prod, lp)
        if rp < 0:
            continue
        pred = prod[lp + 1 : rp]
        if not _is_platform_pred(pred):
            continue
        if _cfg_is_test_only(pred):
            # `#[cfg(all(test, target_os = "linux"))]` —— 测试期专属，不进生产人群。
            continue
        # 跳过紧跟着的其它属性
        k = rb + 1
        while True:
            while k < len(prod) and prod[k] in " \t\r\n":
                k += 1
            if k < len(prod) and prod[k] == "#":
                nb = prod.find("[", k)
                ne = _match_bracket(prod, nb) if nb >= 0 else -1
                if ne < 0:
                    break
                k = ne + 1
                continue
            break
        body_start = k
        depth = 0
        end = -1
        j = k
        while j < len(prod):
            c = prod[j]
            if c in "([":
                e = _match_bracket(prod, j)
                if e < 0:
                    break
                j = e + 1
                continue
            if c == "{":
                e = _match_bracket(prod, j)
                if e < 0:
                    break
                end = e
                break
            if c == ";":
                end = j
                break
            j += 1
        if end < 0:
            end = min(len(prod) - 1, body_start)
        primary = any(
            re.search(r"(?<![A-Za-z0-9_])" + re.escape(s.split(" =")[0]) + r"\b", pred)
            and (s in pred.replace('"', '"'))
            for s in PRIMARY_SHAPES
        )
        # 更直接的判法：谓词整串**不含 not(** 且含主分支形状 ⇒ 主分支臂。
        norm = re.sub(r"\s+", "", pred)
        primary = norm in ('target_os="linux"', "unix", "all(unix)", "target_family=\"unix\"")
        out.append(
            Gate(
                path=path,
                line=prod[: m.start()].count("\n") + 1,
                attr=re.sub(r"\s+", " ", attr),
                pred=norm,
                span=(m.start(), end + 1),
                body=(body_start, end + 1),
                primary=primary,
            )
        )
    return out


# ══════════════════════════════ A 族：平台原语信号表 ══════════════════════════════
#
# 每一条都带**为什么它是平台的**。分母 = 这张表本身（`--json` 里逐条印出来）。

#
# 第三栏 `where`：**这条信号该在哪份文本上找**。这一栏是回测第一版逼出来的 ——
# `posix-signal-name` 不分文本时命中了 `watcher.rs:227` 的一句 `tracing::info!("… → SIGUSR1 → …")`，
# 那是**日志文案**，不是平台符号。⇒ 代码类信号一律在**抹掉字符串内容**的文本上找；
# 只有 `posix-abs-path` 这一条本来就是在找字面量，它走原文。
# 每条信号第三栏 `tier`：
#   **A1 编不过** —— 换个 target 名字解析就过不了（`cargo check --target` 当场红）。
#                    `platform/mod.rs` 头注点名的 `pidfd_open` 属这一档，本件的正题也是它。
#   **A2 跑不对** —— 编得过，但那条路在别的平台上不存在（写死的 POSIX 路径、POSIX shell）。
# 两档**不合成一条**：合成之后「A1 清零」这件事就报不出来了，而那才是「搬得动搬不动」的分界。
#
# 第二栏 `where`：**这条信号该在哪份文本上找**。这一栏是回测第一版逼出来的 ——
# `posix-signal-name` 不分文本时命中了 `watcher.rs:227` 的一句 `tracing::info!("… → SIGUSR1 → …")`，
# 那是**日志文案**，不是平台符号。⇒ 代码类信号一律在**抹掉字符串内容**的文本上找；
# 字面量类信号走原文。
SIGNALS: list[tuple[str, str, str, str, str]] = [
    # (名字, where, tier, 正则, 为什么算平台)
    (
        "std-os-unix",
        "code",
        "A1",
        r"std\s*::\s*os\s*::\s*unix",
        "`std::os::unix` 整个模块在 Windows target 上不存在 ⇒ 名字解析就过不了",
    ),
    (
        "std-os-windows",
        "code",
        "A1",
        r"std\s*::\s*os\s*::\s*windows",
        "`std::os::windows` 在 unix target 上不存在",
    ),
    (
        "std-os-linux",
        "code",
        "A1",
        r"std\s*::\s*os\s*::\s*linux",
        "`std::os::linux` 只在 Linux 上存在",
    ),
    (
        "std-os-fd",
        "code",
        "A1",
        r"std\s*::\s*os\s*::\s*fd",
        "`std::os::fd` 只在 unix/wasi 上存在（Windows 是 `os::windows::io`）",
    ),
    (
        "unix-ext-trait",
        "code",
        "A1",
        r"(?<![A-Za-z0-9_])(PermissionsExt|OpenOptionsExt|MetadataExt|FileTypeExt|"
        r"ExitStatusExt|DirEntryExt|OsStrExt|OsStringExt|CommandExt|"
        r"AsRawFd|FromRawFd|IntoRawFd|RawFd|OwnedFd|BorrowedFd)(?![A-Za-z0-9_])",
        "平台扩展 trait 的名字 —— 换 target 后这个名字不存在（或语义不同）",
    ),
    (
        "libc",
        "code",
        "A1",
        r"(?<![A-Za-z0-9_])libc\s*::",
        "`libc` 的符号绝大多数是 POSIX，Windows 上没有",
    ),
    (
        "win-crate",
        "code",
        "A1",
        r"(?<![A-Za-z0-9_])(windows_sys|winapi)\s*::",
        "Windows 专属 crate",
    ),
    (
        "tokio-signal-unix",
        "code",
        "A1",
        r"tokio\s*::\s*signal\s*::\s*unix",
        "`tokio::signal::unix` 是 unix-only 子模块",
    ),
    (
        "posix-signal-name",
        "code",
        "A1",
        r"(?<![A-Za-z0-9_])(SIGUSR1|SIGUSR2|SIGTERM|SIGKILL|SIGHUP|SIGCHLD|"
        r"POLLIN|POLLOUT|pollfd|pid_t|SYS_[A-Za-z0-9_]+|O_CLOEXEC|O_EXCL|EPERM|ESRCH|E2BIG)"
        r"(?![A-Za-z0-9_])",
        "POSIX 信号 / 系统调用 / errno 常量名",
    ),
    (
        "mode-bits",
        "code",
        "A1",
        r"\.\s*mode\s*\(\s*0o[0-7]{3,4}\s*\)",
        "unix 权限位 —— `OpenOptions::mode` / `set_mode` 只在 unix 扩展上有",
    ),
    (
        "posix-abs-path",
        "literal",
        "A2",
        r'"(/proc/|/dev/|/etc/|/var/|/usr/|/bin/|/sbin/|/tmp/|/proc"|/tmp")',
        "写死的 POSIX 绝对路径 —— Windows 上这条路不存在（编得过、跑不对）",
    ),
    (
        "posix-shell",
        "literal",
        "A2",
        r'Command\s*::\s*new\s*\(\s*"(sh|bash|zsh|/bin/sh|/bin/bash)"',
        "起 POSIX shell —— Windows 上没有 `sh`（编得过、跑不对）",
    ),
]

# ══════════════════════════ 三堆的分堆表（本件人工裁的那一份） ══════════════════════════
#
# 🔴 **这不是白名单** —— 表里每一条都要说清它**是哪一堆**，而不是「免检」：
#   `LEAK`   = **真漏**：该进 `platform/`，今天没进。表里写「归谁 / 什么时候搬」。
#   `LEGIT`  = **合法线外**：它根本不是平台代码（逐条给理由）。
# 一处无门命中**没有**对应条目 ⇒ 印成「未分类」，那是要人来裁的，不许默认放过。
#
# 形状 = (文件相对路径, 行号, 信号名, 堆, 理由)
ROSTER: list[tuple[str, int, str, str, str]] = [
    (
        "observe/watcher.rs",
        655,
        "posix-abs-path",
        "LEAK",
        "`PathBuf::from(\"/tmp\")` 是 tmux socket 根的兜底 —— 写死的 POSIX 路径，"
        "该和 `platform/paths.rs` 住一起。**本件不改**（`observe/` 不在写区），走上报口",
    ),
    (
        "observe/watcher.rs",
        469,
        "posix-shell",
        "LEAK",
        "`Command::new(\"sh\")` —— Windows 上没有 `sh`。**本件不改**（`observe/` 不在写区），走上报口",
    ),
    (
        "observe/watcher.rs",
        505,
        "posix-shell",
        "LEAK",
        "同上，第二处。**本件不改**，走上报口",
    ),
    (
        "agents/fake/mod.rs",
        469,
        "posix-abs-path",
        "LEGIT",
        "`cmdline(\"/usr/bin/vim\")` 是喂给判定器的**反例数据**（断言它**不**被判成 agent），"
        "不是本机要去走的路 ⇒ 它在任何平台上行为相同，不是平台代码",
    ),
]

# `K-R52` 本件**已修**的那两处（量于 d231e50 时它们在「真漏」堆里，修完掉出人群）。
# 留在这里是为了让下一个人复得出「修之前是什么样」——**不是**分堆表的一部分。
FIXED_BY_K_R52: list[tuple[str, int, str, str]] = [
    (
        "sidecars/codepicture/acquire.rs",
        392,
        "std-os-unix / unix-ext-trait / mode-bits（3 处命中、2 行）",
        "`land()` 的 unix 原语无门 ⇒ Windows 上名字解析过不了。09-11 加 `#[cfg(unix)]` 两臂，"
        "非 unix 那臂**不写盘、出声**（不假装设过执行位）",
    ),
    (
        "plugin/discover.rs",
        41,
        "B 族：块体里出现裸 `true`",
        "`is_executable` 非 unix 支什么都算可执行 ⇒ 09-11 改成保守方向 `false`",
    ),
]


def roster_of(path: str, line: int, signal: str) -> tuple[str, str] | None:
    """查分堆表。返回 (堆, 理由)；查不到返回 None ⇒ 印成「未分类」。"""
    for p_, l_, s_, pile, why in ROSTER:
        if p_ == path and l_ == line and s_ == signal:
            return pile, why
    return None


@dataclass
class HitA:
    path: str
    line: int
    signal: str
    text: str
    tier: str = "A1"
    gated_by: str | None = None  # 罩它的那道门（None = 无门）
    file_gated_by: str | None = None
    pile: str = "未分类"
    why: str = ""


@dataclass
class HitB:
    path: str
    line: int
    attr: str
    why: str
    excerpt: str


@dataclass
class Census:
    files: list[str] = field(default_factory=list)
    a_ungated: list[HitA] = field(default_factory=list)
    a_gated: list[HitA] = field(default_factory=list)
    b_red: list[HitB] = field(default_factory=list)
    b_population: int = 0
    gates: list[Gate] = field(default_factory=list)
    undecidable: list[tuple[str, str]] = field(default_factory=list)


def rs_files(root: str) -> list[str]:
    out = []
    for d, _, fs in os.walk(root):
        for f in fs:
            if f.endswith(".rs"):
                out.append(os.path.join(d, f))
    return sorted(out)


def file_level_gates(files: list[str], prods: dict[str, str]) -> dict[str, str]:
    """`#[cfg(平台)] mod x;` ⇒ `x.rs` / `x/**` 整份文件都在门后。返回 相对路径 -> 门。"""
    gated: dict[str, str] = {}
    for p in files:
        prod = prods[p]
        for g in find_gates(p, prod):
            body = prod[g.body[0] : g.body[1]]
            m = re.match(r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;", body.strip())
            if not m:
                continue
            name = m.group(1)
            d = os.path.dirname(p)
            for cand in (
                os.path.join(d, name + ".rs"),
                os.path.join(d, name, "mod.rs"),
            ):
                if os.path.exists(cand):
                    gated[os.path.relpath(cand, SRC)] = f"{os.path.relpath(p, SRC)}:{g.line} {g.attr}"
            sub = os.path.join(d, name)
            if os.path.isdir(sub):
                for q in rs_files(sub):
                    gated.setdefault(
                        os.path.relpath(q, SRC), f"{os.path.relpath(p, SRC)}:{g.line} {g.attr}"
                    )
    return gated


BARE_TRUE = re.compile(r"(?<![A-Za-z0-9_.])true(?![A-Za-z0-9_])")


def b_family_check(prod: str, g: Gate) -> str | None:
    """`fallback_guard` 的两条判红条件，原样搬来。返回红的理由，或 None。"""
    body = prod[g.body[0] : g.body[1]]
    if BARE_TRUE.search(body):
        return "块体里出现裸 `true`"
    # 块的最后一个表达式
    inner = body.strip()
    if inner.startswith("{") and inner.endswith("}"):
        inner = inner[1:-1]
    last = [ln.strip() for ln in inner.splitlines() if ln.strip()]
    if last:
        tail = last[-1].rstrip(",;")
        if tail.startswith("Some(") or tail.startswith("Ok("):
            return "块的最后一个表达式以 `Some(` / `Ok(` 打头"
    return None


def run_census() -> Census:
    c = Census()
    files = rs_files(SRC)
    prods = {}
    for p in files:
        raw = open(p, encoding="utf-8").read()
        prods[p] = production_text(raw)
        # 反向自检：剥完不许再出现测试属性
        attr = "#[" + "test]"
        if attr in prods[p]:
            c.undecidable.append((os.path.relpath(p, SRC), f"剥完仍残留 {attr} ⇒ 这一份的剥法坏了"))
    c.files = [os.path.relpath(p, SRC) for p in files]
    fgates = file_level_gates(files, prods)

    for p in files:
        rel = os.path.relpath(p, SRC)
        prod = prods[p]
        gates = find_gates(p, prod)
        c.gates.extend(gates)
        # B 族：回退臂（非主分支）的人群 = 全树
        for g in gates:
            if g.primary:
                continue
            body = prod[g.body[0] : g.body[1]]
            if re.match(r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z0-9_]+\s*;", body):
                continue  # `mod x;` 没有块体，判不了（整文件绕过 —— 见下面的 undecidable）
            c.b_population += 1
            why = b_family_check(prod, g)
            if why:
                c.b_red.append(
                    HitB(
                        path=rel,
                        line=g.line,
                        attr=g.attr,
                        why=why,
                        excerpt=re.sub(r"\s+", " ", body)[:120],
                    )
                )
        # A 族：平台原语有没有门
        code = mask_string_literals(prod)
        raw_lines = open(p, encoding="utf-8").read().splitlines()
        for name, where, tier, pat, _why in SIGNALS:
            hay = code if where == "code" else prod
            for m in re.finditer(pat, hay):
                inside = next(
                    (
                        f"{rel}:{g.line} {g.attr}"
                        for g in gates
                        if g.span[0] <= m.start() < g.span[1]
                    ),
                    None,
                )
                line = hay[: m.start()].count("\n") + 1
                raw_line = raw_lines[line - 1].strip()
                h = HitA(
                    path=rel,
                    line=line,
                    signal=name,
                    text=raw_line[:110],
                    tier=tier,
                    gated_by=inside,
                    file_gated_by=fgates.get(rel),
                )
                if inside or fgates.get(rel):
                    h.pile = "GATED"
                    h.why = "有门 ⇒ 合法线外（门的住址在 gated_by / file_gated_by）"
                    c.a_gated.append(h)
                else:
                    r = roster_of(rel, line, name)
                    if r:
                        h.pile, h.why = r
                    c.a_ungated.append(h)
    # 分堆表不许烂掉：一条登记了却再也匹配不上的，要点名（不然它慢慢变成免检名单）
    for p_, l_, s_, pile, _why in ROSTER:
        if not any(h.path == p_ and h.line == l_ and h.signal == s_ for h in c.a_ungated):
            c.undecidable.append(
                (f"{p_}:{l_} [{s_}]", f"分堆表里登记着（{pile}），而今天树上**匹配不上** ⇒ 过期条目，该删")
            )
    # `#[cfg(平台)] mod x;` 那一族：整文件绕过 B 族判据 —— 如实登记成「判不了」
    for rel, by in sorted(fgates.items()):
        c.undecidable.append(
            (rel, f"整份文件由 `{by}` 选进来 ⇒ 文件体内一个平台 cfg 都没有，B 族对它贡献 0 个受检块")
        )
    return c


def git_sha() -> str:
    try:
        return subprocess.run(
            ["git", "-C", TREE, "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except Exception:
        return "<判不了：git 不可用>"


BACKTEST = [
    ("A", "sidecars/codepicture/acquire.rs", "std-os-unix"),
    ("B", "plugin/discover.rs", "块体里出现裸 `true`"),
]


def backtest(c: Census) -> list[tuple[str, str, bool, str]]:
    out = []
    a = [h for h in c.a_ungated if h.path == BACKTEST[0][1] and h.signal == BACKTEST[0][2]]
    out.append(
        (
            "A 族样本",
            f"{BACKTEST[0][1]} 的 `use std::os::unix::fs::OpenOptionsExt;`（无门）",
            bool(a),
            "；".join(f"{h.path}:{h.line} {h.text}" for h in a) or "没逮到",
        )
    )
    b = [h for h in c.b_red if h.path == BACKTEST[1][1]]
    out.append(
        (
            "B 族样本",
            "plugin/discover.rs::is_executable 非 unix 支的裸 `true`",
            bool(b),
            "；".join(f"{h.path}:{h.line} {h.attr} ⇒ {h.why}" for h in b) or "没逮到",
        )
    )
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--backtest", action="store_true")
    ap.add_argument("--sha", action="store_true")
    ap.add_argument(
        "--at",
        help="这棵树是哪个提交的检出（`--tree` 指到 `git archive` 的检出时 git 读不到，"
        "用它把出处写进读数；不给就印「判不了」，**不许编一个**）。",
    )
    ap.add_argument(
        "--tree",
        help="被测树的 `remote-daemon-proto` 目录（默认 = 本文件旁边那棵）。回测基线用它。",
    )
    args = ap.parse_args()
    if args.tree:
        globals()["TREE"] = os.path.abspath(args.tree)
        globals()["SRC"] = os.path.join(globals()["TREE"], "src")

    c = run_census()
    sha = args.at or git_sha()
    if args.sha:
        print(sha)
        return 0

    bt = backtest(c)
    if args.backtest:
        print(f"== 回测（被测树 {SRC}，量于 {sha}）==")
        ok = True
        for name, what, hit, detail in bt:
            print(f"  {'逮到' if hit else '没逮到'}  {name}：{what}")
            print(f"          {detail}")
            ok &= hit
        print("== 尺子" + ("有牙" if ok else "坏了 —— 重造，别将就") + " ==")
        return 0 if ok else 1

    if args.json:
        print(
            json.dumps(
                {
                    "sha": sha,
                    "tree": SRC,
                    "signals": [{"name": n, "where": wh, "tier": t, "re": p, "why": w} for n, wh, t, p, w in SIGNALS],
                    "files": len(c.files),
                    "a_ungated": [h.__dict__ for h in c.a_ungated],
                    "a_gated": len(c.a_gated),
                    "b_population": c.b_population,
                    "b_red": [h.__dict__ for h in c.b_red],
                    "undecidable": c.undecidable,
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    print(f"== K-R52 D1 平台线普查 ==")
    print(f"被测树：{SRC}")
    print(f"量于：{sha}")
    print(f"分母：{len(c.files)} 个 `.rs`（`src/` 递归全部），每份剥掉测试段与注释后再数")
    print(f"平台 cfg 属性（生产段、非测试期）：{len(c.gates)} 处，其中回退臂受检块 {c.b_population} 个")
    print()
    leaks = [h for h in c.a_ungated if h.pile == "LEAK"]
    legit = [h for h in c.a_ungated if h.pile == "LEGIT"]
    unclassified = [h for h in c.a_ungated if h.pile == "未分类"]
    a1 = [h for h in leaks if h.tier == "A1"]
    a2 = [h for h in leaks if h.tier == "A2"]
    print(
        f"— A 族命中 {len(c.a_ungated) + len(c.a_gated)} 处："
        f"**有门** {len(c.a_gated)} · **无门** {len(c.a_ungated)} —"
    )
    print()
    print(f"【堆一 · 真漏】{len(leaks)} 处（A1 编不过 {len(a1)} · A2 跑不对 {len(a2)}）")
    for h in sorted(leaks, key=lambda x: (x.tier, x.path, x.line)):
        print(f"  [{h.tier}] {h.path}:{h.line}  [{h.signal}]  {h.text}")
        print(f"        {h.why}")
    print()
    print(f"【堆二 · 合法线外】有门 {len(c.a_gated)} 处 + 分堆表判 LEGIT {len(legit)} 处")
    print("  · 有门那 %d 处逐条住址走 `--json` 的 a_gated（门的住址在每条的 gated_by）" % len(c.a_gated))
    for h in sorted(legit, key=lambda x: (x.path, x.line)):
        print(f"  · {h.path}:{h.line}  [{h.signal}]  {h.why}")
    print()
    if unclassified:
        print(f"🔴【未分类】{len(unclassified)} 处 —— 分堆表里没有它，要人来裁，**不许默认放过**")
        for h in sorted(unclassified, key=lambda x: (x.path, x.line)):
            print(f"  {h.path}:{h.line}  [{h.signal}]  {h.text}")
        print()
    print(f"— B 族：回退臂编造乐观值 {len(c.b_red)} 处（分母 = 受检块 {c.b_population} 个） —")
    for h in sorted(c.b_red, key=lambda x: (x.path, x.line)):
        print(f"  {h.path}:{h.line}  {h.attr}  ⇒ {h.why}")
        print(f"      {h.excerpt}")
    print()
    print(f"【堆三 · 判不了】{len(c.undecidable)} 条")
    for rel, why in c.undecidable:
        print(f"  {rel}：{why}")
    print()
    print("— 回测 —")
    for name, what, hit, detail in bt:
        print(f"  {'逮到' if hit else '没逮到'}  {name}：{what}  ⇒ {detail}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
