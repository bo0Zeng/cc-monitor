#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-P5f 摸底量具：**「读会话身份」今天的真人群**（换掉 PM 那把针的尺子）。

被测对象 = 本文件所在的那棵树（`Path(__file__).resolve().parents[1]`）。
跑法：`python3 evidence/K-P5f-identity-read-census.py`。零依赖、只读、不起任何进程
（除了 `git` 用来印被测树的尖与列跟踪文件）。

════════════════════════════════════════════════════════════════════════
本量具要治的病（`K-R18` 那一族：**量具枚举的集合 ≠ 标签命名的集合**）
════════════════════════════════════════════════════════════════════════

派工单给的起点是两个数：「含 `@ccm_sid` 的跟踪文件 **40** 份」「其中看起来像在读的
**13** 份」。那两个数枚举的是**一个字面串在文件里出现过**，而标签说的是
**一个语义角色**（「谁在读会话身份」）。两者差得很远：一份文件里可以有 20 处
`@ccm_sid` 而一处都不是读。

**换上来的尺子分三层，每层的口径都写在这里、不藏进代码：**

层 ①「**取回点**」——生产代码里**把身份从载体里取出来**的一处。
    对载体 C1（tmux 会话级 option `@ccm_sid`）来说，取回**只有一种句法**：
    向 tmux 要一个含 `#{@ccm_sid}` 的格式串（`tmux ls -F` / `display-message -p`），
    或对它跑 `show-option(s)`。⇒ **这一层机器枚举得出，而且是穷尽的**：
    绕过这两种句法就拿不到那个值。
    ⚠ 反过来不成立：命中这个句法的**未必**是取回（`set-titles-string
    '#{?@ccm_sid,…}'` 命中它，但那是叫 tmux 自己去派生标题，值不回到我们手里）
    ⇒ 所以本层是**上界**，逐行贴出来由人判，判完的落进 `REGISTRY`，两边对拍。

层 ②「**写入点**」——把身份**写进**载体的一处（`set-option … @ccm_sid`）。
    同样是句法穷尽的。可达性不是句法能判的 ⇒ `REGISTRY` 里人工标注并给理由。

层 ③「**消费点**」——拿着**已经取回**的那个值往下用的一处
    （`TmuxSession.sid` / `Probed.ccm_sid` / cc-bus 回复里的 `ccm_sid`）。
    🔴 **这一层机器只数得出上界**：字段名 `sid` 在全仓是个极常见的词，
    机器分不出「这个 `.sid` 是 `TmuxSession` 的那个」还是别人的。
    ⇒ 本层按**字段的持有类型**人点，机器只负责钉住锚点还在不在。
    **它对本件的意义**：换载体时**这一层不动** —— 这正是 `KP5FD3` 的代价边界。

⚠ **本量具不数「注释里提到 `@ccm_sid`」那一族**，也不数用户可见文案。
   它们既不读也不写载体；数进来就是把 `40` 那个病换个名字再犯一次。

════════════════════════════════════════════════════════════════════════
剥法的边界（诚实写出来，它给的是**上界**不是精确值）
════════════════════════════════════════════════════════════════════════

`strip_line_comments` 只剥**行首**的 `//` `///` `//!` `#` `*`（以及 Rust 的
`/* … */` 单行形）。剥不干净的三形：块注释跨行、行尾注释、`#[cfg(test)]`
不在文件末尾的那些文件。⇒ 层 ①② 的「生产」判定是**上界**。
本量具因此**把每一行都贴出来**：数错了看得见，不是只给一个总数。

测试文件按**文件名**认（`*.test.ts` / `*.vitest.ts` / `e2e/` 下的 `.sh`）
＋ Rust 按「该行是否落在文件里第一个顶格 `#[cfg(test)]` 之后」认。
⚠ 后者会**误伤**「判据与生产同住一文件、而生产函数写在测试模块之后」的那些文件
（本仓有这种形状）⇒ 两个数都印。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

NEEDLE = "@ccm_sid"
# 层①：取回的两种句法。`#{@ccm_sid}` / `#{?@ccm_sid,…}` 是 tmux 格式串；
# `show-option(s)` 是另一条取法（本仓刻意不用它，见 gate.rs 头注，但句法要收进来）。
RE_FMT = re.compile(r"#\{\??@ccm_sid[,}]")
RE_SHOWOPT = re.compile(r"show-options?")
# 层②：写入的唯一句法。
RE_SETOPT = re.compile(r"set-option")
# `set-titles-string` 命中层① 的句法，但它是**写**（叫 tmux 自己派生标题 C3）。
RE_SETTITLES = re.compile(r"set-titles-string")
# `@ccm_sid_expect` 是另一个 key（意图通道 A），不是本件的载体。
RE_EXPECT = re.compile(r"@ccm_sid_expect")


# ══════════════════════════════════════════════════════════════════════
# 登记表：人点的那一半。机器负责钉住「锚点还在、且恰好 N 处」。
# 每一行：(载体, 层, 住址, 锚点, 期望命中数, 面, 类, 它回答的是哪个问题)
# ══════════════════════════════════════════════════════════════════════
REGISTRY: list[dict] = [
    # ── 层① 取回点（C1）───────────────────────────────────────────────
    dict(
        id="R1", carrier="C1", layer="取回", kind="生产", face="跨机器+本机",
        path="src-tauri/src/tmux.rs",
        anchor='const TMUX_LS_FMT: &str = "#{session_name}',
        n=1,
        who="monitor",
        question="这个 tmux 会话跑的是哪个 sid（末列）",
        note="SSH 直跑 tmux ls 与 daemon 推来的 TmuxSessions.raw 都由 parse_tmux_ls 解这一份格式",
    ),
    dict(
        id="R2", carrier="C1", layer="取回", kind="生产", face="跨机器",
        path="src-tauri/src/tmux.rs",
        anchor='"#{session_windows}\\t#{@ccm_sid}"',
        n=1,
        who="monitor 拼串、远端 shell 判",
        question="这个 tmux 会话是不是我们的（Gate 2，只问**设没设**，不问是谁）",
        note="build_guarded_tmux_cmd 把 display-message 与动作折进同一个原子远端命令",
    ),
    dict(
        id="R3", carrier="C1", layer="取回", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/observe/watcher.rs",
        anchor='const TMUX_LS_FMT: &str = "#{session_name}',
        n=1,
        who="daemon",
        question="这个 tmux 会话跑的是哪个 sid（末列）",
        note="与 R1 是**双写点**（该文件头注逐字：「改此须同步 monitor」）",
    ),
    dict(
        id="R4", carrier="C1", layer="取回", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/gate.rs",
        anchor='const LIST_FMT: &str = "#{session_name}\\t#{session_id}\\t#{@ccm_sid}"',
        n=1,
        who="daemon（cc-bus `bus-list` 的 join_identity）",
        question="本机每个 tmux 会话跑的是哪个 sid（身份三元组）",
        note="**唯一一处把身份真的送上数据通道的**：inbound 命令 bus-list 的回复里带 ccm_sid",
    ),
    dict(
        id="R5", carrier="C1", layer="取回", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/gate.rs",
        anchor='const PROBE_FMT: &str = "#{session_id}\\t#{@ccm_sid}\\t#{session_windows}"',
        n=1,
        who="daemon（Gate 2 + identity_tag 的幂等对比）",
        question="这个 pane 所在会话是不是我们的 / 它现在打的是哪个 sid",
        note="identity_tag::tag 复用它做「值没变就不 set-option」的幂等判断",
    ),
    # ── 层② 写入点（C1）───────────────────────────────────────────────
    dict(
        id="W1", carrier="C1", layer="写入", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/identity_tag.rs",
        anchor='.args(["set-option", "-t", &target, "@ccm_sid", sid])',
        n=1,
        who="daemon",
        question="（写）事实通道 B：这个会话确实在跑这个 sid",
        note="生产可达（watcher.rs::process_session_added 唯一调用点）",
    ),
    dict(
        id="W2", carrier="C1", layer="写入", kind="生产·不可达", face="daemon 本地",
        path="remote-daemon-proto/src/control/launch.rs",
        anchor='let _ = tmux(&["set-option", "-t", &t, "@ccm_sid", sid]);',
        n=1,
        who="daemon launch 命令",
        question="（写）请求里带来的 ccm_sid",
        note="生产不可达：monitor 侧两个 launch_args 生产调用方都传 None（本量具现打）",
    ),
    dict(
        id="W3", carrier="C1", layer="写入", kind="生产", face="跨机器+本机",
        path="src/session-backend.ts",
        anchor="@ccm_sid ${ccmSid} 2>/dev/null || true) && `",
        n=1,
        who="前端 TS 兜底渲染器（L2 路）",
        question="（写）建会话时把计划里的 ccmSid 直写事实通道",
        note="生产可达（planResumeTmux 会给 ccmSid）",
    ),
    dict(
        id="W4", carrier="C3", layer="写入", kind="生产", face="跨机器",
        path="shared/ccm",
        anchor="tmux set-option set-titles-string '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}'",
        n=1,
        who="ccm",
        question="（写）叫 tmux 自己从 C1 派生窗口标题 C3",
        note="命中层①的句法但**不是取回**：值不回到我们手里",
    ),
    dict(
        id="W5", carrier="C3", layer="写入", kind="生产", face="跨机器+本机",
        path="src/session-backend.ts",
        anchor="set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null",
        n=1,
        who="前端 TS 兜底渲染器",
        question="（写）同 W4",
        note="同 W4",
    ),
    dict(
        id="W6", carrier="C3", layer="写入", kind="生产·不可达", face="跨机器",
        path="remote-daemon-proto/src/control/launch.rs",
        anchor='"ccm-rbind-#{@ccm_sid}"',
        n=1,
        who="daemon launch 命令",
        question="（写）同 W4",
        note="与 W2 同因不可达",
    ),
    # ── 层③ 消费点：拿着**已取回**的值往下用（换载体时这一层不动）─────
    dict(
        id="U1", carrier="C1", layer="消费", kind="生产", face="跨机器+本机",
        path="src-tauri/src/tmux.rs",
        anchor="pub sid: Option<String>,",
        n=1,
        who="monitor（TmuxSession 的 sid 字段，ts-rs 导出到前端）",
        question="取回之后那个值住在哪",
        note="载体换了它也不用换：它装的是「值」，不是「值从哪来」",
    ),
    dict(
        id="U2", carrier="C1", layer="消费", kind="生产", face="跨机器+本机",
        path="src-tauri/src/ssh_source.rs",
        anchor=".any(|s| s.sid.as_deref() == Some(sid))",
        n=1,
        who="monitor（tmux_origin_for_sid ⇒ 灰灯判定）",
        question="claude 死了之后，那个 tmux 还在不在",
        note="🔴 **这一格在 claude 已经退出之后才问** —— 见交回节对 K-R16 的答复",
    ),
    dict(
        id="U3", carrier="C1", layer="消费", kind="生产", face="跨机器+本机",
        path="src/tmux-sessions.ts",
        anchor="s.sid === sid && isClaudeTmuxCommand(s.command)",
        n=2,
        who="前端 findClaudeTmux（`:52`）与 willFallBackToCwd（`:93`）",
        question="目标 sid 此刻活在哪个 tmux",
        note="锚点恰好 2 处 —— 第一版我写了 1，机器当场逮住（经过留着）",
    ),
    dict(
        id="U4", carrier="C1", layer="消费", kind="生产", face="跨机器+本机",
        path="src/tmux-sessions.ts",
        anchor="s.sid === sid && !isClaudeTmuxCommand(s.command)",
        n=1,
        who="前端 findIdleTmux",
        question="目标 sid 的**空** tmux（claude 已退、容器还在）",
        note="🔴 同 U2：这一格的前提就是 claude 已经不在了",
    ),
    dict(
        id="U5", carrier="C1", layer="消费", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/gate.rs",
        anchor="gate_core::gate2(name, Some(&p.ccm_sid))",
        n=2,
        who="daemon（send-keys 与 kill 各一次）",
        question="这个会话是不是我们的（只看**设没设**）",
        note="Gate 2 只要 presence，不要值",
    ),
    dict(
        id="U6", carrier="C1", layer="消费", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/cc_bus.rs",
        anchor='o.insert("ccm_sid".to_string(), sid);',
        n=1,
        who="daemon（bus-list 回复）",
        question="把身份放上数据通道",
        note="🔴 今天**唯一**一处「身份已经在数据通道上」的先例",
    ),
    dict(
        id="U7", carrier="C1", layer="消费", kind="生产", face="daemon 本地",
        path="remote-daemon-proto/src/control/identity_tag.rs",
        anchor="if probed.ccm_sid == sid {",
        n=1,
        who="daemon（打标幂等）",
        question="现在打着的是不是已经是这个 sid",
        note="",
    ),
]


def sh(*args: str) -> str:
    # ⚠ `core.quotepath=false`：仓里有中文文件名，默认 git 会把它转义成 `"\351\241..."`，
    #   照那个串去开文件必定 FileNotFoundError（本量具第一版就是这样炸的，经过留着）。
    return subprocess.run(
        ("git", "-c", "core.quotepath=false") + args[1:] if args[0] == "git" else args,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    ).stdout


def tracked() -> list[str]:
    return [p for p in sh("git", "ls-files").splitlines() if p]


def strip_line_comments(text: str, path: str) -> list[tuple[int, str]]:
    """回 (1-based 行号, 行) —— 只留「看起来不是纯注释」的行。剥法边界见模块头注。"""
    out: list[tuple[int, str]] = []
    for i, raw in enumerate(text.splitlines(), 1):
        s = raw.strip()
        if not s:
            continue
        if s.startswith(("//", "///", "//!", "*", "/*")):
            continue
        if path.endswith((".sh", "ccm", ".py")) and s.startswith("#"):
            continue
        out.append((i, raw))
    return out


def rust_test_boundary(text: str) -> int:
    """文件里第一个**顶格** `#[cfg(test)]` 的行号；没有 ⇒ 一个很大的数。"""
    for i, raw in enumerate(text.splitlines(), 1):
        if raw.startswith("#[cfg(test)]"):
            return i
    return 10**9


def is_test_file(path: str) -> bool:
    return (
        path.endswith((".test.ts", ".vitest.ts"))
        or path.startswith("e2e/")
        or "/fixtures/" in path
    )


def is_evidence(path: str) -> bool:
    """`evidence/` 下的 `.py` 是**量具**，既不是生产也不是判据。

    ⚠ 声明为排除项：不排它，本量具会把**别人的量具里的字符串**数成生产
    （现打：`kg3-c1-cuts.py` 4 行 · `K-P5-Bx-seam-census.py` 2 行会混进来）。
    """
    return path.startswith("evidence/")


def classify(path: str, line: str) -> str:
    """把一行含 `@ccm_sid` 的行分到层里。返回层名或 '—'（既不读也不写）。"""
    if RE_EXPECT.search(line) and not re.search(r"@ccm_sid(?!_)", line):
        return "—（另一个 key：_expect 意图通道 A）"
    if RE_SETTITLES.search(line):
        return "写入（派生 C3 标题）"
    if RE_SETOPT.search(line) and re.search(r"@ccm_sid(?!_)", line):
        return "写入"
    if RE_FMT.search(line) or RE_SHOWOPT.search(line):
        return "取回"
    return "—"


def main() -> int:
    head = sh("git", "rev-parse", "HEAD").strip()
    branch = sh("git", "rev-parse", "--abbrev-ref", "HEAD").strip()
    print(f"被测树 = {ROOT}")
    print(f"量于提交 = {head}  分支 = {branch}")
    print(f"跟踪文件分母 = {len(tracked())} 份")
    print()

    # ── §A 旧尺子：能不能复打出 40 / 13 ────────────────────────────────
    print("=" * 72)
    print("§A 派工单那两个数（40 / 13）在 14 个作用域上的复打尝试")
    print("=" * 72)
    scopes = [
        ("全仓跟踪文件 · 针 `@ccm_sid`", ["--", "."]),
        ("全仓跟踪文件 · 针 `ccm_sid`（去掉 @）", None),
        ("排 *.md", ["--", ".", ":!*.md"]),
        ("排 *.md 排 e2e/", ["--", ".", ":!*.md", ":!e2e/*"]),
        ("只 *.rs", ["--", "*.rs"]),
        ("只 *.ts", ["--", "*.ts"]),
        ("*.rs + *.ts", ["--", "*.rs", "*.ts"]),
        ("*.rs + *.ts 排测试文件名", ["--", "*.rs", "*.ts", ":!*.test.ts", ":!*.vitest.ts"]),
        ("src-tauri/src", ["--", "src-tauri/src"]),
        ("remote-daemon-proto/src", ["--", "remote-daemon-proto/src"]),
        ("src", ["--", "src"]),
        ("shared", ["--", "shared"]),
        ("四棵树合计", ["--", "src-tauri/src", "remote-daemon-proto/src", "src", "shared"]),
    ]
    for label, spec in scopes:
        if spec is None:
            n = len(sh("git", "grep", "-l", "ccm_sid").splitlines())
        else:
            n = len(sh("git", "grep", "-l", NEEDLE, *spec).splitlines())
        flag = "  ← 40?" if n == 40 else ""
        print(f"  {n:>4} 份   {label}{flag}")
    # 「看起来像在读」那一格
    for label, spec in [
        ("全仓", ["--", "."]),
        ("*.rs + *.ts", ["--", "*.rs", "*.ts"]),
        ("排 *.md 排 e2e/", ["--", ".", ":!*.md", ":!e2e/*"]),
    ]:
        files = sh("git", "grep", "-l", NEEDLE, *spec).splitlines()
        hit = 0
        for f in files:
            t = (ROOT / f).read_text(encoding="utf-8", errors="replace")
            if re.search(r"show-options?|display-message|#\{@", t):
                hit += 1
        flag = "  ← 13?" if hit == 13 else ""
        print(f"  {hit:>4} 份   其中「附近有 show-option/display-message/#{{@」· {label}{flag}")
    print()
    print("  ⇒ **一个作用域都没复打出 40 或 13**（最近的是 41 与 17）。")
    print("     派工单没给作用域 ⇒ 那两个数按 brief 第 12 条不可复打，本件不沿用。")
    print()

    # ── §B 新尺子：逐行贴出来 ────────────────────────────────────────
    print("=" * 72)
    print("§B 新尺子层①② —— 每一条含 `@ccm_sid` 的**非纯注释**行，逐行分类")
    print("    （分母 = 全仓跟踪文件里含该串的行；本表把每一行都印出来）")
    print("=" * 72)
    buckets: dict[str, list[str]] = {}
    total_lines = 0
    for p in tracked():
        fp = ROOT / p
        if not fp.is_file():
            continue
        try:
            text = fp.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if NEEDLE not in text:
            continue
        boundary = rust_test_boundary(text) if p.endswith(".rs") else 10**9
        for ln, line in strip_line_comments(text, p):
            if NEEDLE not in line:
                continue
            total_lines += 1
            layer = classify(p, line)
            if layer == "—":
                continue
            if is_evidence(p):
                kind = "量具（排除项）"
            elif is_test_file(p):
                kind = "测试文件"
            elif p.endswith(".md"):
                kind = "文档"
            elif p.endswith(".rs") and ln >= boundary:
                kind = "判据/测试段"
            else:
                kind = "生产段"
            buckets.setdefault(f"{layer} · {kind}", []).append(
                f"      {p}:{ln}  {line.strip()[:110]}"
            )
    print(f"  含 `@ccm_sid` 的**非纯注释**行合计 = {total_lines} 行（这是分母）")
    print()
    for key in sorted(buckets):
        rows = buckets[key]
        print(f"  [{key}]  {len(rows)} 行")
        for r in rows:
            print(r)
        print()

    # ── §C 登记表对拍 ───────────────────────────────────────────────
    print("=" * 72)
    print("§C 登记表（人点的那一半）· 机器只钉「锚点还在、且恰好 N 处」")
    print("=" * 72)
    bad = 0
    for r in REGISTRY:
        fp = ROOT / r["path"]
        if not fp.is_file():
            print(f"  ✗ {r['id']}  文件不在：{r['path']}")
            bad += 1
            continue
        text = fp.read_text(encoding="utf-8", errors="replace")
        got = text.count(r["anchor"])
        ok = got == r["n"]
        bad += 0 if ok else 1
        mark = "✓" if ok else "✗"
        print(
            f"  {mark} {r['id']:<3} {r['carrier']} {r['layer']:<2} {r['kind']:<10} "
            f"{r['face']:<12} 锚点 {got}/{r['n']}  {r['path']}"
        )
        print(f"        谁：{r['who']}")
        print(f"        它回答：{r['question']}")
        if r["note"]:
            print(f"        ⚠ {r['note']}")
    print()

    # ── §D 分开数 ───────────────────────────────────────────────────
    print("=" * 72)
    print("§D `KP5FD1` 要的那几刀（分母 = 上面的登记表，不是文件数）")
    print("=" * 72)

    def cnt(**kw) -> int:
        n = 0
        for r in REGISTRY:
            if all(kw[k] in r[k] if isinstance(kw[k], str) else r[k] == kw[k] for k in kw):
                n += 1
        return n

    print(f"  层① 取回点（生产）        = {cnt(layer='取回')} 处")
    print(f"  层② 写入点（生产，含不可达）= {cnt(layer='写入')} 处"
          f"（其中生产不可达 {sum(1 for r in REGISTRY if r['layer']=='写入' and '不可达' in r['kind'])} 处）")
    print(f"  层③ 消费点（人点，生产）  = {cnt(layer='消费')} 处")
    print()
    print("  按面切（登记表全体）：")
    for face in ("daemon 本地", "跨机器+本机", "跨机器"):
        print(f"    {face:<12} = {sum(1 for r in REGISTRY if r['face'] == face)} 处")
    print()
    print("  按载体切：")
    for c in ("C1", "C3"):
        print(f"    {c} = {sum(1 for r in REGISTRY if r['carrier'] == c)} 处")
    print()
    print("  🔴 层① 里，**真在问「是谁」**的与**只问「设没设」**的要分开：")
    who_is = [r["id"] for r in REGISTRY if r["layer"] == "取回" and "是不是我们的" not in r["question"]]
    is_ours = [r["id"] for r in REGISTRY if r["layer"] == "取回" and "是不是我们的" in r["question"]]
    print(f"     问「是谁」：{who_is}  ⇒ {len(who_is)} 处")
    print(f"     只问「设没设」（Gate 2 归属证明）：{is_ours}  ⇒ {len(is_ours)} 处")
    print()

    # ── §E CCM_LAUNCH_ID 的消费者（K-P5d 那个数，本拍重打）───────────
    print("=" * 72)
    print("§E `CCM_LAUNCH_ID` 今天的消费者（K-P5d 09-02 18:01 报过零，本拍重打）")
    print("=" * 72)
    hits = sh("git", "grep", "-n", "CCM_LAUNCH_ID").splitlines()
    print(f"  全仓跟踪文件里 `CCM_LAUNCH_ID` 命中 {len(hits)} 行，逐行：")
    for h in hits:
        print(f"      {h[:150]}")
    print()

    # ── §F 那道文档闸（KP5FD2 要点名的 PM 独占面）──────────────────
    print("=" * 72)
    print("§F 往 wire 帧加一个字段会撞上的那道闸（`KP5FD2` 的面清单）")
    print("=" * 72)
    guard = ROOT / "remote-daemon-proto/src/protocol_doc_guard.rs"
    g = guard.read_text(encoding="utf-8", errors="replace")
    for fn in (
        "every_wire_field_appears_in_the_protocol_doc",
        "every_wire_frame_kind_has_a_row_in_the_frame_table",
        "wire_rs_has_no_serde_rename_on_fields",
        "every_inbound_command_appears_in_the_protocol_doc",
        "every_command_payload_field_appears_in_its_own_doc_section",
    ):
        print(f"  {'✓' if f'fn {fn}(' in g else '✗'} protocol_doc_guard::{fn}")
    wire = (ROOT / "remote-daemon-proto/src/wire.rs").read_text(encoding="utf-8", errors="replace")
    beg = wire.find("    SessionAdded {")
    end = wire.find("\n    },", beg)
    body = wire[beg:end]
    print(f"  SessionAdded 变体体 = {len(body)} 字节；"
          f"其中 `skip_serializing_if = \"Option::is_none\"` = {body.count('Option::is_none')} 处"
          f"（= additive 先例数）")
    doc = ROOT / "doc/IPC-PROTOCOL.md"
    d = doc.read_text(encoding="utf-8", errors="replace")
    sec = d.find("## 10. 远端 daemon wire 协议")
    sec_end = d.find("\n## ", sec)
    print(f"  doc/IPC-PROTOCOL.md §10 区间 = {sec_end - sec} 字节（判据要求 > 8000）")
    print()

    print("=" * 72)
    print(f"登记表对拍：{'全绿' if bad == 0 else f'{bad} 条不合'}")
    print("=" * 72)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
