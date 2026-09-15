#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R117 第一拍（只读摸底）的尺子 —— 安装面人群 · 归属 · `Side` 栏复量。

╔══════════════════════════════════════════════════════════════════════════════╗
║ 量具住址 · 被测对象（纪律 12：量具住址要唯一定位到那一份，且要写明被测对象指向哪棵树）║
╚══════════════════════════════════════════════════════════════════════════════╝

* 本文件的住址：`<被测树>/evidence/K-R117-ruler.py`（**跟着被测树走**，不住临时目录）。
* 被测对象：默认 = **本文件所在目录的父目录**（即那棵树的根）。`--root` 可以指到别处
  （死值验就是这么做的：把树拷一份、在副本上变异、让同一份尺子去量副本）。
* 每趟开头都会把「我在量哪棵树 · 那棵树的 HEAD 是什么」印出来 —— 那两行就是这份读数的分母。

╔══════════════════════════════════════════════════════════════════════════════╗
║ 它判什么 · 不判什么                                                            ║
╚══════════════════════════════════════════════════════════════════════════════╝

**判**（判红，退出码非 0）：
  R1 人群没塌（每个解析器都有地板；扫不到东西一律 CRASH，不许当「零违例」）。
  R2 `installable: true` 那个数：**19 与 7 的差要逐行对得上**（Σ 分档 == 朴素 grep 数），
     且「真·字段声明」那一档必须等于 `TOOLS` 里 `installable == true` 的条数
     （两把互相独立的尺子对拍：一把数文本行、一把解声明表）。
  R3 归档覆盖：`LEDGER` 现打的每一个能力 id 都要在 `CAP_ARCHIVE` 里恰好一条。
     **少一条红** = 新长出一条命令（带新能力 id）而没人归档；**多一条红** = 表在腐烂。
  R4 种子不许被划出安装面：从 `tool_registry::claims()` 的装 / 卸实现符号派生出来的那几条
     命令，它们的能力**必须**归 ①/②/③。有人把一条真装口归成「非装面」⇒ 当场红。
  R5 写盘落点（`WRITE_SITES`）同样两向对拍：带 `Some(tool)` 的行逐行归档，
     多一行 / 少一行都红。
  R6 命令住址从源码派生：`LEDGER` 里的每条命令都要在 `#[tauri::command]` 面上找得到，
     找不到红（住址这一列不许手写）。
  R7 `Side` 栏那四处连锁**现打还在不在**（`EXPECTED_LOCAL_OR_BOTH` · `ORIGIN_TAKING_BOTH` ·
     `tmux.manage` 那条散文 · 钉那句散文的判据名）。少一处红 —— 那意味着 `K-R115` 交回的
     代价读数已经不是今天的形状。

**不判**（诚实边界，逐条写死）：
  B1 **不判归属对不对**。`CAP_ARCHIVE` 的每一格是**人的判断**（依据写在那一格的 `why` 里），
     本尺子只保证「没有一条命令没人回答过」，不保证「回答得对」。
     ——— 同 `tool_registry::UnmanagedEnv.who` 那条头注的口径：设计判断不是读数。
  B2 **不判「翻了 `Side` 会不会真的对」**。它只数「今天有几行、签字表说几行、连锁在不在」。
  B3 **`Side` 的派生类（`RemoteOnly` / `FramePlane` / …）本尺子不重算** ——
     那是 `parity_ledger::command_dispatch_class()` 的活，它要用 `guard_core` 的整套剥法
     （块注释 / 行尾注释 / `#[cfg(test)] mod` 的词法掩码收尾）。在这里抄一份近似的剥法，
     就是本仓头注反复点名的「同一件事的第二份表示，而它们会各自漂」。
     ⇒ 本尺子只复量**纯数据**那几个（55 行 · 签字表四档 · 人裁 6 条里几条是「说假话」），
     派生那一半由 `cargo test -p monitor` 跑 `the_remote_side_column_is_signed_off` 给。
  B4 只看 `src-tauri/src/**.rs`（外加 `build.rs` 这个名字出现在 `WRITE_SITES` 里那一行）。
     `remote-daemon-proto/` 那棵树、`src/**.ts` 前端面、shell 脚本，本尺子一概盖不到。
  B5 解析是**文本解析**，不是 `syn`。它认的是本仓 rustfmt 之后的那几种固定形状
     （见每个 parser 的 docstring）；形状一变就会掉到地板断言上（R1）——
     **宁可 CRASH，不许静默少数几条**。
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections import Counter, OrderedDict

# ══════════════════════════════════════════════════════════════════════════════
# 归档表 —— **人的判断住这里**，每一格都要给依据（定框 id）。
#
# 🔴 这张表不是「人群」。人群从源码派生（`LEDGER` 现打有几个能力 id 就是几个），
#    这张表只回答「那一条归三处里的哪一处」。覆盖不全 ⇒ R3 当场红。
#
# 三处的名字取自 `R63` 裁定三（件文件 `§0b`）：
#   ① 装后端  ② 生成 rc 片段让用户自己填  ③ 装 MCP / skill 等
#   非装面   —— 它根本不是安装 / 卸载 / 查装态的动作
# ══════════════════════════════════════════════════════════════════════════════

B1 = "①装后端"
B2 = "②生成rc片段"
B3 = "③装MCP/skill"
NA = "非装面"

BUCKETS = (B1, B2, B3, NA)

# cap -> (bucket, 依据, 一句话)
CAP_ARCHIVE: "OrderedDict[str, tuple]" = OrderedDict([
    # ─────────────────────────── ① 装后端 ───────────────────────────
    ("ccm.install", (B1, "R64+K26",
                     "`ccm` 就是后端的 CLI 入口；`install_local_ccm_entry` 放下去的逐字是"
                     "「后端二进制自己的改名副本」⇒ 装 ccm 与装后端是同一个动作")),
    ("ccm.uninstall", (B1, "R64+K26", "同上，卸那一侧")),
    ("ccm.install-ui", (B1, "R64+K26", "装 ccm 之前的预览 / 扫 PATH，是同一颗按钮的前半")),
    ("ccm.status", (B1, "R64+K26+K-R111",
                    "查装态。`K-R111` 已判 `cc_integration_status` 归属重判、该挪进安装面")),
    ("daemon.deploy", (B1, "K33+K27", "推 / 撤远端那一份后端，本来就是「装后端」")),
    ("acct-iso.deploy", (B1, "R75〔用@09-14「account进后端」〕", "已裁：account 进后端 ⇒ 落 ①")),
    ("acct-iso.check", (B1, "R75", "同上，查装态那一半")),
    # ──────────────────── ② 生成 rc 片段让用户自己填 ────────────────────
    ("alias.account-commands", (B2, "K33+K34",
                                "「有需要动用户 alias 的就生成命令让用户自己填」；"
                                "先例 `accounts-section.ts::renderRcSnippet`")),
    ("acct-iso.shellinit", (B2, "K33",
                            "它产出的就是一段 rc 片段（`cc-acct-iso` 的 `cmd_shellinit` "
                            "只 `printf`、一个字节都不写盘）")),
    # ─────────────────────── ③ 装 MCP / skill 等 ───────────────────────
    ("mcp.write", (B3, "K34", "「包括安装 skill / MCP 等等」")),
    ("mcp.remove", (B3, "K34", "同上，撤那一侧")),
    ("cc-bus.deploy", (B3, "K34",
                       "落点是 `<claude_dir>/skills/cc-bus/` ⇒ 属「装 skill」，不属「装后端」")),
    ("cc-bus.install-state", (B3, "K34", "同上，查装态那一半")),
    ("skill.inbox", (B3, "K34",
                     "写侧（`write_skill_file`）是往用户项目里装东西；读侧两条由 "
                     "`CMD_OVERRIDE` 划出去 —— 同一个能力 id 里读写两性质")),
    # ─────────────────────────── 非装面 ───────────────────────────
    # 下面每一条都是「它不是装 / 卸 / 查装态的动作」。理由一律给**它到底在干什么**，
    # 不写「与安装无关」这种同义反复。
    ("accounts.last-used", (NA, "—", "读账号最近用过谁")),
    ("accounts.list", (NA, "—", "列账号")),
    ("accounts.session-accounts", (NA, "—", "列会话用的账号")),
    ("accounts.trust", (NA, "—", "查远端信任态（ssh 那一侧，不是装东西）")),
    ("app.auto-launch", (NA, "—", "app 自己的开机自启开关")),
    ("app.config", (NA, "—", "app 自己的配置读写")),
    ("app.daemon-policy", (NA, "—", "app 自己的退出策略开关")),
    ("app.data-paths", (NA, "—", "报 app 自己的数据目录")),
    ("app.diagnostics", (NA, "—", "app 自己的诊断开关 / 埋点")),
    ("app.logs", (NA, "—", "app 自己的日志")),
    ("app.window.self", (NA, "—", "窗口动作")),
    ("app.window.session", (NA, "—", "窗口动作")),
    ("app.window.settings", (NA, "—", "窗口动作")),
    ("audit.config-surface", (NA, "—", "配置面审计页的**读**侧（它报告安装面，不改它）")),
    ("audit.drift-ledger", (NA, "—", "漂移账本的读侧")),
    ("cc-bus.cockpit", (NA, "—", "cc-bus 驾驶舱的读 / 发消息，不是装 cc-bus")),
    ("creds.relay-key", (NA, "—", "中转那把第三方 API key（monitor 自己的凭据文件）")),
    ("daemon.lifecycle", (NA, "—", "起 / 停 / 列后端进程 —— 是**跑**它，不是**装**它")),
    ("daemon.status", (NA, "—", "问后端活没活 —— 同上，不是查装态")),
    ("history.branch", (NA, "—", "会话历史")),
    ("history.delete", (NA, "—", "会话历史")),
    ("history.list-projects", (NA, "—", "会话历史")),
    ("history.list-sessions", (NA, "—", "会话历史")),
    ("history.metadata", (NA, "—", "会话历史")),
    ("history.read-session", (NA, "—", "会话历史")),
    ("hooks.diagnose", (NA, "—",
                        "**只诊断**钩子，不装它（`TouchEffect::GenerateOnly`：产品自己不动手）")),
    ("launch.render-attach", (NA, "—", "渲染一条命令串")),
    ("launch.render-cli", (NA, "—", "渲染一条命令串")),
    ("launch.render-payload", (NA, "—", "渲染一份载荷")),
    ("launch.send-into", (NA, "—", "往已有会话里送一段")),
    ("mcp.list-origins", (NA, "—", "列 MCP 的 origin —— 读")),
    ("mcp.list-project-dirs", (NA, "—", "列项目目录 —— 读")),
    ("mcp.read", (NA, "—", "读 MCP 配置")),
    ("panorama.code-graph", (NA, "—", "代码全景（sidecar 的**用**，不是装它）")),
    ("plugins.marketplaces", (NA, "—", "列插件市场 —— 只读枚举")),
    ("port-forward", (NA, "—", "端口转发")),
    ("relay.routing", (NA, "—", "问这几个账号走不走中转")),
    ("search.history", (NA, "—", "搜索")),
    ("search.index", (NA, "—", "搜索索引（app 自己的索引，不落用户环境）")),
    ("session.activity", (NA, "—", "会话")),
    ("session.forget", (NA, "—", "会话")),
    ("session.launch", (NA, "—", "起会话")),
    ("session.list-active", (NA, "—", "会话")),
    ("session.tasks", (NA, "—", "会话")),
    ("sftp.file-panel", (NA, "—", "文件面板（用户自己搬文件，不是产品装东西）")),
    ("ssh.host-config", (NA, "—", "ssh 主机配置 / 推公钥 —— 连得上那一层，不是装我们的东西")),
    ("subagent.load", (NA, "—", "读 subagent 定义")),
    ("terminal.focus", (NA, "—", "把终端提到前台")),
    ("tmux.local-census", (NA, "—", "列 tmux 会话")),
    ("tmux.manage", (NA, "—", "tmux 会话管理")),
    ("usage.aggregate", (NA, "—", "用量")),
    ("usage.per-account", (NA, "—", "用量")),
])

# 同一个能力 id 里读写两性质时，按命令名覆盖。**key 必须在 `LEDGER` 里**（R3 的第三向）。
CMD_OVERRIDE = {
    "list_skills": (NA, "K34", "列 skill —— 只读，不是装"),
    "read_skill_file": (NA, "K34", "读那个收件箱文件 —— 只读"),
}

# 写盘落点（`write_site_registry::WRITE_SITES`）里**带 tool id 的那几行** ＋ `§0b` 另外点名的
# `build.rs::embed_daemons`（它的 tool id 是 `None`，见 §S5 那段读数）。
# key = "文件::函数"。两向对拍：人群从 `WRITE_SITES` 现打，这张表少一行 / 多一行都红。
SITE_ARCHIVE = {
    "local_backend.rs::install_local_ccm_entry": (
        B1, "R64+K26", "放的就是「后端二进制自己的改名副本」"),
    "build.rs::embed_daemons": (
        B1, "K33", "把内嵌后端复制进 `OUT_DIR` —— 装后端那条链的构建期一环"),
    "profile_installer.rs::install_to_profile": (
        B2, "K33", "今天真写用户 profile；② 的目标形状是改成只生成待贴片段"),
    "profile_installer.rs::uninstall_from_profile": (
        B2, "K33", "同上，摘那一侧"),
    "profile_installer.rs::atomic_write_string": (
        B2, "K33", "上面两个动作唯一的落盘漏斗"),
    "profile_installer.rs::atomic_replace_path": (
        B2, "K33", "跨设备回退的 rename，同一条落盘链"),
    "cc_bus_deploy.rs::deploy_into": (
        B3, "K34", "写 `<claude_dir>/skills/cc-bus/`"),
    "mcp.rs::write_json_atomic": (
        B3, "K34", "写项目级 MCP 配置，是 `mcp.write` 的真落点"),
}

# `§0b` 里点名、而 `WRITE_SITES` 的 tool id 是 `None` 的那几行 —— 单列，别混进「带 id」那个数。
SITE_TOOLID_NONE_BUT_ARCHIVED = ("build.rs::embed_daemons",)

# ══════════════════════════════════════════════════════════════════════════════
# 解析器（全部只读文本；每个都有地板断言）
# ══════════════════════════════════════════════════════════════════════════════

RED: list = []
NOTE: list = []


def red(tag: str, msg: str) -> None:
    RED.append((tag, msg))


def crash(msg: str) -> None:
    print(f"\nCRASH（地板断言没过，本趟所有读数作废）：{msg}", file=sys.stderr)
    sys.exit(3)


def slurp(path: str) -> str:
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def const_block(src: str, head: str) -> str:
    """取一个 `const X: … = &[` 到**列 4 的 `];`** 之间那一段（本仓 rustfmt 的固定形状）。"""
    i = src.index(head)
    j = src.index("\n    ];", i)
    return src[i:j]


def parse_tools(src: str):
    """`TOOLS` 的 `(id, installable, uninstallable)`。

    形状：`id: "x",` 之后最近的一条 `installable: <bool>,` 与 `uninstallable: <bool>,`。
    只在 `pub const TOOLS` 那一段里找 ⇒ `UNMANAGED_ENV` 的 `id:` 不会混进来。
    """
    seg = const_block(src, "pub const TOOLS: &[ToolSpec] = &[")
    out = []
    for m in re.finditer(r'\n        id: "([^"]+)",', seg):
        tail = seg[m.end():]
        inst = re.search(r"\n        installable: (true|false),", tail)
        unin = re.search(r"\n        uninstallable: (true|false),", tail)
        if not inst or not unin:
            crash(f"TOOLS 里 `{m.group(1)}` 解不出 installable/uninstallable")
        out.append((m.group(1), inst.group(1) == "true", unin.group(1) == "true"))
    return out


def parse_unmanaged(src: str):
    """`UNMANAGED_ENV` 的 `(id, who)`。"""
    seg = const_block(src, "pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[")
    out = []
    for m in re.finditer(r'\n        id: "([^"]+)",', seg):
        tail = seg[m.end():]
        who = re.search(r"\n        who: Provisioning::(\w+),", tail)
        out.append((m.group(1), who.group(1) if who else "?"))
    return out


def parse_ledger(src: str):
    """`LEDGER` 的 `(cmd, cap, side)`。形状：三元组字面量，`Side::X` 收尾。"""
    seg = const_block(src, "const LEDGER: &[(&str, &str, Side)] = &[")
    return re.findall(r'"([A-Za-z0-9_]+)"\s*,\s*"([^"]+)"\s*,\s*Side::(\w+)', seg)


def parse_write_sites(src: str):
    """`WRITE_SITES` 的 `(file, fn, Option<tool>)`。形状：`("f", "n", Some("t")|None,`。"""
    seg = const_block(
        src, "pub(crate) const WRITE_SITES: &[(&str, &str, Option<&str>, &str)] = &[")
    out = []
    for m in re.finditer(r'\(\s*"([^"]+\.rs)"\s*,\s*"([A-Za-z0-9_]+)"\s*,\s*'
                         r'(None|Some\("([^"]+)"\))\s*,', seg):
        out.append((m.group(1), m.group(2), m.group(4)))
    return out


def parse_claims(src: str):
    """`tool_registry::claims()` 的 `(tool, install_addr, uninstall_addr)`。

    形状：`Claim { tool: "x", home: …, install: Some(ImplSite { addr: "f::n", … }) … }`。
    取法：按 `Claim {` 切段，段内找 `install:` / `uninstall:` 之后最近的 `addr: "…"`。
    """
    i = src.index("fn claims() -> Vec<Claim> {")
    seg = src[i:src.index("\n    }\n", i)]
    out = []
    parts = seg.split("Claim {")[1:]
    for p in parts:
        tm = re.search(r'tool: "([^"]+)"', p)
        if not tm:
            continue
        addrs = {}
        for field in ("install", "uninstall"):
            fm = re.search(field + r":\s*Some\(", p)
            if fm:
                am = re.search(r'addr: "([^"]+)"', p[fm.end():])
                if am:
                    addrs[field] = am.group(1)
        out.append((tm.group(1), addrs.get("install"), addrs.get("uninstall")))
    # `profile_install()` / `profile_uninstall()` 这两个闭包形 —— 上面那条 regex 看不见它们，
    # 因为那两行写的是 `install: Some(profile_install()),`。补一次：按闭包体里的 addr 取。
    closure = {}
    for name in ("profile_install", "profile_uninstall"):
        cm = re.search(r"let " + name + r" = \|\| \{?\s*ImplSite \{\s*\n?\s*addr: \"([^\"]+)\"",
                       seg)
        if cm:
            closure[name] = cm.group(1)
    fixed = []
    for tool, ins, unins in out:
        if ins is None and re.search(r'tool: "' + re.escape(tool) + r'"[\s\S]{0,400}?'
                                     r"install: Some\(profile_install\(\)\)", seg):
            ins = closure.get("profile_install")
        if unins is None and re.search(r'tool: "' + re.escape(tool) + r'"[\s\S]{0,400}?'
                                       r"uninstall: Some\(profile_uninstall\(\)\)", seg):
            unins = closure.get("profile_uninstall")
        fixed.append((tool, ins, unins))
    return fixed


def scan_tauri_commands(src_root: str):
    """`#[tauri::command]` → `{命令名: "相对路径:行号"}`。**住址这一列只许从这里来。**"""
    attr = "#[tauri::" + "command]"
    out = {}
    for base, _dirs, files in os.walk(src_root):
        for fn in sorted(files):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(base, fn)
            rel = os.path.relpath(path, src_root).replace("\\", "/")
            lines = slurp(path).split("\n")
            for k, line in enumerate(lines):
                if line.strip() != attr:
                    continue
                for j in range(k + 1, min(k + 12, len(lines))):
                    m = re.search(r"\bfn ([A-Za-z0-9_]+)\s*\(", lines[j])
                    if m:
                        out.setdefault(m.group(1), f"{rel}:{j + 1}")
                        break
    return out


# ══════════════════════════════════════════════════════════════════════════════
# §S1 —— `installable: true`：19 与 7 差在哪
# ══════════════════════════════════════════════════════════════════════════════

def bucket_installable_line(line: str, lineno: int, test_from: int) -> str:
    s = line.strip()
    if "uninstallable: true" in line:
        return "被 `uninstallable: true` 这个**更长的串**顺带命中"
    if s.startswith("//") or s.startswith("///") or s.startswith("//!"):
        return "文档 / 行注释"
    if lineno >= test_from:
        return "测试段（`#[cfg(test)]` 之后）"
    if re.match(r"^installable: true,$", s):
        return "真·字段声明"
    return "生产段里的字符串（报错文案 / 格式串）"


def section_installable(reg_src: str, tools):
    print(head("§S1 `installable: true` —— 19 与 7 差在哪（逐行分档，Σ 必须等于朴素 grep 数）"))
    lines = reg_src.split("\n")
    test_from = next((i + 1 for i, l in enumerate(lines) if l == "#[cfg(test)]"), len(lines) + 1)
    hits = [(i + 1, l) for i, l in enumerate(lines) if "installable: true" in l]
    naive = len(hits)
    occ = reg_src.count("installable: true")
    buckets = Counter()
    detail = []
    for ln, l in hits:
        b = bucket_installable_line(l, ln, test_from)
        buckets[b] += 1
        detail.append((ln, b, l.strip()[:72]))
    print(f"  朴素口径（PM 那条 `grep -c \"installable: true\"`）：**{naive}** 行"
          f"（同串出现 {occ} 次 ⇒ 一行一次，不是某行数了两遍）")
    print(f"  `#[cfg(test)]` 起于第 {test_from} 行")
    print("  分档：")
    for b, n in buckets.most_common():
        print(f"    · {n:2d}  {b}")
    print(f"  Σ = {sum(buckets.values())}")
    real = buckets.get("真·字段声明", 0)
    decl = sum(1 for _, inst, _ in tools if inst)
    print(f"  ⇒ **真·字段声明 {real} 条**；独立那把尺子（解 `TOOLS` 声明表）数出 "
          f"`installable == true` **{decl} 条**")
    print("  逐行（前 6 列宽截断）：")
    for ln, b, txt in detail:
        print(f"    {ln:5d}  {b:<34s} {txt}")
    if sum(buckets.values()) != naive:
        red("R2a", "分档没把朴素口径分完")
    if real != decl:
        red("R2b", f"两把尺子对不上：文本行数 {real} vs 声明表 {decl}")
    return naive, real, decl


# ══════════════════════════════════════════════════════════════════════════════
# 输出小工具
# ══════════════════════════════════════════════════════════════════════════════

def head(t: str) -> str:
    return "\n" + "─" * 78 + f"\n{t}\n" + "─" * 78


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(here),
                    help="被测树的根（默认 = 本文件所在目录的父目录）")
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    src_dir = os.path.join(root, "src-tauri", "src")
    if not os.path.isdir(src_dir):
        crash(f"`{src_dir}` 不存在 —— `--root` 指错树了")
    try:
        sha = subprocess.run(["git", "-C", root, "rev-parse", "--short", "HEAD"],
                             capture_output=True, text=True, timeout=20).stdout.strip()
    except Exception:
        sha = ""
    print(f"量具：{os.path.abspath(__file__)}")
    print(f"被测树：{root}" + (f"（HEAD {sha}）" if sha else "（不是 git 树 / 取不到 HEAD）"))

    reg_src = slurp(os.path.join(src_dir, "tool_registry.rs"))
    led_src = slurp(os.path.join(src_dir, "parity_ledger.rs"))
    wsr_src = slurp(os.path.join(src_dir, "write_site_registry.rs"))

    tools = parse_tools(reg_src)
    unmanaged = parse_unmanaged(reg_src)
    ledger = parse_ledger(led_src)
    sites = parse_write_sites(wsr_src)
    claims = parse_claims(reg_src)
    cmd_addr = scan_tauri_commands(src_dir)

    # ── R1 地板（反向自检）：扫不到东西一律 CRASH，不许当「零违例」 ──────────────
    floors = [("TOOLS", len(tools), 5), ("UNMANAGED_ENV", len(unmanaged), 5),
              ("LEDGER", len(ledger), 100), ("WRITE_SITES", len(sites), 20),
              ("claims()", len(claims), 5), ("#[tauri::command]", len(cmd_addr), 100)]
    print(head("§S0 地板（反向自检 —— 这几个数塌了，下面每一条都会空真地绿）"))
    for name, got, floor in floors:
        print(f"  {name:<20s} 现打 {got:4d}  地板 {floor}")
        if got < floor:
            crash(f"{name} 现打 {got} < 地板 {floor}")

    section_installable(reg_src, tools)

    # ── §S2 声明侧 ────────────────────────────────────────────────────────────
    print(head("§S2 声明侧 —— `TOOLS` ＋ `UNMANAGED_ENV`"))
    print(f"  `TOOLS` {len(tools)} 条：")
    for tid, inst, unin in tools:
        print(f"    {tid:<22s} installable={str(inst):<5s} uninstallable={unin}")
    print(f"  `UNMANAGED_ENV` {len(unmanaged)} 条，按 `who` 分："
          f"{dict(Counter(w for _, w in unmanaged))}")
    for uid, who in unmanaged:
        if who == "AppShips":
            print(f"    · `{uid}` = `Provisioning::AppShips`"
                  f"（app 独有、该我们装；有没有装口由 `EnvTier::of` 另算）")

    print(f"  `claims()` {len(claims)} 条（工具 → 装 / 卸实现住址）：")
    for tool, ins, unins in claims:
        print(f"    {tool:<22s} install={ins}  uninstall={unins}")

    # ── §S3 动作侧 ────────────────────────────────────────────────────────────
    caps = OrderedDict()
    for cmd, cap, side in ledger:
        caps.setdefault(cap, []).append((cmd, side))
    print(head("§S3 动作侧 —— `LEDGER`"))
    print(f"  {len(ledger)} 行命令 / {len(caps)} 个能力 id")
    print(f"  `Side` 分布：{dict(Counter(s for _, _, s in ledger))}")

    # ── §S4 写盘落点 ──────────────────────────────────────────────────────────
    with_tool = [(f, n, t) for f, n, t in sites if t]
    print(head("§S4 写盘落点 —— `WRITE_SITES`"))
    print(f"  {len(sites)} 行，其中**带 tool id** 的 {len(with_tool)} 行：")
    tool_ids = {t for t, _, _ in tools}
    for f, n, t in with_tool:
        flag = "" if t in tool_ids else "   ⚠ 这个 id 不在 TOOLS 里"
        print(f"    {f}::{n}  → `{t}`{flag}")
    # claims() 与 WRITE_SITES 对同一处的归属说法一致吗（两张表都是源码里的权威源）
    claim_by_addr = {}
    for tool, ins, unins in claims:
        for a in (ins, unins):
            if a:
                claim_by_addr.setdefault(a, set()).add(tool)
    print("  ⚠ 两张表对同一处的归属 —— 对不上的逐处点名（本仓今天**没有**判据在对拍这一格）：")
    conflicts = 0
    for f, n, t in with_tool:
        addr = f"{f}::{n}"
        if addr in claim_by_addr and t not in claim_by_addr[addr]:
            conflicts += 1
            print(f"    · `{addr}`：`WRITE_SITES` 说它是 `{t}` 的安装动作，"
                  f"而 `claims()` 说这个符号是 {sorted(claim_by_addr[addr])} 的实现")
    print(f"    （现打 {conflicts} 处对不上）")

    # ── §S5 安装面人群 ＋ 归档（R3 / R4 / R5） ─────────────────────────────────
    print(head("§S5 安装面人群 ＋ 归档"))

    # 种子：从 `claims()` 派生 —— 这几条是「盘上确凿的装 / 卸实现」
    ledger_cmds = {c: cap for c, cap, _ in ledger}
    seed_caps, seed_rows = set(), []
    for tool, ins, unins in claims:
        for a in (ins, unins):
            if not a or "::" not in a:
                continue
            sym = a.split("::")[-1]
            if sym in ledger_cmds:
                seed_caps.add(ledger_cmds[sym])
                seed_rows.append((tool, a, ledger_cmds[sym]))
    print(f"  种子（`claims()` 的装 / 卸实现符号里，同时是 Tauri 命令的）{len(seed_rows)} 条"
          f" ⇒ {len(seed_caps)} 个能力 id：")
    for tool, a, cap in sorted(seed_rows):
        print(f"    `{a}`  →  能力 `{cap}`（工具 `{tool}`）")

    # ═══ DV-ARCHIVE-GATES-BEGIN ═══（死值验刀③「表整段拿掉」摘的就是这两块之间的东西）
    # R3 两向对拍
    missing = [c for c in caps if c not in CAP_ARCHIVE]
    stale = [c for c in CAP_ARCHIVE if c not in caps]
    if missing:
        red("R3a", "这些能力 id 在 `LEDGER` 里而**没人归档**（新长出来的装口就从这里红）："
                   + ", ".join(sorted(missing)))
    if stale:
        red("R3b", "归档表里这些能力 id 今天不在 `LEDGER` 里（表在腐烂）："
                   + ", ".join(sorted(stale)))
    bad_override = [c for c in CMD_OVERRIDE if c not in ledger_cmds]
    if bad_override:
        red("R3c", "`CMD_OVERRIDE` 里这些命令名不在 `LEDGER` 里：" + ", ".join(bad_override))

    # R4 种子不许被划出安装面
    for cap in sorted(seed_caps):
        got = CAP_ARCHIVE.get(cap, (None,))[0]
        if got not in (B1, B2, B3):
            red("R4", f"能力 `{cap}` 有一条**盘上确凿的装 / 卸实现**（见种子表），"
                      f"却被归成 `{got}`")

    # ═══ DV-ARCHIVE-GATES-END ═══

    # 逐条落哪一处（21+ 的那张表，住址从源码派生）
    rows = []
    for cmd, cap, side in ledger:
        b, why, note = CMD_OVERRIDE.get(cmd, CAP_ARCHIVE.get(cap, (NA, "?", "?")))
        if b == NA:
            continue
        addr = cmd_addr.get(cmd)
        if addr is None:
            red("R6", f"命令 `{cmd}` 在 `LEDGER` 里，却在 `#[tauri::command]` 面上找不到")
            addr = "（找不到）"
        rows.append((b, cap, cmd, side, addr, why))
    rows.sort(key=lambda r: (BUCKETS.index(r[0]), r[1], r[2]))
    print("\n  ── Tauri 命令这一半（逐条：今天住哪 · 归哪一处 · 依据）──")
    print(f"  {'归处':<14s}{'能力 id':<24s}{'命令':<32s}{'Side':<8s}{'今天住哪':<30s}依据")
    for b, cap, cmd, side, addr, why in rows:
        print(f"  {b:<14s}{cap:<24s}{cmd:<32s}{side:<8s}{addr:<30s}{why}")
    bycmd = Counter(r[0] for r in rows)
    print(f"  合计 **{len(rows)} 条**："
          + " · ".join(f"{b} {bycmd.get(b, 0)}" for b in (B1, B2, B3)))
    cap_set = sorted({r[1] for r in rows})
    file_set = sorted({r[4].split(":")[0] for r in rows})
    print(f"  跨 **{len(cap_set)} 个能力 id** / **{len(file_set)} 份后端文件**")
    print(f"    能力 id：{', '.join(cap_set)}")
    print(f"    文件：{', '.join(file_set)}")

    # ═══ DV-ARCHIVE-GATES-2-BEGIN ═══
    # R5 写盘落点两向对拍
    site_keys = {f"{f}::{n}" for f, n, _ in with_tool} | set(SITE_TOOLID_NONE_BUT_ARCHIVED)
    s_missing = sorted(site_keys - set(SITE_ARCHIVE))
    s_stale = sorted(set(SITE_ARCHIVE) - site_keys)
    if s_missing:
        red("R5a", "这些写盘落点申报成了某个工具的安装动作，而**没人归档**：" + ", ".join(s_missing))
    if s_stale:
        red("R5b", "归档表里这些写盘落点今天不在人群里：" + ", ".join(s_stale))
    # ═══ DV-ARCHIVE-GATES-2-END ═══
    all_sites = {f"{f}::{n}": t for f, n, t in sites}
    print("\n  ── 非 Tauri 命令的内部动作这一半 ──")
    print(f"  {'归处':<14s}{'落点':<50s}{'WRITE_SITES 的 tool id':<26s}依据")
    for k in sorted(SITE_ARCHIVE, key=lambda k: (BUCKETS.index(SITE_ARCHIVE[k][0]), k)):
        b, why, _note = SITE_ARCHIVE[k]
        tid = all_sites.get(k, "（不在 WRITE_SITES 里）")
        print(f"  {b:<14s}{k:<50s}{str(tid):<26s}{why}")
    bysite = Counter(v[0] for v in SITE_ARCHIVE.values())
    print(f"  合计 **{len(SITE_ARCHIVE)} 处**："
          + " · ".join(f"{b} {bysite.get(b, 0)}" for b in (B1, B2, B3)))

    print(f"\n  ⇒ **人群合计 {len(rows)} 条命令 ＋ {len(SITE_ARCHIVE)} 处内部动作"
          f" = {len(rows) + len(SITE_ARCHIVE)} 项**（现打；分母 = `LEDGER` {len(ledger)} 行 ＋"
          f" `WRITE_SITES` {len(sites)} 行）")

    # ── §S5b 前端落点（切件的写区要按这个切，别手写） ─────────────────────────
    print(head("§S5b 前端落点 —— 每条安装面命令在 `src/**.ts` 里被谁引用"))
    print("  ⚠ 分母 = `<root>/src` 下的 `.ts`，**排除** `*.vitest.ts` 与 `src/generated/`；"
          "`src/ipc/commands.ts` 是**共用包装层**（每条都在它里面），单列不重复印。")
    ts_root = os.path.join(root, "src")
    ts_files = []
    if os.path.isdir(ts_root):
        for base, _d, files in os.walk(ts_root):
            if os.sep + "generated" in base:
                continue
            for fn in sorted(files):
                if fn.endswith(".ts") and not fn.endswith(".vitest.ts"):
                    ts_files.append(os.path.join(base, fn))
    if len(ts_files) < 30:
        print(f"  （`{ts_root}` 下只有 {len(ts_files)} 份 .ts —— 本节跳过，"
              f"多半是 `--root` 指到了只有 src-tauri 的夹具）")
    else:
        texts = {os.path.relpath(f, root).replace("\\", "/"): slurp(f) for f in ts_files}
        wrapper = "src/ipc/commands.ts"
        by_file = {}
        for _b, _cap, cmd, _s, _a, _w in rows:
            hits = [rel for rel, t in texts.items()
                    if rel != wrapper and re.search(r"\b" + re.escape(cmd) + r"\b", t)]
            for h in hits:
                by_file.setdefault(h, []).append(cmd)
            in_wrapper = re.search(r"\b" + re.escape(cmd) + r"\b", texts.get(wrapper, ""))
            print(f"    {cmd:<32s}{'包装层✓' if in_wrapper else '包装层✗'}  "
                  f"{', '.join(hits) if hits else '（前端没有非测试引用）'}")
        print(f"  ⇒ 前端落点 **{len(by_file)} 份文件**（不含共用包装层 `{wrapper}`）：")
        for f in sorted(by_file):
            print(f"    {f:<40s} {len(by_file[f]):2d} 条：{', '.join(sorted(by_file[f]))}")

    # ── §S6 `Side` 栏复量（D4：只量代价，不许翻） ───────────────────────────────
    print(head("§S6 `Side` 栏 —— 复量（纯数据那一半；派生那一半见 docstring 的 B3）"))
    remote = [c for c, _, s in ledger if s == "Remote"]
    print(f"  `Side::Remote` 现打 **{len(remote)}** 行（分母 = `LEDGER` {len(ledger)} 行）")
    seg = const_block(led_src, "const REMOTE_SIDE_SIGNOFF")
    signoff = re.findall(r"\(Derived::(\w+), (\d+)\)", seg)
    print("  签字表 `REMOTE_SIDE_SIGNOFF`：" + " · ".join(f"{k} {v}" for k, v in signoff)
          + f"（合计 {sum(int(v) for _, v in signoff)}）")
    if sum(int(v) for _, v in signoff) != len(remote):
        red("R7a", "签字表合计 ≠ `Side::Remote` 行数")
    seg = const_block(led_src, "const FRAME_PLANE_VERDICTS")
    verdicts = re.findall(r'"([A-Za-z0-9_]+)"\s*,\s*FrameVerdict::(\w+)', seg)
    lies = [c for c, v in verdicts if v == "LiesTodayOwedACorrection"]
    print(f"  人裁表 `FRAME_PLANE_VERDICTS` {len(verdicts)} 条，其中"
          f"「说假话·欠一次订正」**{len(lies)}** 条：{', '.join(lies)}")
    print("  连锁四处，现打在不在：")
    chain = [
        ("EXPECTED_LOCAL_OR_BOTH",
         re.search(r"const EXPECTED_LOCAL_OR_BOTH: usize = (\d+);", led_src)),
        ("ORIGIN_TAKING_BOTH", re.search(r"const ORIGIN_TAKING_BOTH", led_src)),
        ("tmux.manage 那条 ASYMMETRY_REASONS",
         re.search(r'\("tmux\.manage", Asym::', led_src)),
        ("the_tmux_manage_row_stops_waiting_for_a_daemon_primitive",
         re.search(r"fn the_tmux_manage_row_stops_waiting_for_a_daemon_primitive", led_src)),
    ]
    for name, m in chain:
        if m is None:
            red("R7b", f"连锁那一处找不到了：{name}")
            print(f"    ✗ {name}")
        else:
            extra = ""
            if name == "EXPECTED_LOCAL_OR_BOTH":
                extra = f" = {m.group(1)}（翻 5 行 ⇒ {int(m.group(1)) + len(lies)}）"
            if name == "ORIGIN_TAKING_BOTH":
                n = len(re.findall(r'\n        \(\n            "([A-Za-z0-9_]+)",',
                                   const_block(led_src, "const ORIGIN_TAKING_BOTH")))
                extra = f" 现打 {n} 条（翻 5 行 ⇒ 要逐条判要不要 +5）"
            print(f"    ✓ {name}{extra}")

    # 第五处：`refuse_local_write` 的生产调用点 —— 散文里点名的比盘上多
    # 剥法只做**最粗**的一刀：列 0 的第一个 `#[cfg(test)]` 之后一律算测试段。
    # ⚠ 它比 `guard_core::production_code` 弱（不剥块注释、不认 `pub(crate) mod tests`）——
    #    写出来是因为下面这个数要拿去跟散文对拍，**剥不干净会少数**，所以两档都印。
    calls, calls_test = [], []
    for base, _d, files in os.walk(src_dir):
        for fn in sorted(files):
            if not fn.endswith(".rs"):
                continue
            p = os.path.join(base, fn)
            ls = slurp(p).split("\n")
            cut = next((i for i, l in enumerate(ls) if l == "#[cfg(test)]"), len(ls))
            for i, l in enumerate(ls):
                if "refuse_local_write(&origin," in l and not l.strip().startswith("//"):
                    (calls if i < cut else calls_test).append(
                        f"{os.path.relpath(p, src_dir)}:{i + 1}")
    print(f"  ⚠ 现打 `refuse_local_write(&origin,` 的**生产段**调用点 **{len(calls)}** 处："
          f"{', '.join(calls) if calls else '（零）'}"
          f"；测试段另有 {len(calls_test)} 处（{', '.join(calls_test)}）")
    print("    —— 而 `ASYMMETRY_REASONS` 的 `cc-bus.cockpit` 那条散文今天仍逐字写着"
          "「`cc_bus_spawn` / `cc_bus_kill` 对 `<local>` 走 `refuse_local_write`」。")

    # ── 裁决 ──────────────────────────────────────────────────────────────────
    print(head("裁决"))
    if RED:
        for tag, msg in RED:
            print(f"  RED [{tag}] {msg}")
        print(f"\nRULER: FAIL（{len(RED)} 条）")
        return 1
    print("  没有红。⚠ 「没有红」只说明上面那七条判定过了，**不说明归档表的每一格判断是对的**"
          "（诚实边界 B1）。")
    print("\nRULER: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
