#!/usr/bin/env python3
# ruff: noqa
"""K-W1B · D1 量具：把「agent 解耦」的人群一次切清 —— 五把尺子，每把带满五样。

住址（唯一）：`evidence/K-W1B-D1-agent-coupling-census.py`（名字带件号 K-W1B，本件独占）。
被测对象：**本文件所在的那棵工作树**（`Path(__file__).resolve().parents[1]`）——
不写死路径、不指别的树。⇒ 谁在哪棵树上跑它，量的就是那棵树，不会出现
「同一住址下先后住过两份被测对象不同的量具」那种静默假读数（brief 12·5k）。

跑法：
    python3 evidence/K-W1B-D1-agent-coupling-census.py            # 五把尺子的表
    python3 evidence/K-W1B-D1-agent-coupling-census.py --calibrate # 只跑标定（见下）
    python3 evidence/K-W1B-D1-agent-coupling-census.py --kp2       # KP2 那一问的证据面
    python3 evidence/K-W1B-D1-agent-coupling-census.py --json      # 机读

# 🔴 这份量具自己的标定（不标定的尺子不许报数）

尺子①**不是**本量具的产物，它是 daemon 侧 `agent_locality_guard.rs` 里
`general_layer_adapter_call_sites_are_enumerated_one_by_one` 那条判据在数的东西，
而那条判据**逐文件相等**、每趟 cargo 都在跑。⇒ 本量具把 `ADAPTER_CALL_SITES`
那张登记表从 Rust 源码里解析出来，与自己量的逐文件读数**对拍**：
对不上就 `CALIBRATION: FAIL`，整份读数作废。

那就是本量具「生产段剥法」的地板：它剥得对不对，不靠我说，靠一张被 cargo
钉了十天的表。⚠ 反过来的诚实边界：标定只覆盖**尺子①用到的那条路**
（daemon 树 · 6 根路径针 · 行粒度）。尺子②③④⑤ 用的是同一个剥法，
但**它们各自的针没有第二个权威源** —— 那几个数只有本量具一个来源。

# 生产段剥法：`guard_core::production_code` 的逐条移植

顺序与 Rust 侧逐字一致（`src-tauri/crates/guard-core/src/lib.rs::production_code`）：
    strip_block_comments(production_source(src)) → 去整行 `//` → strip_trailing_comments

⚠ **一处刻意的偏差**：Rust 侧那一步「去整行 `//`」是 `.lines().filter(…).join()`
—— 它**删行**，行号从此对不上原文。本量具改成**把那一行抹空**（保留行号），
理由是本量具要报 `文件:行号`（brief 13c 要行号带校验位 ⇒ 得先有行号）。
**这一处偏差不可能改变命中数**：所有针都是非空串，而抹空之后那一行是空串，
空串不含任何非空针 ⇒ 与删掉它等价。同理，被抹空的行对
`strip_trailing_comments` 的跨行字符串状态贡献 0 个引号，与删掉它也等价。
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

TREE = Path(__file__).resolve().parents[1]
DAEMON_SRC = TREE / "remote-daemon-proto" / "src"
MONITOR_SRC = TREE / "src-tauri" / "src"

# ── guard_core 的三个原语，逐条移植 ────────────────────────────────────────


def mask_char_literals(line: str) -> str:
    """把字符字面量换成等长 `_`（`guard_core::mask_char_literals`）。

    只服务于下面两个状态机：一个 `'"'` 会把「在不在字符串里」整段带偏。
    等长替换 ⇒ 下标与原行一一对应。
    """
    b = bytearray(line.encode("utf-8"))
    src = line.encode("utf-8")
    i = 0
    while i < len(src):
        if src[i] == 0x27:  # '
            if i + 3 < len(src) and src[i + 1] == 0x5C:  # backslash
                k = src.find(0x27, i + 3)
                if k != -1:
                    for j in range(i, k + 1):
                        b[j] = 0x5F
                    i = k + 1
                    continue
            # `'c'`：c 可能多字节 ⇒ 整个字符一起替
            rest = line.encode("utf-8")[i + 1 :].decode("utf-8", "ignore")
            if rest:
                c = rest[0]
                cl = len(c.encode("utf-8"))
                if (
                    c not in ("'", "\\")
                    and i + 1 + cl < len(src)
                    and src[i + 1 + cl] == 0x27
                ):
                    for j in range(i, i + 2 + cl):
                        b[j] = 0x5F
                    i = i + 2 + cl
                    continue
        i += 1
    return b.decode("utf-8")


def _raw_string_open(sb: bytes, i: int) -> tuple[int, int] | None:
    """`r"…"` / `r#"…"#` / `b"…"` / `br#"…"#` 的开头在不在 sb[i]。"""

    def ident(c: int) -> bool:
        return chr(c).isalnum() and ord(chr(c)) < 128 or c == 0x5F

    if i > 0 and ident(sb[i - 1]):
        return None
    k = i
    if k < len(sb) and sb[k] == 0x62:  # b
        k += 1
        if k < len(sb) and sb[k] == 0x22:  # "
            return (0, k + 1 - i)
    if k >= len(sb) or sb[k] != 0x72:  # r
        return None
    k += 1
    hash_start = k
    while k < len(sb) and sb[k] == 0x23:  # #
        k += 1
    if k >= len(sb) or sb[k] != 0x22:
        return None
    return (k - hash_start, k + 1 - i)


def try_strip_block_comments(src: str) -> str | None:
    """块注释内容抹成等长空格；模型崩了返回 None（`guard_core` 的兜底：一个字都不剥）。"""
    out: list[bytes] = []
    depth = 0
    in_str = False
    raw_hashes: int | None = None
    for li, raw in enumerate(src.split("\n")):
        scan = (
            mask_char_literals(raw)
            if (depth == 0 and not in_str and raw_hashes is None)
            else raw
        )
        sb = scan.encode("utf-8")
        line = bytearray(raw.encode("utf-8"))
        # 掩码等长 ⇒ 下标一一对应；抹在原行上
        i = 0
        while i < len(sb):
            if depth > 0:
                if sb[i] == 0x2F and i + 1 < len(sb) and sb[i + 1] == 0x2A:  # /*
                    depth += 1
                    line[i] = 0x20
                    line[i + 1] = 0x20
                    i += 2
                    continue
                if sb[i] == 0x2A and i + 1 < len(sb) and sb[i + 1] == 0x2F:  # */
                    depth -= 1
                    line[i] = 0x20
                    line[i + 1] = 0x20
                    i += 2
                    continue
                line[i] = 0x20
                i += 1
                continue
            if raw_hashes is not None:
                h = raw_hashes
                if (
                    sb[i] == 0x22
                    and len(sb) >= i + 1 + h
                    and all(c == 0x23 for c in sb[i + 1 : i + 1 + h])
                ):
                    raw_hashes = None
                    i += 1 + h
                    continue
                i += 1
                continue
            if in_str:
                if sb[i] == 0x5C:
                    i += 2
                    continue
                if sb[i] == 0x22:
                    in_str = False
                i += 1
                continue
            # ── 码状态 ──
            ro = _raw_string_open(sb, i)
            if ro is not None:
                h, consumed = ro
                if h == 0 and sb[i] == 0x62:
                    in_str = True
                else:
                    raw_hashes = h
                i += consumed
                continue
            if sb[i] == 0x22:
                in_str = True
                i += 1
                continue
            if sb[i] == 0x2F and i + 1 < len(sb) and sb[i + 1] == 0x2F:  # //
                break
            if sb[i] == 0x2F and i + 1 < len(sb) and sb[i + 1] == 0x2A:  # /*
                depth += 1
                line[i] = 0x20
                line[i + 1] = 0x20
                i += 2
                continue
            i += 1
        out.append(bytes(line))
    if depth != 0 or in_str or raw_hashes is not None:
        return None
    return b"\n".join(out).decode("utf-8")


def strip_block_comments(src: str) -> str:
    got = try_strip_block_comments(src)
    return src if got is None else got


_CFG_TEST = re.compile(r"^#\[cfg\((.*)\)\]$")


def _cfg_is_test_only(attr: str) -> bool:
    """`#[cfg(test)]` 这一形（`guard_core::cfg_is_test_only` 的射程：单 `test` 谓词）。"""
    m = _CFG_TEST.match(attr.strip())
    if not m:
        return False
    return m.group(1).strip() == "test"


def test_module_ranges(src: str) -> list[tuple[int, int]]:
    """每个「带花括号体的 `#[cfg(test)] mod X { … }`」的字节区间（按字符下标）。"""
    open_pat = "\n#[cfg("
    close_pat = "\n}"
    out: list[tuple[int, int]] = []
    i = 0
    while True:
        rel = src.find(open_pat, i)
        if rel == -1:
            return out
        j = rel
        attr_start = j + 1
        nl = src.find("\n", attr_start)
        attr_end = len(src) if nl == -1 else nl
        mod_start = min(attr_end + 1, len(src))
        nl2 = src.find("\n", mod_start)
        mod_end = len(src) if nl2 == -1 else nl2
        mod_line = src[mod_start:mod_end].strip()
        is_test_mod = (
            _cfg_is_test_only(src[attr_start:attr_end])
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        rel_end = src.find(close_pat, j)
        if rel_end == -1:
            out.append((j, len(src)))
            return out
        end = rel_end + len(close_pat)
        out.append((j, end))
        i = end


def production_source(src: str, keep_lines: bool = True) -> str:
    """剥掉 `#[cfg(test)] mod X {…}`。keep_lines ⇒ 换成等量空行（保住行号）。"""
    out: list[str] = []
    i = 0
    for start, end in test_module_ranges(src):
        out.append(src[i:start])
        if keep_lines:
            out.append("\n" * src[start:end].count("\n"))
        i = end
    out.append(src[i:])
    return "".join(out)


def strip_trailing_comments(src: str) -> str:
    """剥行尾 `//`（`guard_core::strip_trailing_comments`）。行数不变。"""
    out: list[str] = []
    in_str = False
    for raw in src.split("\n"):
        masked = mask_char_literals(raw)
        if 'r"' in masked or "r#" in masked or 'b"' in masked:
            out.append(raw)
            in_str = False
            continue
        mb = masked.encode("utf-8")
        if in_str:
            i = 0
            while i < len(mb):
                if mb[i] == 0x5C:
                    i += 2
                    continue
                if mb[i] == 0x22:
                    in_str = False
                    break
                i += 1
            out.append(raw)
            continue
        i = 0
        cut: int | None = None
        while i < len(mb):
            if in_str:
                if mb[i] == 0x5C:
                    i += 2
                    continue
                if mb[i] == 0x22:
                    in_str = False
                i += 1
                continue
            if mb[i] == 0x22:
                in_str = True
                i += 1
                continue
            if mb[i] == 0x2F and i + 1 < len(mb) and mb[i + 1] == 0x2F:
                cut = i
                break
            i += 1
        if cut is None:
            out.append(raw)
        else:
            out.append(raw.encode("utf-8")[:cut].decode("utf-8", "ignore"))
    return "\n".join(out)


def production_code(src: str) -> str:
    """`guard_core::production_code` 的移植。整行 `//` **抹空**而不是删行（见模块头注）。"""
    no_block = strip_block_comments(production_source(src))
    kept = "\n".join(
        "" if line.lstrip().startswith("//") else line for line in no_block.split("\n")
    )
    return strip_trailing_comments(kept)


# ── 语料 ───────────────────────────────────────────────────────────────────


def rs_files(root: Path) -> list[tuple[str, str]]:
    """(相对 root 的路径, 原文)，已排序。**不摘除任何文件** —— 摘除是各把尺子自己的事。"""
    out = []
    for p in sorted(root.rglob("*.rs")):
        rel = str(p.relative_to(root)).replace("\\", "/")
        out.append((rel, p.read_text(encoding="utf-8")))
    return out


def hits_by_file(
    files: list[tuple[str, str]],
    needles: list[str],
    prod: bool = True,
) -> dict[str, list[tuple[int, str]]]:
    """逐文件的命中：{相对路径: [(行号, 该行逐字), …]}。**行粒度**（一行两次算一次）。"""
    out: dict[str, list[tuple[int, str]]] = {}
    for rel, src in files:
        text = production_code(src) if prod else src
        rows = []
        for n, line in enumerate(text.split("\n"), start=1):
            if any(nd in line for nd in needles):
                rows.append((n, line.strip()))
        if rows:
            out[rel] = rows
    return out


# ── 尺子① daemon 的那个 27（标定用） ──────────────────────────────────────

# 由 HOMES 派生，逐字照 `agent_locality_guard::adapter_path_needles()`：
# 全路径 `agents::<名>::` + 相对写法 `<名>::`。**运行时拼**，免得命中本文件的散文。
DAEMON_HOMES = ["agents/codex/", "agents/claudecode/", "agents/fake/"]
# `scan_tree!` 按构造摘掉调用者自己 ⇒ daemon 那条判据的人群里没有它自己那份。
DAEMON_SELF = "agent_locality_guard.rs"
DAEMON_REGISTRY_FILE = "agents/mod.rs"


def daemon_needles() -> list[str]:
    out = []
    for h in DAEMON_HOMES:
        rel = h.rstrip("/")
        out.append(rel.replace("/", "::") + "::")
        out.append(rel.rsplit("/", 1)[-1] + "::")
    return out


def ruler1() -> dict:
    files = rs_files(DAEMON_SRC)
    total_rs = len(files)
    # 人群：整棵树 − 三个家 − 调用者自己（`scan_tree!` 的摘除）
    scanned = [
        (rel, src)
        for rel, src in files
        if not any(rel.startswith(h) for h in DAEMON_HOMES) and rel != DAEMON_SELF
    ]
    got = hits_by_file(scanned, daemon_needles(), prod=True)
    # 判据④再把注册表文件扣出人群（对价 = 判据⑦把它钉死成 REGISTRY.len()）
    minus_registry = {k: v for k, v in got.items() if k != DAEMON_REGISTRY_FILE}
    return {
        "tree": "remote-daemon-proto/src/**/*.rs",
        "denominator": {
            "该树 .rs 总数": total_rs,
            "三个家的 .rs": sum(
                1 for rel, _ in files if any(rel.startswith(h) for h in DAEMON_HOMES)
            ),
            "调用者自己(scan_tree! 摘除)": 1,
            "进针扫的": len(scanned),
            "判据④再扣注册表文件": 1,
            "判据④的人群": len(scanned) - 1,
        },
        "granularity": "行（一行里出现两次只算一次）",
        "needles": daemon_needles(),
        "segment": "production_code（剥 #[cfg(test)] mod / 块注释 / 整行 // / 行尾 //）",
        "by_file": {k: len(v) for k, v in sorted(minus_registry.items())},
        "total": sum(len(v) for v in minus_registry.values()),
        "registry_file_hits": len(got.get(DAEMON_REGISTRY_FILE, [])),
        "rows": {k: v for k, v in sorted(minus_registry.items())},
    }


_REG_ROW = re.compile(r'^\s*\(\s*"([^"]+)"\s*,\s*(\d+)\s*,', re.M)


def registered_table() -> dict[str, int]:
    """从 daemon 侧 `agent_locality_guard.rs` 里把 `ADAPTER_CALL_SITES` 解析出来。

    ⚠ 这不是「复述那张表」（brief 13b）—— 是**现读**那一份唯一住址的字面量，
    本量具里一个成员都不写死。表的形状换了（比如加了列）解析会返回空 ⇒ 标定 FAIL，
    而不是静默拿一份陈旧的副本对拍。
    """
    src = (DAEMON_SRC / DAEMON_SELF).read_text(encoding="utf-8")
    start = src.find("const ADAPTER_CALL_SITES")
    if start == -1:
        return {}
    end = src.find("];", start)
    if end == -1:
        return {}
    body = src[start:end]
    out: dict[str, int] = {}
    # 多行形：`("control/fork_write.rs", 3, "…")` 与 `(\n "…",\n 6,\n "…",\n)` 两种排版都要认
    for m in re.finditer(r'"([^"]+\.rs)"\s*,\s*(\d+)\s*,', body):
        out[m.group(1)] = int(m.group(2))
    return out


def calibrate() -> tuple[bool, str]:
    r1 = ruler1()
    want = registered_table()
    got = r1["by_file"]
    if not want:
        return False, "解析 ADAPTER_CALL_SITES 得到空表 —— 表的形状变了，标定不成立"
    if got != want:
        return False, f"逐文件对不上。\n本量具实得：{got}\n登记表现读：{want}"
    return True, f"逐文件逐字相等（{len(want)} 文件 / {sum(want.values())} 处）"


# ── 尺子②③④⑤ 桌面侧 ─────────────────────────────────────────────────────

MONITOR_ADAPTER = "adapter.rs"


def ruler2a() -> dict:
    """② 桌面 `adapter::active()` 生产处数 —— **带路径限定**的针。"""
    files = rs_files(MONITOR_SRC)
    needle = "::" + "active("
    got = hits_by_file(files, [needle], prod=True)
    return {
        "tree": "src-tauri/src/**/*.rs",
        "denominator": {"该树 .rs 总数": len(files), "扣除": 0, "人群": len(files)},
        "granularity": "行",
        "needles": [needle],
        "segment": "production_code",
        "by_file": {k: len(v) for k, v in sorted(got.items())},
        "total": sum(len(v) for v in got.values()),
        "rows": {k: v for k, v in sorted(got.items())},
    }


def ruler2b() -> dict:
    """②b `adapter.rs` **自己身上**的裸 `active()`（定义行不算）。"""
    src = (MONITOR_SRC / MONITOR_ADAPTER).read_text(encoding="utf-8")
    text = production_code(src)
    define = "fn " + "active()"
    rows = []
    for n, line in enumerate(text.split("\n"), start=1):
        if "active()" not in line:
            continue
        if define in line:  # 定义行本身不是调用点
            continue
        if "::" + "active(" in line:  # 那半归 ②a，别双计
            continue
        rows.append((n, line.strip()))
    return {
        "tree": "src-tauri/src/adapter.rs（**单文件**）",
        "denominator": {"人群": 1, "口径": "适配层自己那一份文件"},
        "granularity": "行",
        "needles": ["active()（裸，扣掉 `fn active()` 定义行与 `::active(` 那半）"],
        "segment": "production_code",
        "by_file": {MONITOR_ADAPTER: len(rows)},
        "total": len(rows),
        "rows": {MONITOR_ADAPTER: rows},
    }


_KIND_LITERAL = "AgentKind::"


def ruler3() -> dict:
    """③ 桌面 `adapter::for_kind` **真分派**：传运行时 kind（不是字面量）的生产调用点。"""
    files = rs_files(MONITOR_SRC)
    call = "for_kind("
    parse = "parse_" + call
    define = "fn " + call
    rows_by_file: dict[str, list[tuple[int, str]]] = {}
    for rel, src in files:
        text = production_code(src)
        rows = []
        for n, line in enumerate(text.split("\n"), start=1):
            s = line.strip()
            if call not in s:
                continue
            if parse in s:  # parser 的另一个派发器，不是适配层
                continue
            if define in s:  # 定义行
                continue
            # 只要这一行里 `for_kind(` 后面跟的**不是** `AgentKind::` 字面量
            runtime = False
            for m in re.finditer(re.escape(call), s):
                if s[: m.start()].endswith("parse_"):
                    continue
                arg = s[m.end() :]
                if not arg.startswith(_KIND_LITERAL):
                    runtime = True
            if runtime:
                rows.append((n, s))
        if rows:
            rows_by_file[rel] = rows
    return {
        "tree": "src-tauri/src/**/*.rs",
        "denominator": {
            "该树 .rs 总数": len(files),
            "扣除": "`parse_for_kind(` · `fn for_kind(` 定义行 · 实参是 `AgentKind::` 字面量的那些",
            "人群": len(files),
        },
        "granularity": "行（该行至少有一个 `for_kind(` 的实参不是字面量）",
        "needles": [f"{call} 且实参不以 {_KIND_LITERAL} 打头"],
        "segment": "production_code",
        "by_file": {k: len(v) for k, v in sorted(rows_by_file.items())},
        "total": sum(len(v) for v in rows_by_file.values()),
        "rows": {k: v for k, v in sorted(rows_by_file.items())},
    }


# ★ 尺子④的**已知噪音**，逐条登记 + 写清「它到底是什么」。
#
# ⚠ 这张表的性质与「欠账」相反（照 daemon 侧 `NOT_AGENT_KNOWLEDGE` 的头注）：
# 那种是「将来要清零」；**这张是「尺子看走眼了，永远留着」**。
# 把噪音塞进欠账表的后果是它永远清不掉。
#
# 🔴 **它是现打出来的，不是防御性编程**：本件的 D2 判据落地那一刻，尺子④
# 从 **23 涨到 37**，而 14 行全部来自那个判据文件自己的散文与反向夹具 ——
# **量具数到了为量它而写的那份判据**。这是尺子④「不能用」的**第三条**理由
# （前两条：注释行与定义行进分母 · `parse_for_kind` 那 7 行答非所问）。
# ★ 对照着看：D2 那条判据**免疫**同一个病 —— 它走 `guard_core::scan_tree!`
# （按 `file!()` 构造性摘除自己）＋ `production_code`（剥掉 `#[cfg(test)]` 与注释）。
# 差别不是小心，是**用了本仓为这一族立的那两个原语**。
RULER4_NOISE: dict[str, str] = {
    "agent_boundary_guard.rs": "本件 D2 的判据自己 —— 它的头注、登记表与反向夹具里"
    "逐字写着这个针形（它必须写，那正是它要钉的东西）。走 `scan_tree!` 的判据按构造"
    "读不到自己；本尺子是裸扫，读得到。",
}


def ruler4() -> dict:
    """④ 桌面 `for_kind(` **裸子串**、**不剥生产段** —— 留着它就是为了展示它为什么不能用。"""
    files = rs_files(MONITOR_SRC)
    call = "for_kind("
    parse = "parse_" + call
    define = "fn " + call
    got = hits_by_file(files, [call], prod=False)
    breakdown = {"parse_for_kind": 0, "注释行": 0, "定义行": 0, "剩下的真调用": 0}
    for rows in got.values():
        for _, s in rows:
            if s.lstrip().startswith("//"):
                breakdown["注释行"] += 1
            elif parse in s:
                breakdown["parse_for_kind"] += 1
            elif define in s:
                breakdown["定义行"] += 1
            else:
                breakdown["剩下的真调用"] += 1
    noise = {f: len(rows) for f, rows in got.items() if f in RULER4_NOISE}
    total = sum(len(v) for v in got.values())
    return {
        "tree": "src-tauri/src/**/*.rs",
        "denominator": {"该树 .rs 总数": len(files), "扣除": 0, "人群": len(files)},
        "granularity": "行",
        "needles": [call],
        "segment": "🔴 **原文，一个字不剥** —— 注释行与定义行都在分母里",
        "by_file": {k: len(v) for k, v in sorted(got.items())},
        "total": total,
        "breakdown": breakdown,
        "noise": {"逐文件": noise, "合计": sum(noise.values()), "住址": RULER4_NOISE},
        "total_minus_noise": total - sum(noise.values()),
        "rows": {k: v for k, v in sorted(got.items())},
    }


# ── 尺子⑤：`KP2` 的第二个实例 —— 两棵树之外还有第三棵 ─────────────────────

SHARED_CRATES = "src-tauri/crates"


def agent_name_needles() -> list[str]:
    """agent 的**名字**（不是调用路径）。运行时拼，免得命中本文件的散文。"""
    return [
        "clau" + "de",
        "CLAU" + "DE",
        "Clau" + "de",
        "cod" + "ex",
        "COD" + "EX",
        "Cod" + "ex",
    ]


# ★ 尺子⑤的**已知噪音**：针打中了，但那不是 agent 知识。逐条写清它到底是什么。
#
# 判准一句话：**这个词指的是「那个 agent」，还是指「本仓自己」**。
RULER5_NOISE: dict[str, str] = {
    "creds-core/src/store.rs:97": '`home.join("claudecode-frontend")` 是**本仓自己**的'
    "数据目录名（cc-monitor 的仓名），不是 Claude 的布局。同 `local_read_surface_registry` "
    "头注点破的那一族：`~/.claude`（agent 的数据目录）与 `<cwd>/.claude`（项目自己的配置目录）"
    "被同一根针打中，靠分类分开。**这不是针的缺陷**（放宽命中面是对的），是分类该干的活。",
}


def ruler5() -> dict:
    """⑤ **共享 crate 树**（`src-tauri/crates/**`）里的 agent 名字 —— 前四把尺子一根针都够不着。"""
    root = TREE / SHARED_CRATES
    files = rs_files(root)
    got = hits_by_file(files, agent_name_needles(), prod=True)
    noise = sum(
        1 for f, rows in got.items() for n, _ in rows if f"{f}:{n}" in RULER5_NOISE
    )
    return {
        "tree": f"{SHARED_CRATES}/**/*.rs（**两棵被量的树之外的第三棵**）",
        "denominator": {
            "该树 .rs 总数": len(files),
            "扣除": 0,
            "人群": len(files),
            "口径": "整棵共享 crate 树；vendor 不在其中（`src-tauri/vendor/` 是另一棵）",
        },
        "granularity": "行",
        "needles": agent_name_needles(),
        "segment": "production_code",
        "by_file": {k: len(v) for k, v in sorted(got.items())},
        "total": sum(len(v) for v in got.values()),
        "noise": {"合计": noise, "住址": RULER5_NOISE},
        "total_minus_noise": sum(len(v) for v in got.values()) - noise,
        "rows": {k: v for k, v in sorted(got.items())},
    }


def kp2_evidence() -> dict:
    """`KP2` 那一问的证据面：**尺子外面**有什么。

    第一个实例（`§0b` 已给）：整个桌面侧 `src-tauri/src` 不在尺子①的量程里。
    本函数找的是**第二个**：`src-tauri/crates/**` —— 它**同时**不在
    尺子① 的树里（那是 daemon 树）、也不在尺子②③④ 的树里（那是 `src-tauri/src`），
    而 `src-tauri/Cargo.toml` 把它们当依赖编进同一个二进制。
    ⇒ 「agent 知识住哪儿」这个问题上，它是一片**没有任何一把尺子够得着**的地。
    """
    r5 = ruler5()
    # 反面对照：同一批针拿到 `src-tauri/src` 上（尺子②③④ 的树）—— 证明针本身不空转
    monitor = hits_by_file(rs_files(MONITOR_SRC), agent_name_needles(), prod=True)
    return {
        "第二个实例": SHARED_CRATES,
        "为什么它在所有尺子外面": [
            "尺子① 的树是 `remote-daemon-proto/src` ⇒ 够不着",
            "尺子②③④ 的树是 `src-tauri/src` ⇒ 够不着（crates/ 是它的**同级**目录，不是子目录）",
            "针也够不着：②③④ 数的是 `active()` / `for_kind(`，那是 monitor 侧适配层的 API，"
            "共享 crate 里一处都没有（本量具实测，见 `zero_by_adapter_needles`）",
        ],
        "读数": {"文件": r5["by_file"], "合计": r5["total"]},
        "zero_by_adapter_needles": {
            "尺子②的针在共享 crate 树上": sum(
                len(v)
                for v in hits_by_file(
                    rs_files(TREE / SHARED_CRATES), ["::" + "active("], prod=True
                ).values()
            ),
            "尺子③④的针在共享 crate 树上": sum(
                len(v)
                for v in hits_by_file(
                    rs_files(TREE / SHARED_CRATES), ["for_kind("], prod=True
                ).values()
            ),
        },
        "对照(同针在 src-tauri/src 上)": {
            "文件数": len(monitor),
            "合计": sum(len(v) for v in monitor.values()),
        },
    }


# ── 输出 ───────────────────────────────────────────────────────────────────


def head_sha() -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(TREE), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except Exception as e:  # noqa: BLE001
        return f"<拿不到: {e}>"


def dirty() -> str:
    try:
        out = subprocess.run(
            ["git", "-C", str(TREE), "status", "--porcelain"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        return "空" if not out else out
    except Exception as e:  # noqa: BLE001
        return f"<拿不到: {e}>"


RULERS = [
    ("① daemon 那个 27", ruler1),
    ("② 桌面 `::active(` 生产", ruler2a),
    ("②b `adapter.rs` 自己身上的裸 `active()`", ruler2b),
    ("③ 桌面 `for_kind` 真分派", ruler3),
    ("④ 桌面 `for_kind(` 裸子串（原文）", ruler4),
    ("⑤ 共享 crate 树里的 agent 名字", ruler5),
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--calibrate", action="store_true")
    ap.add_argument("--kp2", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--rows", action="store_true", help="连逐行逐字一起打（行号校验位）")
    a = ap.parse_args()

    ok, why = calibrate()
    if a.calibrate:
        print(f"CALIBRATION: {'OK' if ok else 'FAIL'} —— {why}")
        return 0 if ok else 1

    if a.json:
        payload = {
            "tree": str(TREE),
            "head": head_sha(),
            "git_status": dirty(),
            "calibration": {"ok": ok, "why": why},
            "rulers": {name: fn() for name, fn in RULERS},
            "kp2": kp2_evidence(),
        }
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return 0

    print(f"被测对象：{TREE}")
    print(f"量于：{head_sha()}   git status：{dirty()}")
    print(f"CALIBRATION: {'OK' if ok else 'FAIL'} —— {why}")
    if not ok:
        print("🔴 标定 FAIL ⇒ 下面的读数一律作废（剥法与那张被 cargo 钉着的表对不上）")
    print()
    for name, fn in RULERS:
        r = fn()
        print(f"── 尺子 {name} " + "─" * 40)
        print(f"  量哪棵树   : {r['tree']}")
        print(f"  分母       : {r['denominator']}")
        print(f"  粒度       : {r['granularity']}")
        print(f"  针（逐字） : {r['needles']}")
        print(f"  在哪一段量 : {r['segment']}")
        print(f"  逐文件     : {r['by_file']}")
        print(f"  合计       : {r['total']}")
        if "breakdown" in r:
            print(f"  分解       : {r['breakdown']}")
        if "noise" in r:
            print(f"  已知噪音   : {r['noise']['合计']}（住址见 --json 的 `noise.住址`）")
            print(f"  扣噪音后   : {r['total_minus_noise']}")
        if "registry_file_hits" in r:
            print(f"  注册表文件 : {r['registry_file_hits']}（判据⑦钉死 = REGISTRY.len()）")
        if a.rows:
            for f, rows in r["rows"].items():
                for n, s in rows:
                    print(f"      {f}:{n}  {s}")
        print()
    if a.kp2:
        print("── `KP2` 那一问：尺子**外面**有什么 " + "─" * 20)
        print(json.dumps(kp2_evidence(), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
