#!/usr/bin/env python3
"""K-R37 · `remote-daemon-proto/src` 这棵**从来没被打开过的树**的普查量具（09-06 实现方现打）。

# 它回答什么

`scanning_guard_registry::every_registry_guard_keeps_its_reverse_half` 的 `scan_tree!`
实参上一版只有 `src-tauri/src` 一棵树。本量具在**扩射程之前**把三件事量出来：

1. `D1①` **分母怎么画** —— 件文件 `§0` 那四个数是四种画法，本量具把它们逐个复打，
   再加上「这条元判据**真正**会怎么圈人」的那两种画法（文件级 / 判据级）；
2. `D1②` 那 **80** 条 `const X: &[` 的**逐条判词**（登记表 / skip 表 / 形态表 /
   needle 表 / 非判据），口径逐字照 `K-R33` `D1②`（见 `VERDICT_RUBRIC`）；
3. `D1③` 「**是登记表、而且没有反向那半**」的有几条 —— 分**概念**与**机检**两栏，
   两栏差得很远，而差在哪里正是本件买到 / 没买到的东西。

# 🔴 它模拟的是判据本体，不是另写一把尺子

`test_regions` / `test_source` / `test_attr_chunks` / `guard_fn_item` /
`declares_a_guard_table` / `REVERSE` 六处**逐条照 Rust 那一份改写**（住址逐个写在
各自的 docstring 里）。改了那边**不会**自动改这边 —— 拿本量具的数说事之前先对一眼。

# 🔴 被测对象是谁：由 `--root` 给，默认 = 本文件所在工作树的仓根

⚠ 报读数时**连 `--root` 一起写**：同一条命令在不同工作树上跑，答案不同，
而两次输出长得一模一样（`brief` 12：量具住址要能唯一定位到那一份 + 它指向哪棵树）。

# 跑法

    python3 evidence/K-R37-daemon-tree-census.py              # 量工作树此刻的盘面
    python3 evidence/K-R37-daemon-tree-census.py --at 1fd6335 # 量某个提交（D1 的分母就该这么钉）
    python3 evidence/K-R37-daemon-tree-census.py --list       # 逐条判词（D1② 的产物）
"""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys

# ── 与 Rust 那一份对齐的六处口径 ────────────────────────────────────────────

#: `K-R33` 量具用的同一个 grep 口径（件文件 `§0` 那 80 条用的是更窄的
#: `const [A-Z_0-9]+: &\[`，两者在这棵树上同值 —— 输出里印出来对账）。
GREP = re.compile(r"const\s+[A-Za-z0-9_]+\s*:\s*&\[")
#: 「真的是一条声明」：行首锚定。差集 = 注释 / 字符串里提到一个声明的针。
DECL = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))? )?const\s+([A-Za-z0-9_]+)\s*:\s*&\[")
#: 件文件 `§0` 那一行 `grep -rhoE` 用的窄口径，单独复打一次。
NARROW = re.compile(r"const [A-Z_0-9]+: &\[")

#: `scanning_guard_registry.rs::TABLE_DECLS` 的**第二份字面量**（唯一住址在那边）。
CLOSED_SET = ("const REGISTERED:", "const SITES:", "const SCHEDULING_SITES:", "const FORMS:")
#: `scanning_guard_registry.rs::REVERSE` 的第二份字面量（同上）。
REVERSE = ("assert_eq!(", "已经不在了", "已经没有")

TREES = ("src-tauri/src", "remote-daemon-proto/src")
DAEMON = "remote-daemon-proto/src"
#: `scan_tree!` 按 `file!()` 摘掉的那一份。
GUARD_SELF = "src-tauri/src/scanning_guard_registry.rs"

VERDICT_RUBRIC = """判词口径（逐字照 `K-R33` `D1②`，不许在这里改写）：
  登记表   = 表里每一行对应盘上一个**真实存在的东西**（文件 / 调用点 / 事实），
             表腐了（登记了却已经没有）**也该红** ⇒ 这一族才是本元判据管得着的
  needle 表 = 拿去在文本里**搜**的串
  skip 表   = 豁免 / 排除 / 白名单
  形态表   = 概念闭集（档位、类别、合法取值、扫描面清单），不直接当针
  非判据表 = 住在**生产段**、被应用消费的常量表
⚠ 诚实边界（同 `K-R33`）：`skip 表` 与 `形态表` 里有一部分**同样会腐**，
   判成「不是登记表」说的是**这条元判据管不管得着**，不是「它们不会腐」。"""


# ── 六处口径的逐条改写 ──────────────────────────────────────────────────────


def test_regions(src: str) -> str:
    """照 `scanning_guard_registry.rs::test_regions`（文件级那一层喂的就是它）。

    ⚠ 它按 `#[cfg(test)]` 起、`\\n}\\n` 止，且**每命中一次就再往后找一次**
    ⇒ 一份文件里出现两次会把区段拼两份。本量具只用 `in`（存在性），翻倍不影响判定。
    """
    out, i = [], 0
    while True:
        j = src.find("#[cfg(test)]", i)
        if j < 0:
            break
        e = src.find("\n}\n", j)
        out.append(src[j : (len(src) if e < 0 else e)])
        i = j + len("#[cfg(test)]")
    return "\n".join(out)


def test_module_ranges(src: str):
    """照 `guard_core::test_module_ranges`（`test_source` 的实现）。

    开界 `\\n#[cfg(`，且**紧接着那一行**必须是 `mod X {`；闭界 `\\n}`。
    """
    out, i = [], 0
    while True:
        rel = src.find("\n#[cfg(", i)
        if rel < 0:
            return out
        j = rel
        attr_start = j + 1
        k = src.find("\n", attr_start)
        attr_end = len(src) if k < 0 else k
        attr = src[attr_start:attr_end]
        mod_start = min(attr_end + 1, len(src))
        k2 = src.find("\n", mod_start)
        mod_end = len(src) if k2 < 0 else k2
        mod_line = src[mod_start:mod_end].strip()
        # `cfg_is_test_only`：属性里只写了 test。够用的近似 —— 本树上全是 `#[cfg(test)]`。
        is_test_mod = (
            attr.strip() == "#[cfg(test)]"
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        e = src.find("\n}", j)
        if e < 0:
            out.append((j, len(src)))
            return out
        end = e + len("\n}")
        out.append((j, end))
        i = end


def test_source(src: str) -> str:
    """照 `guard_core::test_source`（判据级那一层喂的就是它）。"""
    return "".join(src[a:b] for a, b in test_module_ranges(src))


def test_attr_chunks(src: str):
    """照 `guard_core::test_attr_chunks`：整行 trim 之后逐字等于 `#[test]` 才是边界。"""
    anchor = "#[te" + "st]"
    out, cur = [], []
    for line in src.split("\n"):
        if line.strip() == anchor:
            if cur:
                out.append("\n".join(cur))
                cur = []
            continue
        cur.append(line)
    out.append("\n".join(cur))
    return out


def guard_fn_item(chunk: str):
    """照 `scanning_guard_registry.rs::guard_fn_item`：跳过空行 / 属性行 / 注释行之后
    紧接着必须是 `fn <名字>`；收尾靠**同缩进**的 `}`。"""
    lines = chunk.split("\n")
    head = None
    for i, l in enumerate(lines):
        t = l.lstrip()
        if not (t == "" or t.startswith("#") or t.startswith("//")):
            head = i
            break
    if head is None:
        return None
    first = lines[head]
    indent = first[: len(first) - len(first.lstrip())]
    t = first.lstrip()
    if not t.startswith("fn "):
        return None
    name = ""
    for c in t[len("fn ") :]:
        if c.isalnum() or c == "_":
            name += c
        else:
            break
    if not name:
        return None
    close = indent + "}"
    end = len(lines)
    for k, l in enumerate(lines[head:]):
        if l == close:
            end = head + k + 1
            break
    return name, "\n".join(lines[head:end])


def declares_a_guard_table(text: str) -> bool:
    """照 `scanning_guard_registry.rs::declares_a_guard_table`。"""
    return any(d in text for d in CLOSED_SET)


def guards_declaring_a_table(test_src: str):
    """照 `scanning_guard_registry.rs::guards_declaring_a_table`。"""
    out = []
    for c in test_attr_chunks(test_src):
        it = guard_fn_item(c)
        if it and declares_a_guard_table(it[1]):
            out.append(it)
    return out


# ── 语料来源 ────────────────────────────────────────────────────────────────


class Tree:
    """工作树盘面（`at=None`）或某个提交（`at=<ref>`）—— 两侧同一个读法。"""

    def __init__(self, root: pathlib.Path, at: str | None):
        self.root, self.at = root, at
        self._cache: dict[str, str] = {}
        if at:
            r = subprocess.run(
                ["git", "-C", str(root), "ls-tree", "-r", "--name-only", at],
                capture_output=True, text=True, check=True,
            )
            self._names = [x for x in r.stdout.split("\n") if x.endswith(".rs")]

    def label(self) -> str:
        return f"{self.root}  @ {self.at or '工作树盘面（未提交的改动也算）'}"

    def list_rs(self, subs):
        if self.at:
            cand = [n for n in self._names if any(n.startswith(s + "/") for s in subs)]
        else:
            cand = []
            for sub in subs:
                d = self.root / sub
                if d.is_dir():
                    cand += [p.relative_to(self.root).as_posix() for p in d.rglob("*.rs")]
        return sorted(cand)

    def read(self, rel: str) -> str:
        if rel in self._cache:
            return self._cache[rel]
        if self.at:
            t = subprocess.run(
                ["git", "-C", str(self.root), "show", f"{self.at}:{rel}"],
                capture_output=True, text=True, check=True,
            ).stdout
        else:
            t = (self.root / rel).read_text(encoding="utf-8", errors="replace")
        self._cache[rel] = t
        return t


# ── D1② 逐条判词 ───────────────────────────────────────────────────────────
#
# 🔴 **手填的一栏，不是算出来的** —— 每条都是把那条声明连同它上方的 `///` 注释、
#    以及用它的那条断言一起读过之后写的。键 = `路径:行号`（钉在 `--at 1fd6335` 上；
#    行号会漂，所以第三元把**名字**一起写着，对不上时按名字认，别按行号认）。
VERDICTS: dict[str, tuple[str, str, str]] = {
    # 〔remote-daemon-proto/src/agent_boundary_guard.rs〕
    "remote-daemon-proto/src/agent_boundary_guard.rs:46": ("CORE_FILES", "登记表", "已宣称通用的文件清单（相对 `src/`），每行一个真实文件；配递减/递增棘轮"),
    "remote-daemon-proto/src/agent_boundary_guard.rs:144": ("KNOWN_DEBT", "skip 表", "欠账豁免（今天空表），性质是「命中了但先放过」"),
    "remote-daemon-proto/src/agent_boundary_guard.rs:166": ("FROZEN_COMPAT", "skip 表", "冻结兼容豁免，四元组带解锁条件"),
    "remote-daemon-proto/src/agent_boundary_guard.rs:297": ("EVER_DECLARED_CORE", "登记表", "`CORE_FILES` 的历史全集，棘轮的另一半；每行一个真实文件"),
    # 〔remote-daemon-proto/src/agent_locality_guard.rs〕
    "remote-daemon-proto/src/agent_locality_guard.rs:102": ("HOMES", "登记表", "文件树里真实存在的 agent 家前缀，与 `agents::REGISTRY` 双向对拍"),
    "remote-daemon-proto/src/agent_locality_guard.rs:117": ("FIXTURE_HOMES", "skip 表", "夹具家豁免，带天花板与幽灵检查"),
    "remote-daemon-proto/src/agent_locality_guard.rs:145": ("KIND_DISPATCH_SITES", "登记表", "通用层里真实的 kind 派发点，逐条登记"),
    "remote-daemon-proto/src/agent_locality_guard.rs:181": ("NOT_AGENT_KNOWLEDGE", "skip 表", "判据看走眼的假阳，永久留着"),
    "remote-daemon-proto/src/agent_locality_guard.rs:214": ("AGENT_NAMED_WIRE_FIELDS", "skip 表", "冻结的线上字段名豁免，带解锁条件与天花板"),
    "remote-daemon-proto/src/agent_locality_guard.rs:259": ("ADAPTER_CALL_SITES", "登记表", "写死某个 agent 的真实调用点 `(文件, 处数, 说明)`"),
    "remote-daemon-proto/src/agent_locality_guard.rs:290": ("AGENT_REGISTRY_SITES", "登记表", "注册表类的真实处所，配「恰好 `REGISTRY.len()`」那条对价"),
    "remote-daemon-proto/src/agent_locality_guard.rs:322": ("NEW_AGENT_BLOCKERS", "登记表", "接第三个 agent 的阻塞点逐条盘点 `(能力, 文件, 处数, 今天的失败形态)`"),
    # 〔remote-daemon-proto/src/agents/fake/mod.rs〕—— 夹具 agent 的**生产形**代码
    "remote-daemon-proto/src/agents/fake/mod.rs:219": ("CAPABILITIES", "形态表", "12 种能力的**名字闭集**，顺序即 `FakeCaps` 字段序；与 `NEW_AGENT_BLOCKERS` 逐条对账的是它的**内容**，它本身不指向盘上处所"),
    "remote-daemon-proto/src/agents/fake/mod.rs:325": ("STAGES", "形态表", "全流程 7 段的名字闭集（名字进错误信息，所以是常量）"),
    # 〔remote-daemon-proto/src/agents/mod.rs〕
    "remote-daemon-proto/src/agents/mod.rs:112": ("REGISTRY", "非判据表", "**生产段**的 agent 适配器注册表，被应用消费；判据拿它当对拍的另一半，但表本身不是判据自带的"),
    "remote-daemon-proto/src/agents/mod.rs:220": ("SYNTH_REGISTRY", "形态表", "合成注册表夹具：四种形态（存在 / 缺席 / 说不出路径 / 是文件）的闭集，不指向盘上任何处所"),
    # 〔remote-daemon-proto/src/build_id_guard.rs〕
    "remote-daemon-proto/src/build_id_guard.rs:49": ("SUBCOMMAND_HISTORY", "登记表", "`(BUILD_ID, 子命令集指纹)` 只追加的历史，每行对应一个真实发布过的构建"),
    # 〔remote-daemon-proto/src/common/tmux_utf8.rs〕
    "remote-daemon-proto/src/common/tmux_utf8.rs:173": ("CONSUMERS", "登记表", "本 crate 里真实引用这个家的两个消费者（相对 `src/` 的文件路径）"),
    # 〔remote-daemon-proto/src/control/cli_control.rs〕
    "remote-daemon-proto/src/control/cli_control.rs:241": ("NOT_ON_CLI", "skip 表", "帧面有、CLI 面没有的命令逐条登记理由 —— 形状自陈抄 `readonly_guard::ALLOWED`（那也是一张 skip 表）"),
    "remote-daemon-proto/src/control/cli_control.rs:372": ("NO_INPUT_TODAY", "登记表", "不收入方向载荷的命令，与 `REGISTRY.takes_input` **相等断言**对拍 ⇒ 多一条少一条都红"),
    # 〔remote-daemon-proto/src/inbound.rs〕
    "remote-daemon-proto/src/inbound.rs:78": ("COMMANDS", "非判据表", "**生产段**：随 hello 上线的命令集，被应用消费（判据拿它与 `REGISTRY` 对拍）"),
    "remote-daemon-proto/src/inbound.rs:386": ("REGISTRY", "非判据表", "**生产段**：命令单一事实源，`dispatch` 从它查"),
    "remote-daemon-proto/src/inbound.rs:1450": ("ZERO_FIELD_REASONS", "skip 表", "`(命令名, 为什么它可以声明零载荷字段)` —— 豁免清单"),
    # 〔remote-daemon-proto/src/layering_guard.rs〕
    "remote-daemon-proto/src/layering_guard.rs:43": ("ALLOWED_OBSERVE_TO_CONTROL", "skip 表", "允许的跨层边逐条登记"),
    "remote-daemon-proto/src/layering_guard.rs:67": ("ALLOWED_INTO_PLUGIN", "skip 表", "允许进 plugin 的接口面逐条登记"),
    "remote-daemon-proto/src/layering_guard.rs:549": ("RELAY_MUST_NOT_KNOW", "形态表", "判据①的**人群**：`relay/` 不许认识的层名闭集"),
    "remote-daemon-proto/src/layering_guard.rs:572": ("WHO_MAY_NOT_REACH_INTO_RELAY", "形态表", "方向②的人群闭集（层名）"),
    "remote-daemon-proto/src/layering_guard.rs:580": ("D2_REACHING_INTO_RELAY", "形态表", "方向②的目标层名闭集"),
    "remote-daemon-proto/src/layering_guard.rs:583": ("D3_RELAY_TO_PLUGIN", "形态表", "方向③正向的层名闭集"),
    "remote-daemon-proto/src/layering_guard.rs:586": ("D3_PLUGIN_TO_RELAY", "形态表", "方向③反向的层名闭集"),
    # 〔remote-daemon-proto/src/main.rs〕
    # ⚠ 下面两条的「落在哪儿」那一列会印成〔模块级测试前言〕，而它们**真的住在生产段** ——
    #    这不是判词写错，是**文件级那一层的 `test_regions` 在这份文件上过界**：
    #    `main.rs:25` 有一条**没有花括号体**的 `#[cfg(test)] mod alloc_probe;`，
    #    而 `test_regions` 从那条属性一路取到下一个 `\n}\n` ⇒ 把中间的生产代码一起算进「测试段」。
    #    （`guard_core::test_module_ranges` 的头注逐字记着这个坑，它自己防住了；
    #      而本元判据的文件级那一层用的是模块内那份**同名但更粗**的 `test_regions`。）
    #    🔴 今天不出红（闭集在这棵树上零命中），但它是扩射程之后新暴露出来的一条诚实边界。
    "remote-daemon-proto/src/main.rs:190": ("CAPABILITIES", "非判据表", "**生产段**：daemon 自陈的能力 token，随 hello 发出去"),
    "remote-daemon-proto/src/main.rs:196": ("EMITS", "非判据表", "**生产段**：本 daemon 会发射的帧 kind 集，随 hello 发出去"),
    "remote-daemon-proto/src/main.rs:825": ("WINDOW_RAISE", "needle 表", "拉窗那一族 Win32 构件的名字，拿去在生产段文本里搜"),
    "remote-daemon-proto/src/main.rs:845": ("WINDOWS_CFGS", "needle 表", "「带 `#[cfg(windows)]`」只认这两种写法 —— 拿去搜的属性串"),
    "remote-daemon-proto/src/main.rs:999": ("STREAM_FLAGS", "非判据表", "**生产段**：argv 解析的流模式 flag 闭集，被 `split_stream_flags` 消费"),
    "remote-daemon-proto/src/main.rs:1002": ("SUBCOMMANDS", "非判据表", "**生产段**：一次性查询子命令闭集，被 `is_query_mode` 消费"),
    "remote-daemon-proto/src/main.rs:1045": ("SUBCOMMAND_OPTIONS", "非判据表", "**生产段**：子命令自己的选项闭集，被 argv 解析消费"),
    # 〔remote-daemon-proto/src/no_timer_guard.rs〕
    "remote-daemon-proto/src/no_timer_guard.rs:242": ("REGISTERED_DURATION_USES", "登记表", "真实 `Duration` 用法逐条登记，且是**相等断言**（收窄人群，不是豁免）"),
    "remote-daemon-proto/src/no_timer_guard.rs:348": ("SKIPPED_BY_NAME", "skip 表", "采集时按文件名主动跳过的文件"),
    "remote-daemon-proto/src/no_timer_guard.rs:640": ("CALL_FORMS", "needle 表", "按调用形态扫的那八个名字"),
    "remote-daemon-proto/src/no_timer_guard.rs:739": ("KNOWN_PASSING_COUNTEREXAMPLES", "登记表", "盘上真实存在、而它放过了的那一处（修掉就该摘行）"),
    # 〔remote-daemon-proto/src/panorama_locus_guard.rs〕
    "remote-daemon-proto/src/panorama_locus_guard.rs:95": ("LOCKS", "登记表", "头注逐字「登记表①」：今天真实存在的那几份 `Cargo.lock`"),
    "remote-daemon-proto/src/panorama_locus_guard.rs:105": ("ENGINE_LINKED_BY", "登记表", "头注逐字「登记表②」，且是一道**相等断言**（多一份少一份都红）"),
    # 〔remote-daemon-proto/src/platform/fallback_guard.rs〕
    "remote-daemon-proto/src/platform/fallback_guard.rs:101": ("FALLBACK_CFGS", "needle 表", "会引出平台分支的 cfg 属性串，拿去搜"),
    "remote-daemon-proto/src/platform/fallback_guard.rs:111": ("PRIMARY_CFGS", "needle 表", "主分支 cfg 属性串，拿去搜"),
    "remote-daemon-proto/src/platform/fallback_guard.rs:481": ("CATCHABLE", "形态表", "全表①：今天拦得住的形状 + 样本块（自检语料）"),
    "remote-daemon-proto/src/platform/fallback_guard.rs:491": ("BLIND", "形态表", "全表②：今天拦不住的形状（诚实表达，含有意放行的两条）"),
    # 〔remote-daemon-proto/src/plugin/invoke.rs〕
    "remote-daemon-proto/src/plugin/invoke.rs:93": ("INHERITED_ENV_KEYS", "非判据表", "**生产段**：子进程环境白名单，被 `run()` 消费（判据在别处拿它对拍）"),
    # 〔remote-daemon-proto/src/plugin/mod.rs〕
    "remote-daemon-proto/src/plugin/mod.rs:151": ("FILES_IN_THIS_LAYER", "登记表", "本层今天有哪几个 `.rs` —— 逐个列名，多一个少一个都红"),
    "remote-daemon-proto/src/plugin/mod.rs:307": ("ENV_CALL_SHAPES", "needle 表", "本层动子进程环境的三种调用形，拿去搜"),
    "remote-daemon-proto/src/plugin/mod.rs:318": ("ENV_CALL_SITES", "登记表", "今天的登记面 `(文件, 调用形, 处数)`，与扫描结果**相等断言**"),
    "remote-daemon-proto/src/plugin/mod.rs:450": ("EXIT_CODE_CONSTS_IN_THIS_LAYER", "登记表", "本层允许存在的退出码常量，今天恰好一条；是登记面不是过滤器"),
    # 〔remote-daemon-proto/src/plugin_walk_fixture.rs〕
    "remote-daemon-proto/src/plugin_walk_fixture.rs:147": ("REQUIRED_CAPS", "形态表", "宿主侧的必需能力清单（子集检查用的合成语料）"),
    "remote-daemon-proto/src/plugin_walk_fixture.rs:296": ("HOPS", "形态表", "本夹具真走得到的那几跳的名字闭集（顺序承重）"),
    "remote-daemon-proto/src/plugin_walk_fixture.rs:306": ("KNOWLEDGE", "形态表", "每一跳缺了就走不动的那一样知识，与 `HOPS` 一一对应"),
    "remote-daemon-proto/src/plugin_walk_fixture.rs:1426": ("SYNTHETIC_ONLY", "skip 表", "已知的**非真起进程**命中逐条登记（天花板 + 只许变短）"),
    # 〔remote-daemon-proto/src/protocol_doc_guard.rs〕
    "remote-daemon-proto/src/protocol_doc_guard.rs:77": ("DISPATCH_FILES", "登记表", "抽取面文件清单，且下面有 `dispatch_registry_is_complete` 反向核对"),
    "remote-daemon-proto/src/protocol_doc_guard.rs:674": ("ALLOWED", "skip 表", "不在 `wire.rs` 里也放行的类型，三元组带理由"),
    "remote-daemon-proto/src/protocol_doc_guard.rs:815": ("PROTOCOL_CODES", "形态表", "协议级错误码闭集"),
    "remote-daemon-proto/src/protocol_doc_guard.rs:1049": ("EXEMPT", "skip 表", "不需要门控的帧，逐条给理由"),
    # 〔remote-daemon-proto/src/ratchet_guard.rs〕
    "remote-daemon-proto/src/ratchet_guard.rs:65": ("PINS", "登记表", "`(文件, 那一行, 恰好几处, 守什么)` —— 处数不对就红"),
    "remote-daemon-proto/src/ratchet_guard.rs:296": ("STALE_FALLBACK_BACKLOG", "登记表", "今天仍在说谎的那几句，只许降；每条对应盘上真实的处数"),
    # 〔remote-daemon-proto/src/readonly_guard.rs〕
    "remote-daemon-proto/src/readonly_guard.rs:136": ("FS_MUTATION_PATTERNS", "needle 表", "文件系统变更调用的串，拿去搜"),
    "remote-daemon-proto/src/readonly_guard.rs:172": ("WHITELIST_STILL_FORBIDDEN", "needle 表", "白名单模块仍禁的串，拿去搜"),
    "remote-daemon-proto/src/readonly_guard.rs:457": ("READ_ONLY", "形态表", "只读动词白名单 —— 判「这个调用是不是只读」的类，不是登记处所"),
    "remote-daemon-proto/src/readonly_guard.rs:649": ("ALLOWED", "skip 表", "生产段允许起的进程，五元组带理由与解锁条件"),
    "remote-daemon-proto/src/readonly_guard.rs:998": ("CELLS", "形态表", "四情形闭集（`§0a` 那张表）"),
    "remote-daemon-proto/src/readonly_guard.rs:1187": ("KNOWN_PASSING_COUNTEREXAMPLES", "登记表", "盘上真实的一处漏网，带住址（修掉就该摘行）"),
    "remote-daemon-proto/src/readonly_guard.rs:1312": ("STAGED_ZERO", "登记表", "被钉住的那些「零」逐条登记，每行一个真实住址"),
    "remote-daemon-proto/src/readonly_guard.rs:1611": ("DEP_SECTIONS", "形态表", "两个段名常量组成的闭集"),
    "remote-daemon-proto/src/readonly_guard.rs:1621": ("VERDICTS", "形态表", "判档闭集 + 每档的解锁条件"),
    "remote-daemon-proto/src/readonly_guard.rs:1666": ("SIGNED", "登记表", "每个 crate 一行的签字表，与真实依赖图对拍"),
    # 〔remote-daemon-proto/src/relay/creds_guard.rs〕
    "remote-daemon-proto/src/relay/creds_guard.rs:37": ("ALLOWED_LOG_ARGS", "skip 表", "允许进日志的名字白名单（默认拒绝）"),
    "remote-daemon-proto/src/relay/creds_guard.rs:97": ("LOG_SITES", "登记表", "每一处日志的格式串起首 —— **相等断言的另一半**"),
    # 〔remote-daemon-proto/src/relay/http1.rs〕
    "remote-daemon-proto/src/relay/http1.rs:119": ("HOP_BY_HOP", "非判据表", "**生产段**：逐跳头闭集，被转发逻辑消费"),
    "remote-daemon-proto/src/relay/http1.rs:563": ("SRC", "形态表", "一条判据体内的**合成请求字节**夹具（`&[u8]`），不是表"),
    # 〔remote-daemon-proto/src/relay/server.rs〕
    "remote-daemon-proto/src/relay/server.rs:788": ("AUTH_HEADER_NAMES", "非判据表", "**生产段**：中转认得的鉴权头名闭集，被换头逻辑消费"),
    # 〔remote-daemon-proto/src/single_stream_guard.rs〕
    "remote-daemon-proto/src/single_stream_guard.rs:166": ("PINS", "登记表", "`(文件, 那一行, 文件内处数, 全 crate 处数, 说法)`"),
    # 〔remote-daemon-proto/src/wire.rs〕
    "remote-daemon-proto/src/wire.rs:635": ("STREAMING", "登记表", "🔴 **本树上最像本元判据该管的那一条**：登记表 ＋ `scan_tree!` 扫描面，声明在判据体内"),
}

#: grep 口径命中、但**不是**一条声明的那几条（注释 / 字符串里提到一个声明）。
NOT_A_DECL = {
    "remote-daemon-proto/src/protocol_doc_guard.rs:1015": (
        "EMITS", "非判据（不是声明）", "一行 `//` 注释里举的例子：「从 main.rs 生产段抠 `const EMITS: &[&str] = …`」"
    ),
}


def scan(tree: Tree, subs):
    """按 `scan_tree!` 的口径列一棵（或几棵）子树的 `.rs`，并摘掉元判据自己那一份。"""
    return [r for r in tree.list_rs(subs) if r != GUARD_SELF]


def simulate(tree: Tree, subs):
    """模拟 `every_registry_guard_keeps_its_reverse_half` 在给定实参下采到什么。"""
    population, missing, per_guard, missing_guards = [], [], [], []
    for rel in scan(tree, subs):
        src = tree.read(rel)
        for name, item in guards_declaring_a_table(test_source(src)):
            per_guard.append(f"{rel}::{name}")
            if not any(m in item for m in REVERSE):
                missing_guards.append(f"{rel}::{name}")
        regs = test_regions(src)
        if not declares_a_guard_table(regs):
            continue
        population.append(rel)
        if not any(m in regs for m in REVERSE):
            missing.append(rel)
    return population, missing, per_guard, missing_guards


def decl_rows(tree: Tree, rels):
    """逐条声明 + 它落在哪儿（生产段 / 模块级测试前言 / 某条 `#[test]` 体内）。"""
    rows = []
    for rel in rels:
        src = tree.read(rel)
        lines = src.split("\n")
        regs = test_regions(src)
        tsrc = test_source(src)
        # 判据级：把「这条声明的文本行」落在哪条判据体内算出来。
        owner = {}
        for c in test_attr_chunks(tsrc):
            it = guard_fn_item(c)
            if not it:
                continue
            for l in it[1].split("\n"):
                if DECL.match(l):
                    owner[l.strip()] = it[0]
        for i, line in enumerate(lines, 1):
            m = DECL.match(line)
            if not m:
                continue
            in_test = line in regs or line.strip() in {x.strip() for x in regs.split("\n")}
            rows.append((rel, i, m.group(2), in_test, owner.get(line.strip())))
    return rows


def main() -> int:
    here = pathlib.Path(__file__).resolve()
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(here.parent.parent), help="仓根（默认 = 本文件所在的那棵工作树）")
    ap.add_argument("--at", default=None, help="量哪个提交（不给 = 量工作树此刻的盘面）")
    ap.add_argument("--list", action="store_true", help="逐条判词（D1② 的产物）")
    a = ap.parse_args()
    tree = Tree(pathlib.Path(a.root).resolve(), a.at)
    print(f"# 被测对象：{tree.label()}")
    print(f"# 本趟用的闭集（{len(CLOSED_SET)} 个名字）：{' · '.join(CLOSED_SET)}")
    print(f"# 本趟用的 REVERSE（{len(REVERSE)} 形）：{' · '.join(REVERSE)}")

    daemon_files = tree.list_rs((DAEMON,))
    print(f"\n## 一 · `D1①` 分母的几种画法（都在 `{DAEMON}` 上）")
    guardish = [r for r in daemon_files if r.endswith("_registry.rs") or r.endswith("_guard.rs")]
    n_narrow = n_grep = n_decl = 0
    files_with_decl = set()
    for rel in daemon_files:
        for line in tree.read(rel).split("\n"):
            if NARROW.search(line):
                n_narrow += 1
            if GREP.search(line):
                n_grep += 1
            if DECL.match(line):
                n_decl += 1
                files_with_decl.add(rel)
    # 元判据真正会怎么圈人（文件级 / 判据级）。
    pop_t, miss_t, pg_t, mg_t = simulate(tree, (DAEMON,))
    shape_pop = [r for r in daemon_files if GREP.search(test_regions(tree.read(r)))]
    print(f"  ① 全部 `.rs`                                      {len(daemon_files):4d} 份")
    print(f"  ② 名字像登记表的（`*_registry.rs`/`*_guard.rs`）  {len(guardish):4d} 份")
    print(f"  ③ `const X: &[` —— 件文件 `§0` 那把窄尺           {n_narrow:4d} 条")
    print(f"     同一批，`K-R33` 那把宽尺                        {n_grep:4d} 条")
    print(f"     其中**真的是一条声明**（行首锚定）             {n_decl:4d} 条  ← `D1②` 的分母")
    print(f"     带声明的文件                                    {len(files_with_decl):4d} 份")
    print(f"  ④ **这条元判据真正会圈的人**（闭集按名字认）：")
    print(f"     文件级人群 {len(pop_t)} 份 · 判据级 {len(pg_t)} 条 ← 🔴 **本件的核心读数**")
    print(f"  ⑤ 若闭集换成「按形状认」（测试段里有 `const X: &[` 就算）：{len(shape_pop)} 份")

    print("\n## 二 · `D1③` 「是登记表、而且没有反向那半」")
    reg_rows, reg_missing_concept, reg_missing_machine = [], [], []
    unknown = []
    for rel in daemon_files:
        src = tree.read(rel)
        lines = src.split("\n")
        regs = test_regions(src)
        tsrc = test_source(src)
        owner = {}
        for c in test_attr_chunks(tsrc):
            it = guard_fn_item(c)
            if not it:
                continue
            for l in it[1].split("\n"):
                if DECL.match(l):
                    owner[l.strip()] = it
        for i, line in enumerate(lines, 1):
            m = DECL.match(line)
            if not m:
                continue
            key = f"{rel}:{i}"
            v = VERDICTS.get(key)
            if v is None:
                unknown.append(f"{key} {m.group(2)}")
                continue
            if v[0] != m.group(2):
                unknown.append(f"{key} 名字对不上：判词表写 {v[0]}，盘上是 {m.group(2)}")
                continue
            if v[1] != "登记表":
                continue
            own = owner.get(line.strip())
            scope = own[0] if own else "（模块级前言 / 生产段）"
            in_test = own is not None or any(line == x for x in regs.split("\n"))
            body = own[1] if own else regs
            has_rev = [r for r in REVERSE if r in body]
            reg_rows.append((key, m.group(2), scope, in_test, has_rev))
            if not has_rev:
                reg_missing_machine.append(f"{key} {m.group(2)} @ {scope}")
    if unknown:
        print("  🔴 判词表与盘面对不上（行号漂了 / 新增了声明）—— **本趟读数不许直接用**：")
        for u in unknown:
            print(f"     {u}")
    n_reg = sum(1 for v in VERDICTS.values() if v[1] == "登记表")
    print(f"  · 判成**登记表**的：{n_reg} 条（分母 = {n_decl} 条真声明）")
    print(f"  · 其中它所在的那一段（判据体 / 测试段）里**没有任何 `REVERSE` 形态**的：{len(reg_missing_machine)} 条")
    for x in reg_missing_machine:
        print(f"      {x}")
    print(f"  🔴 而这条元判据**今天真会点名**的：{len(mg_t)} 条（判据级） · {len(miss_t)} 份（文件级）")
    print("     —— 差额的全部理由是**闭集按名字认**，见下一节。")

    print("\n## 三 · 闭集在这棵树上命中几次（`D1③` 那个差额的根因）")
    tot = 0
    for rel in daemon_files:
        src = tree.read(rel)
        hits = [c for c in CLOSED_SET if c in src]
        if hits:
            tot += 1
            print(f"    {rel}: {hits}")
    print(f"  合计 {tot} 份文件里出现过闭集里的名字（整份文件文本，比测试段还宽）")

    print("\n## 四 · 扩射程前后的采集量（`D2` 的 acceptor 逐字要这一格）")
    for label, subs in (("扩之前（只有 src-tauri/src）", ("src-tauri/src",)),
                        ("扩之后（两棵树）", TREES),
                        ("单看 daemon 那棵", (DAEMON,))):
        pop, miss, pg, mg = simulate(tree, subs)
        print(f"  {label:34s} 扫描面 {len(scan(tree, subs)):4d} 份 · "
              f"文件级人群 {len(pop):2d} · 实缺 {len(miss)} · 判据级 {len(pg):2d} · 实缺 {len(mg)}")

    print("\n## 五 · `D1④` 判据级那一层会不会在这棵树上**误采**")
    print(f"  · 今天（闭集按名字）：判据级在 daemon 树上采到 {len(pg_t)} 条 ⇒ 误采**不可能发生**，")
    print("    因为它一条都采不到。这个「不误采」买的是零，不是准。")
    print("  · 若有人把闭集换成「按形状认」，那时的人群与其中的切法风险：")
    for rel in shape_pop:
        src = tree.read(rel)
        tsrc = test_source(src)
        chunks = test_attr_chunks(tsrc)
        recognised = sum(1 for c in chunks if guard_fn_item(c))
        print(f"    {rel:56s} `#[test]` 块 {len(chunks):3d} · `guard_fn_item` 认得出 {recognised:3d}"
              f" · 认不出 {len(chunks) - recognised}")

    if a.list:
        print("\n## 六 · `D1②` 逐条判词")
        print(VERDICT_RUBRIC)
        cnt: dict[str, int] = {}
        for rel in daemon_files:
            src = tree.read(rel)
            lines = src.split("\n")
            tsrc = test_source(src)
            regs = test_regions(src)
            owner = {}
            for c in test_attr_chunks(tsrc):
                it = guard_fn_item(c)
                if not it:
                    continue
                for l in it[1].split("\n"):
                    if DECL.match(l):
                        owner[l.strip()] = it[0]
            printed_header = False
            k = 0
            for i, line in enumerate(lines, 1):
                if not DECL.match(line):
                    continue
                k += 1
                if not printed_header:
                    print(f"\n  〔{rel}〕")
                    printed_header = True
                key = f"{rel}:{i}"
                name, verdict, why = VERDICTS.get(key, ("?", "判不了", "判词表里没有这一条"))
                cnt[verdict] = cnt.get(verdict, 0) + 1
                own = owner.get(line.strip())
                where = f"判据 `{own}` 体内" if own else (
                    "模块级测试前言" if line in regs.split("\n") else "生产段")
                print(f"  · [{key}] `{name}` — **{verdict}**：{why}  〔{where}〕")
        for key, (name, verdict, why) in NOT_A_DECL.items():
            cnt[verdict] = cnt.get(verdict, 0) + 1
            print(f"\n  〔grep 口径多出来的那一条〕")
            print(f"  · [{key}] `{name}` — **{verdict}**：{why}")
        print("\n  总账：" + " · ".join(f"{k} {v}" for k, v in sorted(cnt.items())))
    return 0


if __name__ == "__main__":
    sys.exit(main())
