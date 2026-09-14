#!/usr/bin/env python3
"""K-R102 的**只读尺子**：`SUBCOMMANDS` 人群分档 —— 几条有自己的「可达性伞」，几条裸着。

被测对象（住址，交回时连它一起报）
---------------------------------
`<本文件所在工作树>/remote-daemon-proto/src/`。本脚本按**自己的位置**推工作树
（`Path(__file__).resolve().parents[1]`），不接受参数指树 —— 免得同名量具被换过被测对象
之后照原用法复跑，跑出的是另一棵树上的数（brief 12「静默的假读数」那一形）。

它答什么
--------
对 `crate::SUBCOMMANDS` 里的每一条 token `T`，问一句**行为**的话：

  > 把 `T` 的**分派臂**摘掉（`SUBCOMMANDS` 那张表一个字不动），今天有没有判据会红？

会红 ⇒ `T` 有自己的伞；一条都不红 ⇒ `T` **裸着**。

🔴 它**不**按名字收人
--------------------
`fn` 名里带 `reachable` / `dispatched` 的一律不算数（件文件点名的失效方向）。
人群是**遍历**出来的：扫全 crate 里每一处读 `main.rs` **源码文本**的地方
（`include_str!("main.rs")` 那一族），每一处都必须在下面 `MAIN_SOURCE_READERS` 里表态
—— 表里没有的当场 fail-closed，读数作废。这一形抄 `daemon_kill.rs::CREATION_PATHS`
（逐字：「人群靠遍历发现，不靠手写清单」）。

它买不到什么（诚实边界）
------------------------
1. **射程只到 `remote-daemon-proto` 这一个 crate 的判据面。** crate 外的伞（真跑二进制的
   e2e、monitor 侧的集成测试）不在分母里 —— 现打过一趟：本 crate 没有 `tests/` 集成目录、
   零 `CARGO_BIN_EXE`，而 monitor 侧那两处 `run_query(..., &["--usage"])` /
   `&["--list-subagents"]` **自己断言 sidecar 不存在**（前提自检），够不到 daemon 的分派。
   这两条今天由下面 `assert_no_out_of_crate_umbrella()` 各钉一刀，翻了就 fail-closed。
2. **它是静态模拟，不是真跑 `cargo`。** 每一条谓词都是对应 Rust 断言的 Python 转写；
   转写对不对由 `K-R102-cut.py` 那几刀在沙箱里真跑一遍回测（`evidence/K-R102-deathvalue.md`）。
3. **「摘臂」这个动作本身有三种形状**（单臂 / 多 token 合并臂 / 派生臂），
   派生臂那一刀是**共享**的：摘掉它，走那条路的全部 token 一起失联 ⇒ 它们的读数是同生共死的。
"""

from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

WT = Path(__file__).resolve().parents[1]
SRC = WT / "remote-daemon-proto" / "src"

FAILS: list[str] = []


def die(msg: str) -> None:
    FAILS.append(msg)


# ───────────────────────── 剥生产段（转写 guard_core::production_code） ─────────────────────────
TEST_ATTR = re.compile(r"^\s*#\[cfg\(test\)\]\s*$")


def strip_block_comments(s: str) -> str:
    out, i, depth = [], 0, 0
    while i < len(s):
        if s.startswith("/*", i):
            depth += 1
            i += 2
            continue
        if depth and s.startswith("*/", i):
            depth -= 1
            i += 2
            continue
        if depth == 0 and s.startswith("//", i):
            # 行注释里的 /* 不开块 —— 整行原样吐出去，交给下一道 filter
            j = s.find("\n", i)
            j = len(s) if j < 0 else j
            out.append(s[i:j])
            i = j
            continue
        if depth == 0:
            out.append(s[i])
        elif s[i] == "\n":
            out.append("\n")
        i += 1
    return "".join(out)


def production_code(src: str) -> str:
    """剥掉 `#[cfg(test)]` 模块 + 块注释 + 整行 `//` + 行尾 `//`。

    ⚠ 它是 Rust 那份剥法的**近似**转写。近似到不到位，由下面 `assert_stripper_sane()`
    的三条自检兜（剥完没有 `#[test]` · 字节数没塌 · 承重锚点还在）—— 那三条正是
    `guard_support::main_production_section_keeps_its_load_bearing_items` 的同一形。
    """
    lines = src.split("\n")
    kept, i = [], 0
    while i < len(lines):
        if TEST_ATTR.match(lines[i]):
            # 往后找 `mod X {` / `impl` 之类的块头，然后按列 0 的 `}` 收尾（同 Rust 那份的口径）
            j = i + 1
            while j < len(lines) and (
                lines[j].strip().startswith("#[") or lines[j].strip().startswith("#!")
            ):
                j += 1
            if j < len(lines) and lines[j].rstrip().endswith("{"):
                k = j + 1
                while k < len(lines) and lines[k] != "}":
                    k += 1
                i = k + 1
                continue
            # 单项（`const X: … = …;` / `fn …`）：跳到下一个分号或列 0 的 `}`
            if j < len(lines) and lines[j].rstrip().endswith(";"):
                i = j + 1
                continue
            k = j
            while k < len(lines) and lines[k] != "}" and not lines[k].rstrip().endswith(";"):
                k += 1
            i = k + 1
            continue
        kept.append(lines[i])
        i += 1
    s = strip_block_comments("\n".join(kept))
    s = "\n".join(l for l in s.split("\n") if not l.lstrip().startswith("//"))
    # 行尾 `//`：只在不在字符串里的时候切（够用：本尺子只吃 main.rs / history_query.rs）
    out = []
    for raw in s.split("\n"):
        in_str, cut, k = False, None, 0
        while k < len(raw):
            c = raw[k]
            if in_str:
                if c == "\\":
                    k += 2
                    continue
                if c == '"':
                    in_str = False
            elif c == '"':
                in_str = True
            elif c == "/" and raw[k + 1 : k + 2] == "/":
                cut = k
                break
            k += 1
        out.append(raw[:cut].rstrip() if cut is not None else raw)
    return "\n".join(out)


def assert_stripper_sane(name: str, raw: str, prod: str, anchors: list[str]) -> None:
    if "#[test]" in prod or "#[cfg(test)]" in prod:
        die(f"[剥法] {name} 剥完仍有测试属性 —— 生产段里混进了测试段，本尺子在拿副本对账")
    if not (2000 < len(prod) < len(raw)):
        die(f"[剥法] {name} 剥完只剩 {len(prod)} 字节（原文 {len(raw)}）—— 剥法坏了")
    for a in anchors:
        if a not in prod:
            die(f"[剥法] {name} 的生产段里找不到承重锚点 `{a}` —— 剥过头了，扫描面在静默缩水")


# ───────────────────────── 表侧：`SUBCOMMANDS`（一张 const 表，住在块外） ─────────────────────────
def read_subcommands(main_prod: str) -> list[str]:
    m = re.search(r"const SUBCOMMANDS: &\[&str\] = &\[(.*?)\n\];", main_prod, re.S)
    if not m:
        die("[表侧] 抓不到 `const SUBCOMMANDS` 的整块 —— 抽取坏了，本尺子此刻在空转")
        return []
    toks = re.findall(r'"(--[A-Za-z0-9_-]+)"', m.group(1))
    if len(toks) != len(set(toks)):
        die("[表侧] `SUBCOMMANDS` 里有重复 token")
    return toks


# ───────────────────────── 臂侧：三条路，三个**块内**的解析（表进不来） ─────────────────────────
DISPATCH_BEG = "let code = match args.first()"
DISPATCH_END = "std::process::exit(code);"
HISTORY_BEG = "let result = match args.first()"
HISTORY_END = "Some(other) =>"


def span(prod: str, beg: str, end: str, who: str) -> str:
    i = prod.find(beg)
    if i < 0:
        die(f"[臂侧] {who}：找不到块界起点 `{beg}`")
        return ""
    j = prod.find(end, i)
    if j < 0:
        die(f"[臂侧] {who}：找不到块界终点 `{end}`")
        return ""
    return prod[i:j]


def arm_tokens(block: str) -> list[str]:
    return re.findall(r'Some\("(--[A-Za-z0-9_-]+)"\)', block)


def routes(main_prod: str, hist_prod: str):
    blk = span(main_prod, DISPATCH_BEG, DISPATCH_END, "main.rs 一次性查询分派块")
    hblk = span(hist_prod, HISTORY_BEG, HISTORY_END, "history_query.rs 的 run 分派块")
    # 🔴 反喂饱自检：表**不许**落在臂侧解析的那一段里，否则又是「同一次扫描互相喂饱」
    if "SUBCOMMANDS" in blk:
        die("[反喂饱] `SUBCOMMANDS` 出现在分派块内 —— 表与臂又回到了同一次扫描，分档作废")
    lit = arm_tokens(blk)
    hist = arm_tokens(hblk)
    derived = "control::cli_control::handles(" in blk
    catchall = any(l.strip().startswith("_ =>") for l in blk.split("\n"))
    if len(lit) < 10:
        die(f"[臂侧] 分派块里只抠到 {len(lit)} 个字面量臂 token（地板 10）—— 块界找错了")
    if len(hist) < 4:
        die(f"[臂侧] history_query 的块里只抠到 {len(hist)} 个 token（地板 4）—— 块界找错了")
    return lit, hist, derived, catchall


# `cli_control::handles` 的静态对应物：REGISTRY 里 `run` 不是 `Builtin` 的那几条 + PROBE_FLAG。
def cli_exposed_flags(inbound_prod: str, cli_prod: str) -> set[str]:
    m = re.search(r"const REGISTRY: &\[CommandSpec\] = &\[(.*?)\n\];", inbound_prod, re.S)
    if not m:
        die("[臂侧] 抓不到 `inbound::REGISTRY` 的整块")
        return set()
    body = m.group(1)
    out = set()
    for spec in re.split(r"\n    CommandSpec \{", body)[1:]:
        nm = re.search(r'name: "([a-z0-9-]+)"', spec)
        if not nm:
            continue
        if re.search(r"run: Run::Builtin", spec):
            continue
        out.add("--" + nm.group(1))
    pm = re.search(r'const PROBE_FLAG: &str = "(--[a-z0-9-]+)"', cli_prod)
    if not pm:
        die("[臂侧] 抓不到 `cli_control::PROBE_FLAG`")
    else:
        out.add(pm.group(1))
    if len(out) < 5:
        die(f"[臂侧] CLI 派生面只算出 {len(out)} 条（地板 5）—— 派生法坏了")
    return out


# ───────────────────────── 人群：谁在读 `main.rs` 的源码文本（遍历，不是手写清单） ─────────────────────────
# 每一处必须表态。第二列 = 它是不是**某条子命令的分派臂**的伞；第三列 = 它是谁 / 为什么不是。
# 抄 `daemon_kill.rs::CREATION_PATHS` 那一形：表里没有的当场红，别静默放过。
MAIN_SOURCE_READERS: dict[tuple[str, str], tuple[str | None, str]] = {
    ("guard_support.rs", "main_production_section_keeps_its_load_bearing_items"): (
        None,
        "钉的是剥法没剥过头（BUILD_ID / CAPABILITIES / mod 声明），与分派臂无关",
    ),
    ("listen.rs", "refusal_reasons_are_a_closed_set"): (
        None,
        "钉 `refusal_line(` 的唯一调用点与实参来源，不是子命令分派",
    ),
    ("inbound.rs", "the_daemon_runtime_keeps_more_than_one_worker"): (
        None,
        "钉 tokio runtime 不是单 worker",
    ),
    ("protocol_doc_guard.rs", "DISPATCH_FILES"): (
        None,
        "它是**表**不是判据：把 main.rs 喂给 `dispatched_subcommands` 的字面量扫描 —— "
        "而 `SUBCOMMANDS` 自己就住在这份被扫的生产段里 ⇒ 这条路正是本件那个盲区的成因",
    ),
    ("protocol_doc_guard.rs", "emits_is_a_subset_of_frame_kinds_with_named_exemptions"): (
        None,
        "抠 `const EMITS`，帧种那一面",
    ),
    ("wire.rs", "production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen"): (
        None,
        "钉 hello 帧的 `homes` 恒空",
    ),
    ("build_id_guard.rs", "subcommand_fingerprint"): (
        None,
        "指纹取自 `crate::SUBCOMMANDS` **那张表的编译后常量值**；main.rs 的生产段只喂给剥法自检 ⇒ 臂删了它一个字不变",
    ),
    ("build_id_guard.rs", "the_crate_version_is_deliberately_zero_and_the_real_identity_has_a_home"): (
        None,
        "钉 `version = \"0.0.0\"` 与 `const BUILD_ID` 的声明各只有一处",
    ),
    ("main.rs", "production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen"): (
        None,
        "钉 hello 帧的 `unavailable` 恒空",
    ),
    ("main.rs", "the_windows_arm_is_wired_into_the_source"): (
        None,
        "钉 `TMUX_PLATFORM` 的三档选择器",
    ),
    ("main.rs", "window_raising_lives_in_one_file_and_only_behind_cfg_windows"): (
        None,
        "钉拉窗构件只住一个文件、且只在 `cfg(windows)` 之下",
    ),
    ("main.rs", "every_listed_subcommand_has_a_live_dispatch_route"): (
        "route:total",
        "`K-R102` 本件加的**总伞**：表侧读编译后的常量值，臂侧三处**块内**解析 ＋ 一次运行期派生",
    ),
    ("main.rs", "the_capture_pane_subcommand_is_actually_reachable"): (
        "pin:--capture-pane",
        "`K-R86` 加的：`pin_line` 整行钉住那条臂",
    ),
    ("main.rs", "the_oneshot_session_subcommand_is_actually_reachable"): (
        "pin:--oneshot-session",
        "`K-R87` 加的：`pin_line` 整行钉住那条臂",
    ),
    ("main.rs", "every_dispatch_arm_actually_calls_an_implementation"): (
        "arms>=7",
        "钉**存在的**臂调不调实现；摘掉一整条臂它看不见（臂数只从 7 往上掉才响）",
    ),
    ("dial/mod.rs", "the_dial_arm_is_actually_wired_into_the_dispatch"): (
        "count:--dial",
        "`K-P6b` 的 `7u` 探针补的：`\"--dial\"` 恰好 2 处 ＋ `dial::run(` 恰好 1 处",
    ),
    ("observe/accounts_query.rs", "main_dispatches_every_subcommand_we_handle"): (
        "xfile:accounts",
        "跨文件：从 accounts_query 自己的 `Some(\"--x\")` 收人，回头查 main 生产段有没有同形串",
    ),
}


def enumerate_main_source_readers() -> list[tuple[str, str]]:
    """遍历：谁 `include_str!` 了 `main.rs`。返回 (相对路径, 所在 fn / const 名)。"""
    found = []
    for p in sorted(SRC.rglob("*.rs")):
        rel = p.relative_to(SRC).as_posix()
        lines = p.read_text(encoding="utf-8").split("\n")
        for i, l in enumerate(lines):
            if re.search(r'include_str!\("(\.\./)*main\.rs"\)', l):
                owner = "?"
                for j in range(i, -1, -1):
                    m = re.match(r"\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn ([a-z0-9_]+)", lines[j])
                    if m:
                        owner = m.group(1)
                        break
                    m = re.match(r"const ([A-Z_]+):", lines[j])
                    if m:
                        owner = m.group(1)
                        break
                found.append((rel, owner))
    return found


def assert_no_out_of_crate_umbrella() -> None:
    """crate 外的伞今天不存在 —— 两条现打的判据，翻了就 fail-closed（诚实边界 1 的落点）。"""
    if (WT / "remote-daemon-proto" / "tests").is_dir():
        die("[射程] `remote-daemon-proto/tests/` 出现了 —— 可能有集成测试真跑二进制，本尺子的人群不再完整")
    hits = [
        p.relative_to(WT).as_posix()
        for p in SRC.rglob("*.rs")
        if "CARGO_BIN_EXE" in p.read_text(encoding="utf-8")
    ]
    if hits:
        die(f"[射程] 出现 `CARGO_BIN_EXE`（真跑二进制）：{hits} —— 本尺子的人群不再完整")


# ───────────────────────── 摘臂：三种形状 ─────────────────────────
def cut_arm(main_prod: str, hist_prod: str, tok: str, lit, hist, derived):
    """返回摘掉 `tok` 的分派臂之后的 (main_prod', hist_prod', 这一刀的形状)。表一个字不动。"""
    if tok in lit:
        # 单臂：整行 `Some("tok") => …,`；合并臂：那一行只有模式、没有 `=>`
        out, hit = [], 0
        for l in main_prod.split("\n"):
            if re.match(r'\s*\|?\s*Some\("' + re.escape(tok) + r'"\)', l):
                hit += 1
                continue
            out.append(l)
        if hit != 1:
            die(f"[摘臂] `{tok}` 的臂行命中 {hit} 次（应 1）—— 这一刀落点不明")
        return "\n".join(out), hist_prod, "字面量臂"
    if derived and tok in CLI_FLAGS:
        out = [l for l in main_prod.split("\n") if "control::cli_control::handles(" not in l]
        return "\n".join(out), hist_prod, "CLI 派生臂（共享一刀）"
    if tok in hist:
        out, hit = [], 0
        for l in hist_prod.split("\n"):
            if re.match(r'\s*Some\("' + re.escape(tok) + r'"\)', l):
                hit += 1
                continue
            out.append(l)
        if hit != 1:
            die(f"[摘臂] `{tok}` 在 history_query 的臂行命中 {hit} 次（应 1）")
        return main_prod, "\n".join(out), "历史查询兜底臂"
    die(f"[臂侧] `{tok}` 今天**根本没有分派落点** —— 它是幽灵条目，本尺子不给它分档")
    return main_prod, hist_prod, "无落点"


# ───────────────────────── 谓词：每条注册在案的伞，转写成一个「红不红」 ─────────────────────────
def pin_line_ok(prod: str, line: str) -> bool:
    return sum(1 for l in prod.split("\n") if l.strip() == line) == 1


PIN_LINES = {
    "--capture-pane": 'Some("--capture-pane") => control::capture_pane::run(&args),',
    "--oneshot-session": 'Some("--oneshot-session") => control::oneshot_session::run(&args),',
}


def umbrellas_that_go_red(main_p: str, hist_p: str, acct: list[str], live: dict[str, str]) -> list[str]:
    """摘臂之后，**今天盘上真在的**那几条伞里有谁会红。

    🔴 `live` 是从**遍历**结果里算出来的（判据在不在盘上 ⇒ 它的谓词算不算）——
    不是一张写死的谓词表。D1 的死值验 ① 就打在这一处：把某条伞的判据整段摘掉，
    它的谓词必须跟着从 `live` 里消失，分档读数当场变。第一版写死了谓词表，
    摘掉判据读数一动不动 —— 那一版数的是「臂在不在」，不是「有没有伞在看着它」。
    """
    red = []
    for key, who in live.items():
        if key.startswith("pin:"):
            tok = key[4:]
            if not pin_line_ok(main_p, PIN_LINES[tok]):
                red.append(who)
        elif key == "count:--dial":
            if not (main_p.count('"--dial"') == 2 and main_p.count("dial::run(") == 1):
                red.append(who)
        elif key == "xfile:accounts":
            if any(f'Some("{n}")' not in main_p for n in acct):
                red.append(who)
        elif key == "arms>=7":
            blk = span(main_p, DISPATCH_BEG, DISPATCH_END, "（摘臂后）分派块")
            arms = [b for _, sep, b in (l.partition("=>") for l in blk.split("\n")) if sep]
            if len(arms) < 7:
                red.append(who)
        elif key == "route:total":
            if total_umbrella_reds(main_p, hist_p):
                red.append(who)
    return red


def total_umbrella_reds(main_p: str, hist_p: str) -> bool:
    """`route:total` 的转写：表侧 = `SUBCOMMANDS` 的值；臂侧 = 三处块内解析 ＋ 运行期派生。"""
    blk = span(main_p, DISPATCH_BEG, DISPATCH_END, "（摘臂后）分派块")
    hblk = span(hist_p, HISTORY_BEG, HISTORY_END, "（摘臂后）history_query 块")
    lit = arm_tokens(blk)
    fallback = arm_tokens(hblk)
    derived = "cli_control::handles(" in blk
    catchall = any(l.strip().startswith("_ =>") for l in blk.split("\n"))
    if len(lit) < 6 or not derived or not catchall or len(fallback) < 3:
        return True  # 反空真地板先响
    for t in read_subcommands(main_p):
        if t in lit:
            continue
        if derived and t in CLI_FLAGS:
            continue
        if catchall and t in fallback:
            continue
        return True
    # 第二格：双路那几条的字面量臂不许悄悄消失（表只有一个住址 —— Rust 源里那一张）
    for t in DUAL_ROUTE_ARMS:
        if t not in lit:
            return True
    for t in read_subcommands(main_p):
        if derived and t in CLI_FLAGS and t in lit and t not in DUAL_ROUTE_ARMS:
            return True
    return False


def read_dual_route_arms(raw_main: str) -> list[str]:
    """`DUAL_ROUTE_ARMS` 的**唯一住址**是 Rust 源里那张表 —— 本尺子现读，不复述成员。"""
    m = re.search(r"const DUAL_ROUTE_ARMS: &\[\(&str, &str\)\] = &\[(.*?)\n    \];", raw_main, re.S)
    if not m:
        # 表不在盘上，只有两种可能：① 本件的实现被整段退掉了（`unharden` 那一刀，合法）；
        # ② 表被人删了而总伞还在（那就是真缺陷）。② 由下面「route:total 在人群里却没表」那一条判。
        return []
    toks = re.findall(r'"(--[a-z0-9-]+)",', m.group(1))
    if not toks:
        die("[双路表] `DUAL_ROUTE_ARMS` 抠出空集 —— 抠法坏了")
    return toks


def accounts_own_tokens() -> list[str]:
    prod = production_code((SRC / "observe" / "accounts_query.rs").read_text(encoding="utf-8"))
    toks = []
    for t in re.findall(r'Some\("(--[a-z0-9-]+)"\)', prod):
        if t not in toks:
            toks.append(t)
    if len(toks) != 4:
        die(f"[人群] 从 accounts_query 生产段抠到 {len(toks)} 条（Rust 那条判据自己断言 4）")
    return toks


# ───────────────────────── 主流程 ─────────────────────────
def main() -> int:
    global CLI_FLAGS, DUAL_ROUTE_ARMS
    raw_main = (SRC / "main.rs").read_text(encoding="utf-8")
    raw_hist = (SRC / "observe" / "history_query.rs").read_text(encoding="utf-8")
    raw_inb = (SRC / "inbound.rs").read_text(encoding="utf-8")
    raw_cli = (SRC / "control" / "cli_control.rs").read_text(encoding="utf-8")
    main_prod = production_code(raw_main)
    hist_prod = production_code(raw_hist)
    inb_prod = production_code(raw_inb)
    cli_prod = production_code(raw_cli)
    assert_stripper_sane("main.rs", raw_main, main_prod, ["const BUILD_ID", "const SUBCOMMANDS"])
    assert_stripper_sane("history_query.rs", raw_hist, hist_prod, ["pub fn run("])
    assert_no_out_of_crate_umbrella()

    subs = read_subcommands(main_prod)
    lit, hist, derived, catchall = routes(main_prod, hist_prod)
    CLI_FLAGS = cli_exposed_flags(inb_prod, cli_prod)
    acct = accounts_own_tokens()
    DUAL_ROUTE_ARMS = read_dual_route_arms(raw_main)

    # 人群自检：遍历出来的每一处都要在表里表态
    seen = enumerate_main_source_readers()
    unreg = [x for x in seen if x not in MAIN_SOURCE_READERS]
    if unreg:
        die(f"[人群] 这几处读了 `main.rs` 源码文本却没在 `MAIN_SOURCE_READERS` 里表态：{unreg}")
    # ⚠ 两个方向**不同档**，刻意不压成一件事：
    #   · **登记了、盘上没有** ⇒ 那条判据被摘掉了。这是**建模得对**的表现（它的谓词跟着退出
    #     人群，分档读数当场变）⇒ 只**点名**，不作废 —— D1 的死值验 ① 打的就是这一格。
    #   · **盘上有、没登记** ⇒ 本尺子不知道那条判据在守什么 ⇒ 分档可能画错，**fail-closed**。
    ghost = [k for k in MAIN_SOURCE_READERS if k not in seen]
    # 🔴 谓词人群 = **遍历到的** ∩ **登记为伞的**。判据被摘掉 ⇒ 它的谓词当场退出人群。
    live = {
        MAIN_SOURCE_READERS[k][0]: f"{k[0]}::{k[1]}"
        for k in seen
        if MAIN_SOURCE_READERS[k][0]
    }
    if "route:total" in live and not DUAL_ROUTE_ARMS:
        die("[双路表] 总伞在盘上，而 `const DUAL_ROUTE_ARMS` 抓不到 —— 它的第二格此刻模不出来")

    print(f"# K-R102 尺子 · 被测树 {WT}")
    print(f"# 量于 `git -C {WT} rev-parse --short HEAD`（见交回时贴的那个尖）")
    print()
    print(f"人群（`SUBCOMMANDS` 现打）：**{len(subs)}** 条")
    print(f"读 `main.rs` 源码文本的地方（遍历）：{len(seen)} 处，逐处表态 {len(MAIN_SOURCE_READERS)} 条")
    print(f"其中登记为**伞**、且今天真在盘上的谓词：{len(live)} 条 —— {sorted(live)}")
    print(f"登记在案、今天**不在盘上**的判据：{len(ghost)} 条 —— {ghost}（它们的谓词已退出人群）")
    print(f"CLI 派生面（`REGISTRY` 非 Builtin ＋ PROBE_FLAG）：{len(CLI_FLAGS)} 条")
    print(f"双路登记表 `DUAL_ROUTE_ARMS`（现读 Rust 源）：{len(DUAL_ROUTE_ARMS)} 条 —— {DUAL_ROUTE_ARMS}")
    print()

    rows, bare, umb = [], [], []
    for t in subs:
        m2, h2, shape = cut_arm(main_prod, hist_prod, t, lit, hist, derived)
        red = umbrellas_that_go_red(m2, h2, acct, live)
        rows.append((t, shape, red))
        (umb if red else bare).append(t)

    w = max(len(t) for t in subs) + 1
    print(f"{'token'.ljust(w)}| 分派形状            | 摘掉它的臂 ⇒ 谁红")
    print("-" * (w + 68))
    for t, shape, red in rows:
        print(f"{t.ljust(w)}| {shape.ljust(19)}| {'、'.join(red) if red else '🔴 一条都不红（裸）'}")
    print()
    print(f"**已有伞：{len(umb)}/{len(subs)}** —— {umb}")
    print(f"**裸着：{len(bare)}/{len(subs)}** —— {bare}")

    # 死值验 ② 的阳性回测（纪律 ⑳）：已知会命中的样本必须落进「已有伞」
    if "--capture-pane" not in umb:
        die("[阳性回测] `--capture-pane` 没被数进「已有伞」—— 尺子没跑或人群画错，读数作废")
    if "--capture-pane" in umb:
        r = dict((t, red) for t, _, red in rows)["--capture-pane"]
        if not any("the_capture_pane_subcommand_is_actually_reachable" in x for x in r):
            die("[阳性回测] `--capture-pane` 红的不是 `K-R86` 那条判据 —— 分档认错了人")

    print()
    if FAILS:
        print("## 🔴 fail-closed —— 读数作废")
        for f in FAILS:
            print("  · " + f)
        return 1
    print("## 自检全过（剥法 · 反喂饱 · 人群遍历 · 射程 · 阳性回测）")
    return 0


CLI_FLAGS: set[str] = set()
DUAL_ROUTE_ARMS: list[str] = []

# ───────────────────────── 附：改动面 —— 逐 `fn` 整块 md5（基点 vs 工作树） ─────────────────────────
def fn_blocks(text: str) -> dict[str, str]:
    """按**缩进**认块：`<缩进>[pub…][async ]fn 名(` 起，同缩进的 `}` 止。

    ⚠ 射程（别读宽）：① 认任意缩进（本件改的全在 `mod argv_table_guard` 里，顶层那一版够不着）；
    ② 比的是**整块字节**（含块内注释），**不含**上方的 `///` 头注 ⇒ 只改头注不判「变了」；
    ③ 同名只留最后一个 —— 交回时把「几个名字 / 几块」一起报，对不上说明认块跑飞了。
    """
    out: dict[str, str] = {}
    lines = text.split("\n")
    pat = re.compile(r"^(\s*)(?:pub(?:\([a-z()]+\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
    for i, l in enumerate(lines):
        m = pat.match(l)
        if not m:
            continue
        ind, name = m.group(1), m.group(2)
        close = ind + "}"
        j = i + 1
        while j < len(lines) and lines[j] != close:
            j += 1
        if j >= len(lines):
            continue
        blk = "\n".join(lines[i : j + 1])
        out[name] = hashlib.md5(blk.encode()).hexdigest()[:12]
    return out


def changed_surface(rev: str) -> int:
    import subprocess

    rels = ["remote-daemon-proto/src/main.rs"]
    print(f"# 改动面 · 逐 `fn` 整块 md5 · 基点 {rev} vs 工作树 {WT}")
    rc = 0
    for rel in rels:
        base = subprocess.run(
            ["git", "-C", str(WT), "show", f"{rev}:{rel}"], capture_output=True, text=True
        )
        if base.returncode:
            print(f"🔴 取不到 {rev}:{rel} —— {base.stderr.strip()}")
            return 3
        a, b = fn_blocks(base.stdout), fn_blocks((WT / rel).read_text(encoding="utf-8"))
        print(f"\n## {rel} —— 基点 {len(a)} 块 · 工作树 {len(b)} 块")
        for k in sorted(set(a) | set(b)):
            if a.get(k) == b.get(k):
                continue
            rc = 1
            print(f"  {k:56s} {a.get(k, '（基点没有）')} -> {b.get(k, '（没了）')}")
    print("\n（上面没列出来的每一块，基点与工作树整块 md5 逐个相同）")
    return 0


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--fn-md5":
        sys.exit(changed_surface(sys.argv[2]))
    sys.exit(main())
