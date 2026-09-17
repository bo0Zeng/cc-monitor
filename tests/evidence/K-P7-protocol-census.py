#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-P7 摸底量具 —— 把 `§0a` 那四个读数与用户裁定那三格**逐条重打**。

住址（`brief` 12 要求量具的住址唯一）：
    <工作树>/evidence/K-P7-protocol-census.py
被测对象：**运行本脚本时所在的那棵工作树**（`--root` 默认取本文件的上上级目录）。
    ⇒ 交回时连「跑在哪棵树的哪个 sha 上」一起写；本脚本自己也把它印在输出头上。

它答哪几格（每一格都印出「分母怎么数的」与「用什么量的」）：

  【1】`wire::Frame` 的变体数 —— **两把独立的尺子**：
       ① `pub enum Frame {` 里 brace-depth==1 的变体名（声明面）
       ② `impl Frame` 里两个**穷尽 match** 的臂数（`loss_is_recoverable` / `loss_identity`）
       两把同值才算数；不同就把差集印出来（`§3 P7M1` 要的就是这一格）。
  【2】`inbound::COMMANDS` 的条数 + 逐字成员 + 它住在第几行。
  【3】`single_stream_guard::PINS` —— **条数**（PM 写「三根针」，本格重打）
       ＋ 每条的 (文件, 锚点, 文件内登记数, 全 crate 登记数)，并用**复刻的 `production_code`**
       在盘上重打「文件内实际命中」与「全 crate 实际命中」，与登记值对拍。
       ⚠ 复刻口径：`production_source`（剥 `#[cfg(test)] mod X {…}`）→ 剥块注释 →
         删整行 `//` → 剥行尾 `//`；全 crate 那一档**排除 `single_stream_guard.rs` 自己**
         （`scan_tree!` 逐字 `scan_tree_excluding_self(…, file!())`）。
  【4】`Overflow` / `LostFrame` 的**语义读数**：字段表 ＋ `loss_is_recoverable` 的
       可恢复 / 不可恢复分组（那本账按什么记，是本件第 4 问的全部内容）。
  【5】`relay/` 的形状：文件数 · 行数 · `Frame::`|`wire::` 命中数（PM 说 11 份 / 8514 行 / 0 处）。
  【6】monitor 侧 `parse_frame` 认识的 kind 集（消费 / 认识但不消费），
       与 daemon 侧 `Frame` 变体逐个对拍 —— 「界面到底要哪几类」的分母就是它。
  【7】`no_timer_guard` 的禁用构件 × 界面侧拨号路径的**生产段** ——
       「把拨号搬进 daemon」要付、而 `§0a` 三堵墙里没有的那一笔（读数落 `readings §④-辛`）。

⚠ 本尺子**保证不了**什么（别读大一格）：
  · 它是**文本尺子**，不是编译器。`use` 别名、宏展开、`cfg` 分支它都不解析。
  · 【3】的「全 crate」只扫 `remote-daemon-proto/src/**.rs`，与 `crate_sources()` 同人群。
  · 【6】按 `match kind {` 的字面臂取名，改成非字面量匹配它就看不见（会在输出里出声）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

# ── 复刻 guard_core 的 production_code ────────────────────────────────────────
# 住址：src-tauri/crates/guard-core/src/lib.rs
#   production_code = strip_block_comments(production_source(src))
#                     → 删整行 `//` → strip_trailing_comments


def _cfg_is_test_only(attr: str) -> bool:
    """逐字复刻 `cfg_is_test_only`：属性里出现独立的 `test` 这个词。"""
    def ident(c: str) -> bool:
        return c.isalnum() or c in "_-"

    for m in re.finditer("test", attr):
        k = m.start()
        before_ok = k == 0 or not ident(attr[k - 1])
        after = k + 4
        after_ok = after >= len(attr) or not ident(attr[after])
        if before_ok and after_ok:
            return True
    return False


def production_source(src: str) -> str:
    """复刻 `test_module_ranges` + `production_source`：剥 `#[cfg(test)] mod X {…}`。"""
    OPEN, CLOSE = "\n#[cfg(", "\n}"
    ranges: list[tuple[int, int]] = []
    i = 0
    while True:
        rel = src.find(OPEN, i)
        if rel < 0:
            break
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
        rel_end = src.find(CLOSE, j)
        if rel_end < 0:
            ranges.append((j, len(src)))
            break
        end = rel_end + len(CLOSE)
        ranges.append((j, end))
        i = end
    out, cur = [], 0
    for (s, e) in ranges:
        out.append(src[cur:s])
        cur = e
    out.append(src[cur:])
    return "".join(out)


def _mask_char_literals(line: str) -> str:
    b = list(line)
    i, n = 0, len(b)
    while i < n:
        if b[i] == "'":
            if i + 3 < n and b[i + 1] == "\\":
                k = line.find("'", i + 3)
                if k >= 0:
                    for t in range(i, k + 1):
                        b[t] = "_"
                    i = k + 1
                    continue
            if i + 1 < n:
                c = b[i + 1]
                if c not in ("'", "\\") and i + 2 < n and b[i + 2] == "'":
                    b[i] = b[i + 1] = b[i + 2] = "_"
                    i += 3
                    continue
        i += 1
    return "".join(b)


def _raw_string_open(sb: str, i: int):
    def ident(c: str) -> bool:
        return c.isalnum() or c == "_"

    if i > 0 and ident(sb[i - 1]):
        return None
    k = i
    if k < len(sb) and sb[k] == "b":
        k += 1
        if k < len(sb) and sb[k] == '"':
            return (0, k + 1 - i)
    if k >= len(sb) or sb[k] != "r":
        return None
    k += 1
    hash_start = k
    while k < len(sb) and sb[k] == "#":
        k += 1
    if k >= len(sb) or sb[k] != '"':
        return None
    return (k - hash_start, k + 1 - i)


def strip_block_comments(src: str) -> str:
    """复刻 `try_strip_block_comments`（失败即原样返回，同 Rust 侧的 unwrap_or_else）。"""
    out: list[str] = []
    depth, in_str, raw_hashes = 0, False, None
    lines = src.split("\n")
    for li, raw in enumerate(lines):
        scan = _mask_char_literals(raw) if (depth == 0 and not in_str and raw_hashes is None) else raw
        line = list(raw)
        i, n = 0, len(scan)
        # 掩码等长 ⇒ 下标一一对应（Rust 侧按字节，这里按字符：两侧都只在 ASCII 记号上下标）
        while i < n:
            if depth > 0:
                if scan[i] == "/" and i + 1 < n and scan[i + 1] == "*":
                    depth += 1
                    line[i] = line[i + 1] = " "
                    i += 2
                    continue
                if scan[i] == "*" and i + 1 < n and scan[i + 1] == "/":
                    depth -= 1
                    line[i] = line[i + 1] = " "
                    i += 2
                    continue
                line[i] = " "
                i += 1
                continue
            if raw_hashes is not None:
                h = raw_hashes
                if scan[i] == '"' and n >= i + 1 + h and all(c == "#" for c in scan[i + 1 : i + 1 + h]):
                    raw_hashes = None
                    i += 1 + h
                    continue
                i += 1
                continue
            if in_str:
                if scan[i] == "\\":
                    i += 2
                    continue
                if scan[i] == '"':
                    in_str = False
                i += 1
                continue
            ro = _raw_string_open(scan, i)
            if ro is not None:
                h, consumed = ro
                if h == 0 and scan[i] == "b":
                    in_str = True
                else:
                    raw_hashes = h
                i += consumed
                continue
            if scan[i] == '"':
                in_str = True
                i += 1
                continue
            if scan[i] == "/" and i + 1 < n and scan[i + 1] == "/":
                break
            if scan[i] == "/" and i + 1 < n and scan[i + 1] == "*":
                depth += 1
                line[i] = line[i + 1] = " "
                i += 2
                continue
            i += 1
        out.append("".join(line))
    if depth != 0 or in_str or raw_hashes is not None:
        return src
    return "\n".join(out)


def strip_trailing_comments(src: str) -> str:
    """复刻 `strip_trailing_comments`（含它对 raw/byte 串那条「原样留下、状态归零」）。"""
    out: list[str] = []
    in_str = False
    for raw in src.split("\n"):
        masked = _mask_char_literals(raw)
        if 'r"' in masked or "r#" in masked or 'b"' in masked:
            out.append(raw)
            in_str = False
            continue
        n = len(masked)
        if in_str:
            i = 0
            while i < n:
                if masked[i] == "\\":
                    i += 2
                    continue
                if masked[i] == '"':
                    in_str = False
                    break
                i += 1
            out.append(raw)
            continue
        i, cut = 0, None
        while i < n:
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
            if masked[i] == "/" and i + 1 < n and masked[i + 1] == "/":
                cut = i
                break
            i += 1
        out.append(raw[:cut] if cut is not None else raw)
    return "\n".join(out)


def production_code(src: str) -> str:
    no_block = strip_block_comments(production_source(src))
    kept = "\n".join(l for l in no_block.split("\n") if not l.lstrip().startswith("//"))
    return strip_trailing_comments(kept)


# ── 各格 ──────────────────────────────────────────────────────────────────────


def read(p: str) -> str:
    with open(p, encoding="utf-8") as f:
        return f.read()


def frame_variants_decl(wire_src: str):
    """尺子①：`pub enum Frame {` 里 brace-depth==1 的变体名。"""
    m = re.search(r"^pub enum Frame \{$", wire_src, re.M)
    if not m:
        return None, "找不到 `pub enum Frame {` 那一行 —— 声明面尺子空转"
    body = wire_src[m.end() :]
    depth, i, names, line_no = 0, 0, [], wire_src[: m.start()].count("\n") + 1
    cur_line = line_no
    while i < len(body):
        c = body[i]
        if c == "\n":
            cur_line += 1
        elif c == "{":
            depth += 1
        elif c == "}":
            if depth == 0:
                break
            depth -= 1
        elif depth == 0 and re.match(r"[A-Z]", c):
            # 变体名只可能出现在 depth==0（enum 体内、字段体外）且行首缩进 4
            j = i
            while j < len(body) and (body[j].isalnum() or body[j] == "_"):
                j += 1
            word = body[i:j]
            # 行首（去掉缩进）就是它 ⇒ 是变体名
            ls = body.rfind("\n", 0, i) + 1
            if body[ls:i].strip() == "":
                names.append((word, cur_line))
            i = j
            continue
        i += 1
    return names, None


def exhaustive_match_arms(wire_src: str, fn_name: str):
    """尺子②：`impl Frame` 里某个穷尽 match 的 `Frame::X` 臂。"""
    m = re.search(r"pub fn %s\(" % re.escape(fn_name), wire_src)
    if not m:
        return None
    tail = wire_src[m.end() :]
    # 取到下一个 `\n    }` （方法收尾）
    end = tail.find("\n    }")
    body = tail[: end if end > 0 else len(tail)]
    body = production_code(body)
    return sorted(set(re.findall(r"Frame::([A-Za-z_][A-Za-z0-9_]*)\s*\{", body)))


def commands_list(inbound_src: str):
    m = re.search(r"pub const COMMANDS: &\[&str\] =\s*\n?\s*&\[(.*?)\];", inbound_src, re.S)
    if not m:
        return None, None
    items = re.findall(r'"([^"]+)"', m.group(1))
    line = inbound_src[: m.start()].count("\n") + 1
    return items, line


def parse_pins(guard_src: str):
    m = re.search(r"const PINS: &\[\(&str, &str, usize, usize, &str\)\] = &\[", guard_src)
    if not m:
        return None
    body = guard_src[m.end() :]
    depth, i = 1, 0
    while i < len(body):
        if body[i] == "[":
            depth += 1
        elif body[i] == "]":
            depth -= 1
            if depth == 0:
                break
        i += 1
    table = body[:i]
    pins = []
    for mm in re.finditer(
        r'\(\s*"([^"]+)",\s*\n\s*"((?:[^"\\]|\\.)*)",\s*\n\s*(\d+),\s*\n\s*(\d+),', table
    ):
        needle = mm.group(2).encode().decode("unicode_escape")
        pins.append((mm.group(1), needle, int(mm.group(3)), int(mm.group(4))))
    return pins


def crate_files(src_root: str):
    out = []
    for dirpath, _dirs, files in os.walk(src_root):
        for f in sorted(files):
            if f.endswith(".rs"):
                p = os.path.join(dirpath, f)
                out.append((os.path.relpath(p, src_root).replace("\\", "/"), p))
    return sorted(out)


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    default_root = os.path.dirname(here)
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=default_root, help="被测工作树的根")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    def rp(*a):
        return os.path.join(root, *a)

    def sha_of(p):
        return hashlib.md5(read(p).encode()).hexdigest()[:8]

    try:
        head = subprocess.run(
            ["git", "-C", root, "rev-parse", "HEAD"], capture_output=True, text=True
        ).stdout.strip()
    except Exception:
        head = "<拿不到>"

    R = {"root": root, "head": head}
    P = print
    P("=" * 78)
    P("K-P7 协议面普查 —— `§0a` 四个读数 + 用户裁定三格，逐条重打")
    P("量具住址: <工作树>/evidence/K-P7-protocol-census.py")
    P(f"被测工作树: {root}")
    P(f"被测树 HEAD: {head}")
    P("=" * 78)

    wire_p = rp("remote-daemon-proto/src/wire.rs")
    inbound_p = rp("remote-daemon-proto/src/inbound.rs")
    guard_p = rp("remote-daemon-proto/src/single_stream_guard.rs")
    src_root = rp("remote-daemon-proto/src")

    # 【1】Frame 变体
    P("\n【1】`wire::Frame` 变体数 —— 两把尺子")
    P(f"  被测文件 wire.rs md5[:8] = {sha_of(wire_p)}  ({sum(1 for _ in open(wire_p, encoding='utf-8'))} 行)")
    wire_src = read(wire_p)
    decl, err = frame_variants_decl(wire_src)
    if err:
        P("  🔴 " + err)
        R["frame_decl"] = None
    else:
        P(f"  尺子① 声明面（`pub enum Frame {{` 内 depth==0 的变体名）：**{len(decl)}** 个")
        for name, ln in decl:
            P(f"      wire.rs:{ln}  {name}")
        R["frame_decl"] = [n for n, _ in decl]
    for fn in ("loss_is_recoverable", "loss_identity"):
        arms = exhaustive_match_arms(wire_src, fn)
        if arms is None:
            P(f"  🔴 尺子② 找不到 `{fn}`")
            continue
        P(f"  尺子② `{fn}` 的穷尽 match 臂：**{len(arms)}** 个 -> {', '.join(arms)}")
        R[f"arms_{fn}"] = arms
        if decl:
            d = set(n for n, _ in decl)
            if d != set(arms):
                P(f"      🔴 两把尺子不同！只在声明面: {sorted(d - set(arms))} · 只在 match: {sorted(set(arms) - d)}")
            else:
                P("      ✅ 与声明面**逐个相同**（分母 = 两边的并集）")

    # 【2】COMMANDS
    P("\n【2】`inbound::COMMANDS`")
    inbound_src = read(inbound_p)
    items, line = commands_list(inbound_src)
    if items is None:
        P("  🔴 找不到 `pub const COMMANDS`")
    else:
        P(f"  住址 inbound.rs:{line}  ·  条数 = **{len(items)}**")
        P("  逐字: " + json.dumps(items, ensure_ascii=False))
        P(f"  字典序? {'是' if items == sorted(items) else '否'}")
        R["commands"] = items

    # 【3】PINS
    P("\n【3】`single_stream_guard::PINS`")
    guard_src = read(guard_p)
    pins = parse_pins(guard_src)
    if pins is None:
        P("  🔴 解析不到 `PINS`")
        return 2
    P(f"  🔴 **登记条数 = {len(pins)}**（分母 = `PINS` 这张表的元组个数，表体由 brace/bracket 配对切出）")
    files = crate_files(src_root)
    prod = {}
    for rel, p in files:
        prod[rel] = production_code(read(p))
    # `scan_tree!` 排除调用者自己那份
    self_rel = "single_stream_guard.rs"
    P(f"  全 crate 人群：`{os.path.relpath(src_root, root)}/**.rs` 共 {len(files)} 份；"
      f"**排除调用者自己** `{self_rel}`（`scan_tree_excluding_self`）⇒ 分母 {len(files) - 1} 份")
    P("")
    P("  | # | 文件 | 锚点 | 登记(文件内) | 重打 | 登记(全crate) | 重打 | 判 |")
    P("  |---|---|---|---|---|---|---|---|")
    rows = []
    for idx, (f, needle, want, cwant) in enumerate(pins, 1):
        got_file = prod.get(f, "").count(needle)
        got_crate = sum(v.count(needle) for k, v in prod.items() if k != self_rel)
        ok = "✅" if (got_file == want and got_crate == cwant) else "🔴"
        P(f"  | {idx} | `{f}` | `{needle}` | {want} | **{got_file}** | {cwant} | **{got_crate}** | {ok} |")
        rows.append(
            {"file": f, "needle": needle, "want": want, "got": got_file,
             "crate_want": cwant, "crate_got": got_crate}
        )
    R["pins"] = rows
    P("")
    P("  ⚠ 上表的「重打」用的是**本尺子复刻的** `production_code`，不是 crate 自己那份 ——")
    P("    两者若不同值，先怀疑复刻，别先怀疑盘上。crate 自己那份的判决在门禁 `daemon` 那一格。")

    # 【4】Overflow / LostFrame 语义
    P("\n【4】`Overflow.lost` 的语义（第 4 个读数）")
    ov = re.search(r"\n    Overflow \{(.*?)\n    \},", wire_src, re.S)
    if ov:
        fields = re.findall(r"^\s{8}([a-z_]+):\s*([^,]+),", ov.group(1), re.M)
        P("  `Frame::Overflow` 的字段（分母 = 那个变体体内缩进 8 的 `名: 类型,` 行）：")
        for n, t in fields:
            P(f"      {n}: {t}")
        R["overflow_fields"] = [n for n, _ in fields]
    lf = re.search(r"pub struct LostFrame \{(.*?)\n\}", wire_src, re.S)
    if lf:
        fields = re.findall(r"^\s{4}pub ([a-z_]+):\s*([^,]+),", lf.group(1), re.M)
        P("  `LostFrame` 的字段：" + ", ".join(f"{n}: {t}" for n, t in fields))
        R["lostframe_fields"] = [n for n, _ in fields]
    m = re.search(r"pub fn loss_is_recoverable\(", wire_src)
    if m:
        tail = wire_src[m.end() :]
        end = tail.find("\n    }")
        body = production_code(tail[: end if end > 0 else len(tail)])
        rec = re.findall(r"Frame::([A-Za-z_]+)\s*\{[^}]*\}\s*=>\s*(true|false)", body)
        yes = [k for k, v in rec if v == "true"]
        no = [k for k, v in rec if v == "false"]
        P(f"  `loss_is_recoverable`：可恢复 **{len(yes)}** 个 {yes}")
        P(f"                        不可恢复 **{len(no)}** 个 {no}")
        P("  ⇒ 那本账的记账单位 = **一条 watcher→writer 通道**（`CHANNEL_CAPACITY`），"
          "身份表上界 `LOST_IDENTITY_CAP`；`subject` 只有 sid / tmux 会话名，**没有任何一维说「哪台机」**。")
        R["recoverable"] = yes
        R["unrecoverable"] = no

    # 【5】relay 形状
    P("\n【5】`relay/` 的形状（PM 写：11 份 / 8514 行 / 引用协议帧 0 处）")
    relay_root = rp("remote-daemon-proto/src/relay")
    rel_files = sorted(
        os.path.join(relay_root, f) for f in os.listdir(relay_root) if f.endswith(".rs")
    )
    subdirs = [d for d in os.listdir(relay_root) if os.path.isdir(os.path.join(relay_root, d))]
    total = 0
    for p in rel_files:
        n = read(p).count("\n") + (0 if read(p).endswith("\n") else 1)
        total += n
        P(f"      {n:6d}  relay/{os.path.basename(p)}")
    P(f"  文件数 = **{len(rel_files)}**（分母 = `relay/` 下 `*.rs`，子目录 {len(subdirs)} 个）· 行数合计 = **{total}**")
    hits = []
    for p in rel_files:
        src = read(p)
        for i, ln in enumerate(src.split("\n"), 1):
            for pat in ("Frame::", "wire::"):
                if pat in ln:
                    hits.append((os.path.basename(p), i, pat, ln.strip()[:60]))
    P(f"  `Frame::` / `wire::` 命中 = **{len(hits)}** 处（分母 = 上面那 {len(rel_files)} 份的**全文**，注释也算）")
    for h in hits:
        P(f"      {h[0]}:{h[1]} [{h[2]}] {h[3]}")
    R["relay"] = {"files": len(rel_files), "lines": total, "frame_hits": len(hits)}

    # 【6】monitor 侧认识的 kind
    P("\n【6】monitor 侧 `parse_frame` 认识的 kind（界面到底要哪几类的分母）")
    ssh_p = rp("src-tauri/src/ssh_source.rs")
    ssh_src = read(ssh_p)
    m = re.search(r"pub fn parse_frame\(line: &str\) -> Option<InboundFrame> \{", ssh_src)
    body = ssh_src[m.end() :] if m else ""
    end = body.find("\n}")
    body = body[: end if end > 0 else len(body)]
    arms = re.findall(r'^\s{8}"([a-z_]+)" =>', body, re.M)
    none_arms = re.findall(r'^\s{8}"([a-z_]+)" => None,', body, re.M)
    P(f"  `match kind` 的字面臂 = **{len(arms)}** 个：{arms}")
    P(f"  其中**认识但刻意不消费**（`=> None`）= {len(none_arms)} 个：{none_arms}")
    if R.get("frame_decl"):
        snake = {re.sub(r"(?<!^)(?=[A-Z])", "_", n).lower() for n in R["frame_decl"]}
        P(f"  与 daemon 侧 {len(snake)} 个变体对拍：daemon 有而 monitor 没有 = {sorted(snake - set(arms))}"
          f" · monitor 有而 daemon 没有 = {sorted(set(arms) - snake)}")
    R["monitor_kinds"] = arms
    R["monitor_kinds_not_consumed"] = none_arms

    # 【7】零定时器铁律 × 拨号路径
    P("\n【7】`no_timer_guard` 的禁用构件 × 界面侧拨号路径（搬家的一条硬代价）")
    banned = ["thread::sleep", "time::sleep", "recv_timeout", "time::interval",
              "Instant::now", "Duration::from_secs"]
    P("  daemon 侧禁用清单（取自 `no_timer_guard` 的 `BANNED`，逐条现读）：" + " · ".join(f"`{b}`" for b in banned))
    ntg = read(rp("remote-daemon-proto/src/no_timer_guard.rs"))
    m = re.search(r"REGISTERED_DURATION_USES: &\[\(&str, &str, &str, &str, &str\)\] = &\[", ntg)
    reg = 0
    if m:
        body = ntg[m.end():]
        d, i = 1, 0
        while i < len(body):
            if body[i] == "[":
                d += 1
            elif body[i] == "]":
                d -= 1
                if d == 0:
                    break
            i += 1
        reg = len(re.findall(r'\(\s*\n\s*"([^"]*)",\s*\n\s*"([^"]*)",\s*\n\s*"', body[:i]))
    P(f"  `REGISTERED_DURATION_USES` 现打 **{reg}** 条（例外要逐条登记 + 写解锁条件）")
    for rel in ["src-tauri/src/ssh_source.rs", "src-tauri/src/port_forward.rs",
                "src-tauri/src/sftp.rs", "src-tauri/src/sftp_pool.rs"]:
        prod_src = production_code(read(rp(rel)))
        hits = []
        for ln_no, ln in enumerate(prod_src.split("\n"), 1):
            for b in banned:
                if b in ln:
                    hits.append((b, ln.strip()[:72]))
        P(f"  {rel} 生产段命中 **{len(hits)}** 处：")
        for b, txt in hits:
            P(f"      [{b}] {txt}")
    P("  ⚠ 分母 = 上面四份文件的**生产段**（同 `production_code` 口径，测试段不算）；")
    P("    尺子是**子串**，不是 `is_call_of` 那个带词边界的调用匹配器 ⇒ 这个数是**上界**。")

    P("\n" + "=" * 78)
    if args.json:
        P(json.dumps(R, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
