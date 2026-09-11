#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R57 摸底量具 B：**装的口 / 查的口 / 卸的口，本机侧与远端侧各有几个** —— 代码侧普查。

住址（唯一）：`evidence/K-R57-B-port-census.py`。
被测对象：**本工作树**（`.claude/worktrees/k-r57`，分支 `track/k-r57`）—— 量具自己算出仓根，
          并把量到的那棵树的 `git rev-parse HEAD` 一起印出来（纪律 12·「被测对象指向哪棵树」）。
读数落点：`evidence/K-R57-B-port-census.out`。

# 分母怎么来的（纪律 12：报的每个数都要写明分母怎么数的）

① **全部 IPC 命令**：`src-tauri/src/lib.rs` 的 `generate_handler![...]` 块，逐行取名。
   —— 这是「app 对外一共有几个口」的唯一分母，**不是**手数的。
② **每个口服务哪一侧**：`src-tauri/src/parity_ledger.rs` 的 `LEDGER` 三元组
   `("命令", "能力", Side::X)`。⚠ 那张表由一条判据钉着「改 generate_handler 就必须来改它」，
   ⇒ 本量具**先对拍两张表**，不一致就当场把差集印出来 —— 差集非空时下面按 Side 分的数**不可信**。
③ **口的种类**（装 / 卸 / 查）：按命令名的动词分类，**分类规则写在 `VERBS` 里、逐条可核**。
   ⚠ 这一档是**本量具的判断**，不是盘上登记的事实 ⇒ 未命中任何动词的一律进 `其它`，
   并把 `其它` 整份印出来，让读者自己核有没有漏分。

# 断自己的前提（纪律 5）

- `generate_handler!` 块取不到 ⇒ 直接 `SystemExit`，**不出 0**。
- `LEDGER` 取到 0 条 ⇒ 直接 `SystemExit`，**不出 0**。
两处「没查到」与「没有」同形的坑都在这里堵掉。

# 它不答什么

**不答「这个口装得上装不上」** —— 那是行为，要真跑。本量具只答「盘上有没有这个口、它服务哪一侧」。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "src-tauri" / "src" / "lib.rs"
LEDGER_RS = ROOT / "src-tauri" / "src" / "parity_ledger.rs"
OUT = []


def w(s=""):
    OUT.append(s)


# ------------------------------------------------------------------ ① 全部命令

src = LIB.read_text(encoding="utf-8")
m = re.search(r"generate_handler!\[(.*?)\n\s*\]\)", src, re.S)
if not m:
    raise SystemExit("❌ 取不到 generate_handler! 块 —— 分母不成立，**不出 0**，量具停")
block = m.group(1)
CMDS = []
for line in block.splitlines():
    line = line.split("//")[0].strip().rstrip(",")
    if not line or line.startswith("/*") or line.startswith("*"):
        continue
    name = line.split("::")[-1]
    if re.fullmatch(r"[a-z_][a-z0-9_]*", name):
        CMDS.append(name)
if not CMDS:
    raise SystemExit("❌ generate_handler! 块解析出 0 个命令 —— 分母不成立，量具停")

# ------------------------------------------------------------------ ② LEDGER

led_src = LEDGER_RS.read_text(encoding="utf-8")
LED = dict(
    (a, c)
    for a, _b, c in re.findall(
        r'\(\s*"([a-z_0-9]+)"\s*,\s*"([^"]+)"\s*,\s*Side::(\w+)\s*,?\s*\)', led_src
    )
)
if not LED:
    raise SystemExit("❌ LEDGER 解析出 0 条 —— 分母不成立，量具停")

# ------------------------------------------------------------------ ③ 动词分类

VERBS = [
    # ⚠ 顺序有意义：「卸」必须排在「装」前面 —— `cc_integration_uninstall` 同时命中
    #   `_install$`（`uninstall` 的尾巴就是 `install`）与 `_uninstall$`，
    #   先判装就会把一个卸口数进装口。〔本量具第一版真的这么错过一次，读数从 5:4 变形〕
    ("卸", re.compile(r"^(uninstall_|remove_)|_uninstall$")),
    ("装", re.compile(r"^(write_|deploy_|install_)|_install$")),
    # 「只生成待贴文本、不落盘」的口单列一档 —— `K34` 裁定三（写可以给按钮、删要用户自己来）
    # 正是落在这一档与「装」之间的分界上，合进任何一档都会把那条分界抹掉。
    ("生成待贴", re.compile(r"^render_|_shellinit$|_preview$")),
    ("查", re.compile(r"(_status$|^diagnose_|^check_|^probe_|_state$|_scan_path$)")),
]


def classify(n):
    for tag, rx in VERBS:
        if rx.search(n):
            return tag
    return "其它"


# ------------------------------------------------------------------ 报头

try:
    sha = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True
    ).stdout.strip()
    br = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True,
        text=True,
    ).stdout.strip()
except Exception as e:  # noqa: BLE001
    sha, br = f"<取不到：{e}>", "<取不到>"

w("=" * 78)
w("K-R57 摸底量具 B · 装的口 / 查的口 / 卸的口 —— 代码侧普查")
w("=" * 78)
w(f"量具住址 ：evidence/K-R57-B-port-census.py")
w(f"被测对象 ：{ROOT}")
w(f"           分支 {br} · sha {sha}")
w(f"分母①   ：`generate_handler![...]` 解析出 **{len(CMDS)}** 个 IPC 命令")
w(f"分母②   ：`parity_ledger::LEDGER` 解析出 **{len(LED)}** 条 (命令, 能力, Side)")
w("")

# ------------------------------------------------------------------ 两表对拍

only_h = sorted(set(CMDS) - set(LED))
only_l = sorted(set(LED) - set(CMDS))
w("─" * 78)
w("【0】两张表先对拍 —— 差集非空时，下面按 Side 分的数**不可信**")
w("─" * 78)
w(f"  只在 generate_handler 里、不在 LEDGER 里：{len(only_h)} 个 {only_h}")
w(f"  只在 LEDGER 里、不在 generate_handler 里：{len(only_l)} 个 {only_l}")
w(f"  ⇒ {'两表一致，下面的 Side 数可信' if not (only_h or only_l) else '⚠ 差集非空 —— 逐个看上面那两行'}")
w("")

# ------------------------------------------------------------------ 分档

buckets = {"装": [], "卸": [], "查": [], "生成待贴": [], "其它": []}
for c in CMDS:
    buckets[classify(c)].append(c)

w("─" * 78)
w("【1】按动词分档（分类规则住 `VERBS`，逐条可核）")
w("─" * 78)
for tag in ("装", "卸", "生成待贴", "查"):
    names = sorted(buckets[tag])
    w(f"  【{tag}】共 {len(names)} 个 / 分母 {len(CMDS)}")
    for n in names:
        side = LED.get(n, "❓不在 LEDGER 里")
        w(f"      {side:<12} {n}")
    w("")
w(f"  【其它】{len(buckets['其它'])} 个 —— **整份印出来**，让读者自己核有没有漏分：")
w("      " + "  ".join(sorted(buckets["其它"])))
w("")

# ------------------------------------------------------------------ 本机 vs 远端

w("─" * 78)
w("【2】🔴 正题所在 —— 装 / 卸 / 查 三档各自的**本机侧 : 远端侧**")
w("─" * 78)
w(f"  {'档':<4} {'Local':>6} {'Remote':>7} {'Both':>6} {'缺登记':>7}   本机 : 远端")
for tag in ("装", "卸", "生成待贴", "查"):
    names = buckets[tag]
    loc = [n for n in names if LED.get(n) == "Local"]
    rem = [n for n in names if LED.get(n) == "Remote"]
    bot = [n for n in names if LED.get(n) == "Both"]
    nol = [n for n in names if n not in LED]
    w(
        f"  {tag:<4} {len(loc):>6} {len(rem):>7} {len(bot):>6} {len(nol):>7}"
        f"   {len(loc)} : {len(rem)}"
    )
    w(f"       Local ：{sorted(loc)}")
    w(f"       Remote：{sorted(rem)}")
    if bot:
        w(f"       Both  ：{sorted(bot)}")
    if nol:
        w(f"       缺登记：{sorted(nol)}")
w("")
w("  ⚠ 口径：这几个数的人群是「**IPC 口**」，不是「**装得上的东西**」。")
w("    一个东西可能有 0 个 IPC 口而照样在盘上（用户自己装的），也可能有口而装不上。")
w("    ⇒ 拿这张表答「环境齐了没有」是**分母对不上**；它只答「app 今天伸出了几只手」。")
w("")

# ------------------------------------------------------------------ 受管工具表

TR = ROOT / "src-tauri" / "src" / "tool_registry.rs"
tr_src = TR.read_text(encoding="utf-8")
tools = re.findall(
    r'id:\s*"([^"]+)",.*?installable:\s*(true|false),\s*\n\s*uninstallable:\s*(true|false)',
    tr_src,
    re.S,
)
w("─" * 78)
w("【3】`tool_registry::TOOLS` —— 仓里**唯一一张**「受管工具」声明表，现打")
w("─" * 78)
if not tools:
    w("  ❌ 解析出 0 条 —— 分母不成立（正则没命中 ≠ 表是空的），本节判不了")
else:
    w(f"  分母：{len(tools)} 条")
    w(f"  {'id':<20} {'installable':<12} uninstallable")
    for tid, ins, uni in tools:
        w(f"  {tid:<20} {ins:<12} {uni}")
w("")
# ⚠ 反向表 `NOT_MANAGED` 的成员**本量具判不了**：第一版那条正则从
#   `tr_src.split("NOT_MANAGED")[-1]` 里捞元组，实得 8 条全是别处的字段
#   （`effect: TouchEffect::` / `needs_sudo: false` …）—— 正则没命中真成员，
#   却给出了一个**自信的错答案**（正是纪律 5 那一族）。⇒ 改成只报「解析得到的候选」，
#   并**显式声明这一格判不了**，要判得了缺的是：拿 `syn` 之类真解析 Rust const 数组。
nm_block = re.search(r"pub const NOT_MANAGED: &\[\(&str, &str\)\] = &\[(.*?)\n\];", tr_src, re.S)
if nm_block:
    nm = re.findall(r'^\s{8}"([a-z0-9][a-z0-9-]*)",\s*$', nm_block.group(1), re.M)
    w(f"  反向表 `NOT_MANAGED`（刻意不收的）：解析出 {len(nm)} 条 {nm}")
    if not nm:
        w("    ⚠ **判不了** —— 块取到了但成员正则零命中（「没查到」≠「没有」）")
else:
    w("  反向表 `NOT_MANAGED`：**判不了** —— 取不到那个 const 块")
w("")
w("  ⚠ 这张表**自己的头注**逐字写着它守不住什么：")
w("    「新增一个『会在用户机器上留下东西』的写点，**没有任何东西逼它申报**」")
w("    ⇒ 它是「**人现在记得**的清单」，不是「**真实发生的安装动作**」的清单。")
w("    ⇒ 拿它当「环境清单」用之前，必须先与【1】那张口表对拍（见下一节）。")
w("")

# ------------------------------------------------------------------ 对拍：表 vs 口

w("─" * 78)
w("【4】🔴 表与口对拍 —— 「表里说装不了」而「口真的在」的，逐条点名")
w("─" * 78)
PORT_OF_TOOL = {
    "ccm": ["install_remote_ccm_helper", "uninstall_remote_ccm_helper"],
    "cc-bus": ["deploy_local_cc_bus", "cc_bus_install_state"],
    "cc-acct-iso": ["deploy_remote_acct_iso", "check_remote_acct_iso"],
    "remote-daemon": ["deploy_remote_daemon", "uninstall_remote_daemon"],
    "project-mcp": ["write_project_mcp_server", "remove_project_mcp_server"],
    "powershell-profile": ["cc_integration_install", "cc_integration_uninstall"],
}
w("  映射住本量具的 `PORT_OF_TOOL`（**人手写的**，不是盘上登记 —— 如实记）")
for tid, ins, uni in tools:
    ports = PORT_OF_TOOL.get(tid, [])
    live = [p for p in ports if p in CMDS]
    flag = ""
    if ins == "false" and live:
        flag = "  🔴 表说 installable:false，而口真的在"
    w(f"  {tid:<20} installable={ins:<6} 盘上真有的口：{live}{flag}")
w("")

text = "\n".join(OUT) + "\n"
Path(__file__).with_suffix(".out").write_text(text, encoding="utf-8")
print(text)
