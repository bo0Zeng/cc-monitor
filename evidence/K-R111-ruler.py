#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R111 只读尺子：**乙一那 14 个能力 id 今天还剩多少接线活**。

只读。一个字节的生产代码都不改，一条开发测试都不跑（`K31`）。

# 它答什么、不答什么

**答**：14 个 id 名下的每一条 Tauri 命令，今天前端**还有没有自己的实现**；
若有，那处实现落在**读面 / 执行面 / 渲染面**哪一面、住址是什么；
后端今天**有没有对侧**（现打 daemon 的两张具名表，不抄名单）。

**不答**：① 这几行代码今天在真机上跑不跑得通（要真机，`K31` 挡着）；
② 一条命令**该不该**归后端（那是定框 `K28`/`K27`/`K33` 的活，本尺子不做语义审查）；
③ 迁一条要花多久。

# 🔴 分档规则只有一个住址 —— [`verdict_of`]，别在散文里复述一份

# 为什么表要从源码派生

`KR111D1` 逐字：「那张表要能**从源码派生**（同 `TS_FALLBACK_KEEPERS` 的做法），
哪天某一条真退完了，表跟着变；**表与源码对不上就红**。」

⇒ 本尺子里**手写**的只有三样，每一样都被现打的源码事实钉着：

| 手写的 | 为什么机器判不了 | 谁在钉它 |
|---|---|---|
| `SITES` 的「住址（文件 :: 函数）」 | 「这条命令的实现今天住在哪个函数里」是语义选择（薄转发壳一层套一层） | 函数找不到 ⇒ 红 |
| `SITES` 的「面」 | 同上 | 现打命中的针属于另一面 ⇒ 红 |
| `SITES` 的「后端对侧」 | 「哪条 daemon 原语算这条命令的对侧」是语义选择 | 点名的原语不在现打的 `SUBCOMMANDS`/`COMMANDS` 里 ⇒ 红 |

**判定本身一个字都不手写** —— 它由「那个函数体里现打命中了哪几类针」算出来。
⇒ 哪天有人把 `read_cc_bus_state` 真改走 `bus-list`，那处 `local_shell_read` 消失，
本尺子当场把它从「还有接线活」挪进「已完」，而 [`BASELINE`] 对不上 ⇒ 红，
逼下一个人把棘轮往下拧。**这是递减棘轮，不是一张快照。**

# ⚠ 射程，逐条如实写（`brief` 硬规则 12：给不出分母就别写全称）

1. **只看被点名的那一个函数体，不追调用链。** 追了的话 `run_list_query` 的体里有
   `connect_and_exec_cmd`，于是「走远端后端协议」会被读成「自己拼 shell」——
   两件事在字面上一模一样。⇒ 射程换成「谁被点名」，而点名是手写的（上表第一行）。
2. **针是闭集，住在 [`NEEDLES`]**，散文里不复述成员。它认不出「换个名字做同一件事」
   （例：把 `std::fs::read_to_string` 包一层叫 `slurp`）——**比没有强，别读成证明**。
3. **`fs::` 的处数不等于「还在自己读」。** 本尺子**不数 grep**：它只问「被点名的那个函数体里
   有没有」，逐处的定性写在 `evidence/K-R111-census.md` 里，由人读过。
4. **剥法**：`#[cfg(test)]` 块 · 注释 · 字符串字面量内容一律剥掉（[`prod_lines`]）。
   自检见 [`selftest_stripper`]。
5. **它不证明「后端那条对侧真的等价」** —— 只证明那条原语的**名字**今天在 daemon 的表里。
   等价要真跑两趟逐字段比，`K31` 挡着，本拍没跑。

# 怎么跑

    python3 evidence/K-R111-ruler.py                     # 人读
    python3 evidence/K-R111-ruler.py --json              # 机器读
    python3 evidence/K-R111-ruler.py --src-root <树>/src-tauri/src \
                                     --daemon-root <树>/remote-daemon-proto/src
    python3 evidence/K-R111-ruler.py --drop-baseline     # 阴性对照：把三档表整段拿掉

退出码：`0` = 表与源码对得上；`1` = 对不上（逐条印出哪一格）；`2` = 尺子自己坏了（CRASH）。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# ══════════════════════════════════════════════════════════════════════════
# 一 · 剥法（生产段）
# ══════════════════════════════════════════════════════════════════════════


def _scan(text: str) -> list[bool]:
    """逐字符扫，返回与原文等长的 mask（True = 生产字符）。

    认：`//` 行注释 · `/* */` 块注释（Rust 可嵌套）· `"…"` 与 `r#"…"#` 字符串
    （**内容**置 False，引号本身留 True）· `'x'` 字符字面量。

    🔴 **首版有一处真缺陷，逐字记下来别让它复活**：先认 `/*` 再认 `//`，于是文档注释里的
    `/**非全零**` 被当成块注释开头，从那一行起整份文件被吞掉 ——
    `usage.rs` 的生产段被读成 **64 行**（真值 211）。⇒ 必须**按字符扫、行注释优先**。
    同族第二处：不认字符字面量 ⇒ `'{'` 把大括号配对算歪，`cc_bus.rs` 的 `#[cfg(test)]`
    块提前 1748 行「闭合」，1000 多行测试代码被当成生产段。两处都由 [`selftest_stripper`] 钉着。
    """
    n = len(text)
    mask = [True] * n
    i = 0
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                mask[k] = False
            i = j
        elif c == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            mask[i] = mask[i + 1] = False
            j = i + 2
            while j < n and depth:
                if text[j] == "/" and j + 1 < n and text[j + 1] == "*":
                    depth += 1
                    mask[j] = mask[j + 1] = False
                    j += 2
                    continue
                if text[j] == "*" and j + 1 < n and text[j + 1] == "/":
                    depth -= 1
                    mask[j] = mask[j + 1] = False
                    j += 2
                    continue
                if text[j] != "\n":
                    mask[j] = False
                j += 1
            i = j
        elif c == "r" and i + 1 < n and text[i + 1] in '#"':
            m = re.match(r'r(#*)"', text[i:])
            if not m:
                i += 1
                continue
            close = '"' + m.group(1)
            j = text.find(close, i + len(m.group(0)))
            j = n if j < 0 else j + len(close)
            for k in range(i + len(m.group(0)), min(j, n)):
                if text[k] != "\n":
                    mask[k] = False
            i = j
        elif c == "'":
            # 字符字面量 `'x'` / `'\n'` / `'\u{1F600}'`；生命周期 `'a` 不匹配 ⇒ 原样放过。
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\.|[^'\\])'", text[i:])
            if m:
                for k in range(i + 1, i + len(m.group(0)) - 1):
                    mask[k] = False
                i += len(m.group(0))
            else:
                i += 1
        elif c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            for k in range(i + 1, min(j, n)):
                if text[k] != "\n":
                    mask[k] = False
            i = min(j + 1, n)
        else:
            i += 1
    return mask


def prod_lines(text: str) -> list[str]:
    """返回**与原文等行数**的生产段行表（注释 / 字符串内容 / `#[cfg(test)]` 块已抹平）。

    行数不变是承重的：住址要能按行号指回去。
    """
    mask = _scan(text)
    kept = "".join(ch if m else (" " if ch != "\n" else "\n") for ch, m in zip(text, mask))
    lines = kept.split("\n")
    i, n = 0, len(lines)
    while i < n:
        if lines[i].strip().startswith("#[cfg(test)]"):
            depth, started, j = 0, False, i
            while j < n:
                for ch in lines[j]:
                    if ch == "{":
                        depth += 1
                        started = True
                    elif ch == "}":
                        depth -= 1
                lines[j] = ""
                if started and depth <= 0:
                    break
                j += 1
            i = j + 1
        else:
            i += 1
    return lines


def fn_body(lines: list[str], name: str) -> tuple[int, list[str]] | None:
    """在生产段里按名字取一个函数体：签名行 → **大括号配对**到 depth 归零。

    🔴 **不用「下一个顶格行为界」那种粗取法**（`exec_site_registry::body_around` 是那一形）：
    本仓真实存在把 `{` 单独顶格写的签名 ——

        pub async fn list_local_session_accounts() -> Result<…, String>
        {

    顶格取法在那里**当场把函数体截成一行签名**，于是「它调没调后端」现打成「一根针都没中」。
    首跑真踩到了这一格（两条 `local_accounts.rs` 的命令），逐字记下来。
    ⚠ 依赖前置的剥法：注释 / 字符串 / 字符字面量里的大括号都已抹平。
    同名函数只取第一处。返回 `(1-based 签名行号, 体)`；找不到回 `None`。
    """
    pat = re.compile(r"(^|\s)fn\s+" + re.escape(name) + r"\b")
    for i, s in enumerate(lines):
        if not pat.search(s):
            continue
        out: list[str] = []
        depth, started = 0, False
        for j in range(i, len(lines)):
            out.append(lines[j])
            for ch in lines[j]:
                if ch == "{":
                    depth += 1
                    started = True
                elif ch == "}":
                    depth -= 1
            if started and depth <= 0:
                break
            # 保险丝：签名之后 200 行还没开括号 ⇒ 认不出，宁可短也别吞掉半份文件
            if not started and j - i > 200:
                break
        return i + 1, out
    return None


# ══════════════════════════════════════════════════════════════════════════
# 二 · 针（闭集，只有这一个住址）
# ══════════════════════════════════════════════════════════════════════════

#: `面 → 针`。**前三面 = 前端自己在做**；`后端` = 它把活交出去了。
#: 🔴 闭集只许有一个家：散文 / 报告 / 件文件里一律**只给住址、不复述成员**（`brief` 13b）。
NEEDLES: dict[str, tuple[str, ...]] = {
    "读面": ("File::open", "fs::read", "fs::read_to_string", "read_dir(", "fs::metadata"),
    "执行面": ("Command::new", "connect_and_exec_cmd", "local_shell_read", "exec_read("),
    "渲染面": ("CliSpec {", "payload::EnvOp", "payload::WrapSpec", "render_local_ccm"),
    "后端": (
        # ⚠ `run_query(` 而不是 `local_query::run_query` —— `list_local_accounts` 把
        #    `use …local_query::{run_query, …}` 放在**模块顶**，函数体里只剩裸调用。
        #    首跑因此把一条真·已完的命令现打成「一根针都没中」。
        "run_query(",
        "run_list_query",
        "daemon_route",
        "daemon_kill::daemon_kill",
        "daemon_send_keys::daemon_send_keys",
        "send_via_daemon",
        "online_via_daemon",
        "broadcast_via_daemon",
        "probe_account_usage",
        "client.call(",
    ),
}

#: 「前端自己在做」的那几面。`后端` 之外的全体 —— **现算，不写死**。
SELF_FACES: tuple[str, ...] = tuple(f for f in NEEDLES if f != "后端")


# ══════════════════════════════════════════════════════════════════════════
# 三 · 人群（14 个 id）与逐条住址
# ══════════════════════════════════════════════════════════════════════════

#: 乙一那 14 个能力 id。**由件 `K-R111` 给全**（＝ `K-R107` 普查 B1 档 19 个减去历史族 5 个）。
#: 本尺子不重推它，只核「这 14 个今天在 `parity_ledger::LEDGER` 里都还在」。
FOURTEEN: tuple[str, ...] = (
    "accounts.list",
    "accounts.session-accounts",
    "accounts.trust",
    "usage.aggregate",
    "usage.per-account",
    "subagent.load",
    "launch.render-cli",
    "launch.render-payload",
    "launch.send-into",
    "tmux.manage",
    "cc-bus.cockpit",
    "ccm.status",
    "session.launch",
    "relay.routing",
)

#: `命令 → (面, 住址文件, 被点名的函数, 后端对侧原语)`。
#:
#: - **面**：手写（语义选择），由「现打命中的针属于哪一面」核对，对不上就红。
#: - **被点名的函数**：手写。薄转发壳（`resume_history_session` → `resume_impl` → `launch_local`
#:   → …）里「实现到底住哪一层」机器判不了 ⇒ 由人点名，函数不存在就红。
#: - **后端对侧**：`None` = 今天没有；否则是一条 daemon 原语的**名字**，
#:   由现打的 `main.rs::SUBCOMMANDS` ＋ `inbound.rs::COMMANDS` 核，不在表里就红。
SITES: dict[str, tuple[str, str, str, str | None]] = {
    # ── accounts：远端走 `--list-accounts` 一族，本机走 sidecar 一次性查询
    "list_remote_accounts": ("后端", "accounts.rs", "list_remote_accounts", "--list-accounts"),
    "list_local_accounts": ("后端", "local_accounts.rs", "list_local_accounts", "--list-accounts"),
    "list_remote_session_accounts": (
        "后端",
        "accounts.rs",
        "list_remote_session_accounts",
        "--session-accounts",
    ),
    "list_local_session_accounts": (
        "后端",
        "local_accounts.rs",
        "list_local_session_accounts",
        "--session-accounts",
    ),
    "check_account_trust": ("后端", "accounts.rs", "check_account_trust", "--account-trust"),
    # ── usage
    "aggregate_usage_all": ("后端", "usage.rs", "aggregate_usage_all", "--usage"),
    "aggregate_remote_usage_all": (
        "后端",
        "remote_history.rs",
        "aggregate_remote_usage_all",
        "--usage",
    ),
    "account_usage": ("后端", "account_usage.rs", "account_usage", "oneshot-session"),
    "account_usage_local": ("后端", "account_usage.rs", "account_usage_local", "oneshot-session"),
    # ── subagent
    "load_subagent": ("后端", "subagent.rs", "query", "--list-subagents"),
    # ── launch 渲染面：两条命令都是**进程内渲染**，后端另有一份 `control/ccm/plan.rs`
    "render_ccm_launch": ("渲染面", "backend/control/launch_wire.rs", "render_ccm_launch", None),
    "render_launch_payload": (
        "渲染面",
        "backend/control/launch_wire.rs",
        "render_launch_payload",
        None,
    ),
    # ── launch.send-into
    "daemon_send_into": ("后端", "backend/control/daemon_launch.rs", "daemon_send_into", "launch"),
    # ── tmux
    "list_remote_tmux": ("执行面", "tmux.rs", "list_remote_tmux", None),
    "capture_remote_pane": ("执行面", "tmux.rs", "capture_remote_pane", "capture-pane"),
    "kill_remote_tmux": ("后端", "tmux.rs", "kill_remote_tmux", "kill"),
    "tmux_send_keys": ("后端", "tmux.rs", "tmux_send_keys", "launch"),
    # ── cc-bus
    "read_cc_bus_state": ("执行面", "cc_bus.rs", "read_cc_bus_state", None),
    "check_cc_bus_agent_online": ("执行面", "cc_bus.rs", "check_cc_bus_agent_online", "bus-list"),
    "read_cc_bus_inbox": ("执行面", "cc_bus.rs", "read_cc_bus_inbox", None),
    "cc_bus_send": ("后端", "cc_bus.rs", "cc_bus_send", "bus-send"),
    "cc_bus_broadcast": ("执行面", "cc_bus.rs", "cc_bus_broadcast", "bus-send"),
    "cc_bus_kill": ("执行面", "cc_bus.rs", "cc_bus_kill", "bus-kill"),
    "cc_bus_spawn": ("执行面", "cc_bus.rs", "cc_bus_spawn", None),
    # ── ccm.status
    "cc_integration_status": ("读面", "profile_installer.rs", "scan_profile", None),
    "local_ccm_entry_status": ("执行面", "ccm_probe.rs", "probe_binary_uncached", None),
    "probe_ccm_cli": ("执行面", "ccm_probe.rs", "probe_ccm_cli", None),
    # ── session.launch：三条最终都落到本机执行面那两处 spawn
    "resume_history_session": ("执行面", "launch.rs", "launch_local_posix_via", None),
    "new_local_session": ("执行面", "launch.rs", "launch_local_posix_via", None),
    "launch_remote_terminal": ("执行面", "launch.rs", "launch_powershell_window", None),
    # ── relay.routing：读的是 monitor 自己数据目录下那份中转凭据表
    "relay_routing_for": ("读面", "history.rs", "relay_rows_at", None),
}

#: 🔴 **递减棘轮的当前刻度** —— 量于主干 `b0953e5`（`git log --oneline -1` 现打）。
#:
#: 一条命令真退完了 ⇒ 现算的档变了 ⇒ 与本表对不上 ⇒ **红**，逼下一个人把刻度拧下来。
#: 反向也红：新长出一处自实现，同样对不上。
#: ⚠ 这不是「今天的答案」，是「上一次有人核过的答案」。
BASELINE: dict[str, str] = {
    "list_remote_accounts": "已完",
    "list_local_accounts": "已完",
    "list_remote_session_accounts": "已完",
    "list_local_session_accounts": "已完",
    "check_account_trust": "已完",
    "aggregate_usage_all": "已完",
    "aggregate_remote_usage_all": "已完",
    "account_usage": "已完",
    "account_usage_local": "已完",
    "load_subagent": "已完",
    "daemon_send_into": "已完",
    "kill_remote_tmux": "已完",
    "tmux_send_keys": "已完",
    "cc_bus_send": "已完",
    "capture_remote_pane": "还有接线活",
    "check_cc_bus_agent_online": "还有接线活",
    "cc_bus_broadcast": "还有接线活",
    "cc_bus_kill": "还有接线活",
    "render_ccm_launch": "不是接线",
    "render_launch_payload": "不是接线",
    "list_remote_tmux": "不是接线",
    "read_cc_bus_state": "不是接线",
    "read_cc_bus_inbox": "不是接线",
    "cc_bus_spawn": "不是接线",
    "cc_integration_status": "不是接线",
    "local_ccm_entry_status": "不是接线",
    "probe_ccm_cli": "不是接线",
    "resume_history_session": "不是接线",
    "new_local_session": "不是接线",
    "launch_remote_terminal": "不是接线",
    "relay_routing_for": "不是接线",
}

TIERS: tuple[str, ...] = ("已完", "还有接线活", "不是接线")


def verdict_of(hit_faces: set[str], counterpart_live: bool | None) -> str:
    """🔴 **分档规则的唯一住址。**

    - 前端**一处自己的实现都没有**（`SELF_FACES` 一针不中）⇒ `已完`。
    - 还有自实现，**且它点名的后端对侧今天真在 daemon 表里** ⇒ `还有接线活`。
    - 还有自实现，**而对侧不存在（`counterpart_live is None`）** ⇒ `不是接线`
      —— 要么先给后端补能力，要么先要一句产品判断。

    ⚠ `counterpart_live is False` 是**尺子坏了**（点名了一条 daemon 表里没有的原语），
    由调用方按红处理，不进本函数的三档。
    """
    if not (hit_faces & set(SELF_FACES)):
        return "已完"
    return "还有接线活" if counterpart_live else "不是接线"


def roll_up(cmd_verdicts: list[str]) -> str:
    """一个 id 的档 = 它名下那几条命令的**卷积**。

    - 全部 `已完` ⇒ `已完`
    - 有残留，且残留**全部**是 `还有接线活` ⇒ `还有接线活`（**纯接线**）
    - 有残留，且**至少一条**是 `不是接线` ⇒ `不是接线`（**不是纯接线**，劈开）
    """
    if all(v == "已完" for v in cmd_verdicts):
        return "已完"
    if any(v == "不是接线" for v in cmd_verdicts):
        return "不是接线"
    return "还有接线活"


# ══════════════════════════════════════════════════════════════════════════
# 四 · 现打：三张具名表（人群从源码派生）
# ══════════════════════════════════════════════════════════════════════════


def _tuple_table(text: str, const_name: str, pattern: str) -> list[tuple]:
    i = text.index(const_name)
    j = text.index("\n    ];", i)
    return re.findall(pattern, text[i:j], re.S)


def read_ledger(src: Path) -> dict[str, list[str]]:
    """`parity_ledger::LEDGER` → `能力 id → [命令名]`。**分母 = 它自己现打的条数。**"""
    t = (src / "parity_ledger.rs").read_text(encoding="utf-8")
    rows = _tuple_table(
        t, "const LEDGER", r'\(\s*"([a-z0-9_]+)",\s*"([a-z0-9.\-]+)",\s*Side::(\w+)\s*,?\s*\)'
    )
    out: dict[str, list[str]] = {}
    for cmd, cid, _side in rows:
        out.setdefault(cid, []).append(cmd)
    return out


def read_daemon_primitives(daemon: Path) -> tuple[list[str], list[str]]:
    """现打 daemon 的两张具名表：一次性子命令 ＋ 流式帧命令。**不抄名单。**"""
    m = (daemon / "main.rs").read_text(encoding="utf-8")
    i = m.index("const SUBCOMMANDS")
    j = m.index("\n];", i)
    subs = re.findall(r'"([^"]+)"', re.sub(r"//[^\n]*", "", m[i:j]))
    n = (daemon / "inbound.rs").read_text(encoding="utf-8")
    i = n.index("pub const COMMANDS")
    j = n.index("\n];", i)
    frames = re.findall(r'"([^"]+)"', re.sub(r"//[^\n]*", "", n[i:j]))
    return subs, frames


def read_read_surface(src: Path) -> list[tuple[str, str, int]]:
    """`local_read_surface_registry::REGISTERED` → `(文件, 类别, 处数)`。"""
    t = (src / "local_read_surface_registry.rs").read_text(encoding="utf-8")
    return [
        (f, k, int(n))
        for f, k, n in _tuple_table(
            t, "const REGISTERED", r'\(\s*\n\s*"([^"]+)",\s*\n\s*"([^"]+)",\s*\n\s*(\d+),'
        )
    ]


def read_exec_sites(src: Path) -> list[tuple[str, str]]:
    """`exec_site_registry::EXEC_SITES` → `(文件, 函数)`。**只管远端那个扼流点。**"""
    t = (src / "exec_site_registry.rs").read_text(encoding="utf-8")
    return _tuple_table(t, "const EXEC_SITES", r'\(\s*"([^"]+\.rs)",\s*"([A-Za-z0-9_]+)",\s*Origin::')


def read_spawn_sites(src: Path) -> list[tuple[str, str]]:
    """`write_site_registry::SPAWNS` → `(文件, 函数)`。**本机执行面**那把尺子。"""
    t = (src / "write_site_registry.rs").read_text(encoding="utf-8")
    return [
        (f, fn)
        for f, fn, _ in _tuple_table(
            t, "const SPAWNS", r'\(\s*\n?\s*"([^"]+)",\s*\n?\s*"([^"]+)",\s*\n?\s*"([^"]*)",'
        )
    ]


def read_ts_fallback(src: Path) -> tuple[list[tuple[str, str, int]], list[tuple[str, str]]]:
    """尺子A `TS_FALLBACK_KEEPERS` ＋ 尺子B `TS_FALLBACK_REACH`。

    ⚠ **两把尺子问的不是同一件事**（`K-R105`）：A 数「盘上还有谁提到它」，
    B 判「这条路今天还站不站在生产上」。**不许拿 A 的数说 B 的话。**
    """
    t = (src / "backend" / "control" / "launch_wire.rs").read_text(encoding="utf-8")
    keepers = [
        (sym, f, int(n))
        for sym, f, n in _tuple_table(
            t,
            "const TS_FALLBACK_KEEPERS",
            r'\(\s*\n\s*"([^"]+)",\s*\n\s*"([^"]+)",\s*\n\s*(\d+),',
        )
    ]
    reach = [
        (f, r)
        for f, _ex, r in _tuple_table(
            t, "const TS_FALLBACK_REACH", r'\(\s*\n\s*"([^"]+)",\s*\n\s*&\[(.*?)\],\s*\n\s*Reach::(\w+),'
        )
    ]
    return keepers, reach


# ══════════════════════════════════════════════════════════════════════════
# 五 · 量具自检
# ══════════════════════════════════════════════════════════════════════════


def selftest_stripper() -> list[str]:
    """★ 量具自检：剥法真的在剥，而且**两处栽过的坑各有一格**。

    不用「剥完更短」当自检（光靠剥注释就能满足，与剥对没剥对无关）——直接喂字面量看输出。
    """
    bad: list[str] = []

    def chk(name: str, got, want):
        if got != want:
            bad.append(f"{name}: 得 {got!r}，要 {want!r}")

    chk("行注释", [l.rstrip() for l in prod_lines("let a = 1; // x\nlet b = 2;")], ["let a = 1;", "let b = 2;"])
    chk("块注释", prod_lines("a/* x */b"), ["a       b"])
    # 🔴 回归格一：文档注释里的 `/**` **不是**块注释开头（首版栽在这里，吞掉半份文件）
    chk("doc 里的 /**", [l.strip() for l in prod_lines("/// a /**b**/ c\nlet z = 1;")], ["", "let z = 1;"])
    # 🔴 回归格二：字符字面量里的大括号不算大括号（首版栽在这里，`#[cfg(test)]` 提前闭合）
    src = "#[cfg(test)]\nmod t {\n  let c = '{';\n}\nfn real() {}"
    chk("字符字面量里的 {", [l for l in prod_lines(src) if l.strip()], ["fn real() {}"])
    # 字符串内容抹掉、引号留着（住址与行号不许漂）；针只许命中真代码，不许命中串里的字
    chk("字符串内容", prod_lines('let s = "fs::read";'), ['let s = "        ";'])
    chk("串里的针不算", "fs::read" in prod_lines('let s = "fs::read";')[0], False)
    # 反向：没有注释的文本一个字都不许动
    chk("不该动的", prod_lines("let x = 1;\nlet y = 2;"), ["let x = 1;", "let y = 2;"])
    # 行数守恒（住址要指得回去）
    txt = "// a\n/* b\nc */\nlet d = 1;\n"
    chk("行数守恒", len(prod_lines(txt)), len(txt.split("\n")))
    return bad


# ══════════════════════════════════════════════════════════════════════════
# 六 · 主体
# ══════════════════════════════════════════════════════════════════════════


def measure(src: Path, daemon: Path) -> dict:
    reds: list[str] = []
    ledger = read_ledger(src)
    subs, frames = read_daemon_primitives(daemon)
    primitives = set(subs) | set(frames)

    # ① 人群对账：14 个 id 都还在 LEDGER 里，命令集合与 SITES 逐条对得上
    cmds_by_id: dict[str, list[str]] = {}
    for cid in FOURTEEN:
        if cid not in ledger:
            reds.append(f"[人群] 能力 id `{cid}` 今天不在 `parity_ledger::LEDGER` 里了")
            continue
        cmds_by_id[cid] = sorted(ledger[cid])
    all_cmds = sorted(c for v in cmds_by_id.values() for c in v)
    missing = [c for c in all_cmds if c not in SITES]
    extra = [c for c in SITES if c not in all_cmds]
    for c in missing:
        reds.append(f"[人群] `LEDGER` 里多出一条命令 `{c}`，`SITES` 没登记 ⇒ 分不了档")
    for c in extra:
        reds.append(f"[人群] `SITES` 登记了 `{c}`，而 `LEDGER` 里今天没有它 ⇒ 申报了一处不存在的命令")

    # ② 逐条现打
    per_cmd: dict[str, dict] = {}
    cache: dict[str, list[str]] = {}
    for cmd in all_cmds:
        if cmd not in SITES:
            continue
        face, rel, fname, counterpart = SITES[cmd]
        p = src / rel
        if not p.is_file():
            reds.append(f"[住址] `{cmd}`：`{rel}` 不在盘上")
            continue
        if rel not in cache:
            cache[rel] = prod_lines(p.read_text(encoding="utf-8"))
        got = fn_body(cache[rel], fname)
        if got is None:
            reds.append(f"[住址] `{cmd}`：`{rel}::{fname}` 在生产段里找不到 ⇒ 住址馊了")
            continue
        lineno, body = got
        blob = "\n".join(body)
        hits = {f: [nd for nd in nds if nd in blob] for f, nds in NEEDLES.items()}
        hit_faces = {f for f, v in hits.items() if v}
        # 面对不上就红（手写那一列被现打核）
        self_hit = hit_faces & set(SELF_FACES)
        if face in SELF_FACES and face not in self_hit:
            reds.append(
                f"[面] `{cmd}`：登记为「{face}」，而 `{rel}::{fname}` 现打命中的是 "
                f"{sorted(hit_faces) or '（一根针都没中）'}"
                + ("（自实现那几面一面都没中 ⇒ 这一处像是退役了，面这一列要跟着改）" if not self_hit else "")
            )
        # ⚠ **刻意不加**「登记为『后端』却现打有自实现的针 ⇒ 红」那一条：
        #    那件事三档表已经咬过一次了（有自实现 ⇒ 判定不可能是 `已完` ⇒ 棘轮对不上）。
        #    加上它就是**同一个事实两副牙**，而两副牙里有一副在阴性对照
        #    （`--drop-baseline` ＋ 死值验刀①）那一格会替三档表挡枪 ——
        #    挡完之后「这张表到底有没有牙」就量不出来了。⇒ 一个事实只留一副牙。
        #    代价如实写：`face == "后端"` 这一格因此**没有独立的机检**，它只是个标签，
        #    承重的是判定本身。
        if not hit_faces:
            reds.append(f"[分不了档] `{cmd}`：`{rel}::{fname}` 里一根针都没中 —— 既不自实现也不调后端")
        # 后端对侧：点名的原语必须真在 daemon 表里
        live: bool | None
        if counterpart is None:
            live = None
        elif counterpart in primitives:
            live = True
        else:
            live = False
            reds.append(
                f"[对侧] `{cmd}` 点名的 daemon 原语 `{counterpart}` 不在现打的两张表里"
                f"（子命令 {len(subs)} 条 ＋ 帧 {len(frames)} 条）"
            )
        v = verdict_of(hit_faces, live)
        per_cmd[cmd] = {
            "面": face,
            "住址": f"{rel}::{fname}",
            "行": lineno,
            "命中": {f: v2 for f, v2 in hits.items() if v2},
            "后端对侧": counterpart,
            "档": v,
        }

    # ③ 卷积到 id
    per_id: dict[str, dict] = {}
    for cid, cmds in cmds_by_id.items():
        vs = [per_cmd[c]["档"] for c in cmds if c in per_cmd]
        per_id[cid] = {
            "命令数": len(cmds),
            "命令": {c: per_cmd[c]["档"] for c in cmds if c in per_cmd},
            "档": roll_up(vs) if vs else "判不了",
            "面": sorted({per_cmd[c]["面"] for c in cmds if c in per_cmd and per_cmd[c]["面"] != "后端"}),
        }

    return {
        "reds": reds,
        "per_cmd": per_cmd,
        "per_id": per_id,
        "cmds_by_id": cmds_by_id,
        "daemon": {"subcommands": subs, "frames": frames},
        "ledger_ids": len(ledger),
        "ledger_cmds": sum(len(v) for v in ledger.values()),
    }


def other_rulers(src: Path, wanted_files: set[str]) -> dict:
    """另外四把现成尺子的现打读数 ＋ **属于这 14 个 id 的那几条**。"""
    reg = read_read_surface(src)
    readers = [(f, k, n) for f, k, n in reg if k == "reader"]
    execs = read_exec_sites(src)
    spawns = read_spawn_sites(src)
    keepers, reach = read_ts_fallback(src)
    base = {Path(f).name for f in wanted_files}
    return {
        "读面": {
            "登记行数": len(reg),
            "reader 模块": len({f for f, _, _ in readers}),
            "reader 处数": sum(n for _, _, n in readers),
            "reader 逐条": [(f, n) for f, _, n in readers],
            "属于这14个的": [(f, n) for f, _, n in readers if Path(f).name in base],
        },
        "执行面·远端": {
            "EXEC_SITES 条数": len(execs),
            "属于这14个的": [f"{f}::{fn}" for f, fn in execs if f in base],
        },
        "执行面·本机": {
            "SPAWNS 条数": len(spawns),
            "属于这14个的": [f"{f}::{fn}" for f, fn in spawns if f in base],
        },
        "渲染面": {
            "尺子A TS_FALLBACK_KEEPERS 行数": len(keepers),
            "尺子A 逐行": [f"{s}@{f}×{n}" for s, f, n in keepers],
            "尺子B TS_FALLBACK_REACH 家数": len(reach),
            "尺子B On": sum(1 for _, r in reach if r == "OnProductionPath"),
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="K-R111 只读尺子")
    here = Path(__file__).resolve().parent.parent
    ap.add_argument("--src-root", default=str(here / "src-tauri" / "src"))
    ap.add_argument("--daemon-root", default=str(here / "remote-daemon-proto" / "src"))
    ap.add_argument("--json", action="store_true")
    ap.add_argument(
        "--drop-baseline",
        action="store_true",
        help="阴性对照：把三档表（BASELINE）整段拿掉 —— 拿掉之后一条都不该红",
    )
    a = ap.parse_args()
    src, daemon = Path(a.src_root), Path(a.daemon_root)

    bad = selftest_stripper()
    if bad:
        print("🔴 量具自检没过（剥法坏了，下面的读数一个都不许信）：")
        for b in bad:
            print("   ·", b)
        return 2

    m = measure(src, daemon)
    reds = list(m["reds"])

    baseline = {} if a.drop_baseline else BASELINE
    for cmd, info in sorted(m["per_cmd"].items()):
        want = baseline.get(cmd)
        if want is None:
            if baseline:
                reds.append(f"[棘轮] `{cmd}` 没有基线刻度 ⇒ 新长出来的命令，先核再登记")
            continue
        if info["档"] != want:
            arrow = "退役了 ✅ 把刻度拧下来" if info["档"] == "已完" else "回潮了 ⚠"
            reds.append(f"[棘轮] `{cmd}`：基线 `{want}` ≠ 现打 `{info['档']}` —— {arrow}")
    for cmd in baseline:
        if cmd not in m["per_cmd"]:
            reds.append(f"[棘轮] 基线里有 `{cmd}`，而今天现打不出它 ⇒ 命令没了还是住址馊了")

    others = other_rulers(src, {v[1] for v in SITES.values()})

    if a.json:
        print(
            json.dumps(
                {"per_cmd": m["per_cmd"], "per_id": m["per_id"], "尺子": others, "reds": reds},
                ensure_ascii=False,
                indent=2,
            )
        )
        return 1 if reds else 0

    print("═" * 78)
    print("K-R111 · 乙一那 14 个 id 今天还剩多少接线活 —— 现打")
    print("═" * 78)
    print(f"被测对象：{src}")
    print(f"        ：{daemon}")
    print(
        f"分母：`parity_ledger::LEDGER` 现打 {m['ledger_cmds']} 条命令 / {m['ledger_ids']} 个能力 id；"
        f"本尺子只看其中 {len(FOURTEEN)} 个 id / {len(m['per_cmd'])} 条命令"
    )
    print(
        f"后端分母：一次性子命令 {len(m['daemon']['subcommands'])} 条 ＋ 流式帧 "
        f"{len(m['daemon']['frames'])} 条（现打两张具名表）"
    )
    print()
    print("── id 逐条三档（档 = 名下各条命令的卷积，规则住 `roll_up`）" + "─" * 18)
    for tier in TIERS:
        ids = [i for i in FOURTEEN if m["per_id"].get(i, {}).get("档") == tier]
        print(f"\n【{tier}】{len(ids)} 个 id")
        for cid in ids:
            d = m["per_id"][cid]
            faces = "·".join(d["面"]) or "—"
            print(f"  · {cid}（{d['命令数']} 条命令 · {faces}）")
            for c, v in d["命令"].items():
                info = m["per_cmd"][c]
                cp = info["后端对侧"] or "后端今天没有对侧"
                mark = {"已完": "  ", "还有接线活": "→ ", "不是接线": "✗ "}[v]
                print(f"      {mark}{c:30} {v:6} {info['住址']}:{info['行']}  ⟨{cp}⟩")
    print()
    print("── 命令级计数（分母 = 上面那 31 条）" + "─" * 36)
    for tier in TIERS:
        n = sum(1 for v in m["per_cmd"].values() if v["档"] == tier)
        print(f"   {tier:6} {n:3} 条")
    print()
    print("── 四把现成尺子的现打读数" + "─" * 46)
    for k, v in others.items():
        print(f"  【{k}】")
        for kk, vv in v.items():
            print(f"      {kk}: {vv}")
    print()
    if reds:
        print(f"🔴 {len(reds)} 处对不上：")
        for r in reds:
            print("   ·", r)
        return 1
    print("✅ 表与源码对得上（分母与量法见上）" + ("；⚠ 基线已被 --drop-baseline 拿掉" if a.drop_baseline else ""))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # noqa: BLE001
        print(f"🔴 CRASH：{type(e).__name__}: {e}", file=sys.stderr)
        sys.exit(2)
