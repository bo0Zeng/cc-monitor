#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R57 摸底量具 A：**这台机器上，app 要的环境齐了没有** —— 只读普查。

住址（唯一）：`evidence/K-R57-A-env-census.py`（工作树 `.claude/worktrees/k-r57`，分支 `track/k-r57`）。
被测对象：**用户这台机器的真实盘面**（不是任何工作树里的副本）。
读数落点：`evidence/K-R57-A-env-census.out`。

# 🔴 它一个字节都不写用户机器

全程只有 `os.stat` / `os.listdir` / `os.readlink` / `open(..., 'r')` / `shutil.which`。
**不执行任何用户机器上的程序**（连 `--version` 都不打）——
理由：`claude` / `ccm` / `cc-*` 这几个在 `--version` / `--help` 路径上是否写 `~/.claude`
本量具判不了，而红线是「一个字节都不许写」⇒ 宁可少一格读数，不赌。
⇒ 凡是「版本」那一栏，本量具一律出 `判不了（没敢执行）`，并写清缺什么才判得了。

# 🔴 量具断自己的前提（纪律 5：先断分母不是 0）

每一条 glob / 计数读数都先打印**它的分母怎么来的**：
- 目录不存在 ⇒ 出 `目录缺席`，**不出 0**（「没查到」与「没有」在这里必须不同形）；
- 目录存在 ⇒ 先印**该目录条目总数**，再印匹配数 ⇒ 匹配 0 而总数 >0 才叫「真的没有」。

# 分档

`在` / `不在` / `判不了`。三值，不许把第三档写成前两档中的任何一个。
"""

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path

HOME = Path(os.path.expanduser("~"))
OUT = []


def w(s=""):
    OUT.append(s)


def stamp():
    return time.strftime("%Y-%m-%d %H:%M:%S %z")


# ---------------------------------------------------------------- 基元（只读）


def describe(p: Path):
    """一条路径的只读画像。返回 (档, 一句话)。"""
    try:
        st = p.lstat()
    except FileNotFoundError:
        return "不在", "不存在"
    except PermissionError as e:
        return "判不了", f"stat 被拒：{e}"
    except OSError as e:
        return "判不了", f"stat 失败：{e}"
    kind = []
    if stat.S_ISLNK(st.st_mode):
        try:
            tgt = os.readlink(p)
        except OSError as e:
            tgt = f"<readlink 失败 {e}>"
        try:
            real = str(p.resolve())
            dangling = not p.exists()
        except OSError:
            real, dangling = "<resolve 失败>", True
        kind.append(f"符号链接 -> {tgt}（realpath {real}{'，悬空' if dangling else ''}）")
    elif stat.S_ISDIR(st.st_mode):
        kind.append("目录")
    elif stat.S_ISREG(st.st_mode):
        kind.append(f"普通文件 {st.st_size} 字节")
        if st.st_mode & 0o111:
            kind.append("可执行")
    else:
        kind.append(f"其它类型 mode={oct(st.st_mode)}")
    kind.append("mtime " + time.strftime("%Y-%m-%d %H:%M", time.localtime(st.st_mtime)))
    return "在", "；".join(kind)


def listdir_census(p: Path, pattern: str = None):
    """目录普查。**先给分母**：目录在不在、总条目数是多少，再给匹配数。"""
    if not p.exists():
        return None, f"目录缺席（{p}）—— 分母不成立，**不是 0**"
    if not p.is_dir():
        return None, f"`{p}` 存在但不是目录 —— 分母不成立"
    try:
        names = sorted(os.listdir(p))
    except OSError as e:
        return None, f"列目录失败：{e} —— 判不了"
    if pattern is None:
        return names, f"目录在，共 {len(names)} 项"
    rx = re.compile(pattern)
    hit = [n for n in names if rx.match(n)]
    return hit, f"目录在，共 {len(names)} 项（分母）；匹配 `{pattern}` 的 {len(hit)} 项"


def grep_file(p: Path, needles):
    """只读 grep。返回 {needle: [行号...]} 或 None（读不了）。"""
    try:
        txt = p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    lines = txt.splitlines()
    out = {}
    for n in needles:
        out[n] = [i + 1 for i, l in enumerate(lines) if n in l]
    return out, len(lines)


# ---------------------------------------------------------------- 报头

w("=" * 78)
w("K-R57 摸底量具 A · 这台机器上 app 要的环境齐了没有 —— 只读普查")
w("=" * 78)
w(f"量于          ：{stamp()}")
w(f"量具住址      ：evidence/K-R57-A-env-census.py（工作树 .claude/worktrees/k-r57）")
w(f"被测对象      ：用户这台机器的真实盘面（HOME={HOME}）")
w(f"python        ：{sys.version.split()[0]}")
w(f"写口           ：**零** —— 本量具不 open('w')、不执行用户机器上的任何程序")
w("分档           ：在 / 不在 / 判不了（三值，第三档不许写成前两档）")
w("")

# ---------------------------------------------------------------- 一 · 外部命令

w("─" * 78)
w("【一】app 要用到的**外部命令** —— 现打 PATH 解析（`shutil.which`，不执行）")
w("─" * 78)
w(f"PATH 分母：{len(os.environ.get('PATH','').split(os.pathsep))} 段")
w("")
EXT = [
    ("claude", "Claude Code CLI 本体", "起会话的被起对象"),
    ("tmux", "会话托管", "后端能力 `tmux`（`Refusal::MissingCap(\"tmux\")`）"),
    ("git", "skill_host / history", "skill 仓操作、历史面"),
    ("ssh", "远端那一侧", "`ssh_source.rs:6381` / 远端一切"),
    ("bash", "cc-bus / ccm_probe", "`ccm_probe.rs:138` `Command::new(\"bash\")`"),
    ("sh", "tmux.rs / account_usage", "`/bin/sh` 兜底"),
    ("pgrep", "cc_bus.rs:2778", "找进程"),
    # 🔴 POSIX 上**真正**被找的只有这一个：`launch.rs::TERMINAL_EXITS`（现打 1 项）。
    #    gnome-terminal / kitty / alacritty 三个**在生产段里被一条判据禁止出现**
    #    （`launch.rs` 那条「生产段出现终端模拟器就红」），下面三行留着只作对照，
    #    别把它们读成「app 会去找这三个」。
    ("xdg-terminal-exec", "launch.rs::TERMINAL_EXITS", "🔴 POSIX 上唯一的终端出口"),
    ("x-terminal-emulator", "曾在候选表里、D 阶段撤掉", "对照项，app 今天**不找**它"),
    ("gnome-terminal", "对照项", "app **不找**它（判据禁止具名终端进生产段）"),
    ("kitty", "对照项", "app **不找**它"),
    ("alacritty", "对照项", "app **不找**它"),
    ("xdg-open", "lib.rs:2329", "开链接"),
    ("icacls", "profile_installer.rs", "Windows 专用，Linux 上本就不该有"),
    ("wt.exe", "launch.rs:533", "Windows 专用"),
    ("powershell.exe", "launch.rs:544", "Windows 专用"),
    ("ccm", "旧那份（第 4 问）", "用户点名要能退役的那个"),
    ("node", "MCP server 常见宿主", "装 MCP 时多半要"),
    ("npx", "MCP server 常见宿主", "装 MCP 时多半要"),
    ("rg", "ripgrep", "查一下有没有（代码里没找到调用点，本行是对照）"),
]
for name, who, why in EXT:
    p = shutil.which(name)
    if p:
        d, detail = describe(Path(p))
        w(f"  在    {name:<16} {p}")
        w(f"        └ {detail}    [{who}]")
    else:
        w(f"  不在  {name:<16} （PATH 里解析不到）    [{who}]")
w("")
w("  ⚠ 版本一栏**判不了（没敢执行）** —— 红线是「一个字节都不许写用户机器」，")
w("    而本量具判不了 `claude --version` / `ccm --ccm-probe` 会不会顺手写 `~/.claude`。")
w("    要判得了，缺的是：在沙箱里各跑一次带 strace/inotify 的写面观测。")
w("")

# ---------------------------------------------------------------- 二 · claude_dir

w("─" * 78)
w("【二】`claude_dir` 解析 —— 照 `paths.rs::resolve_claude_dir` 的三级顺序现打")
w("─" * 78)
env_ccd = os.environ.get("CLAUDE_CONFIG_DIR")
w(f"  ① 用户配置覆盖（monitor settings）：本量具**判不了**（要读 monitor 的 user-data，")
w(f"     而那份路径由 `resolve_monitor_data_dir` 算 ⇒ `~/.claude/claudecode-frontend/`）")
mdd = HOME / ".claude" / "claudecode-frontend"
d, detail = describe(mdd)
w(f"     {d}  {mdd}  —— {detail}")
w(f"  ② 环境变量 CLAUDE_CONFIG_DIR：{env_ccd if env_ccd else '（未设）'}")
if env_ccd:
    d, detail = describe(Path(env_ccd))
    w(f"     {d}  {detail}")
w(f"  ③ 默认 `~/.claude`：")
d, detail = describe(HOME / ".claude")
w(f"     {d}  {HOME/'.claude'}  —— {detail}")
CLAUDE_DIR = Path(env_ccd) if (env_ccd and Path(env_ccd).exists()) else (HOME / ".claude")
w(f"  ⇒ 本量具按 `{CLAUDE_DIR}` 往下量（与 app 的②③两级同口径；①那级判不了，如实记）")
w("")

# ---------------------------------------------------------------- 三 · skills

w("─" * 78)
w("【三】skill 面 —— `<claude_dir>/skills/`")
w("─" * 78)
skills = CLAUDE_DIR / "skills"
names, note = listdir_census(skills)
w(f"  {note}")
if names is not None:
    w(f"  逐条：{', '.join(names) if names else '（空）'}")
    for k in ("cc-bus", "planned-build", "cc-acct-iso"):
        d, detail = describe(skills / k)
        w(f"  {d}  skills/{k}  —— {detail}")
    ccb = skills / "cc-bus"
    if ccb.is_dir():
        cnt = sum(len(f) for _, _, f in os.walk(ccb))
        w(f"      cc-bus 目录下文件数现打 {cnt}（`cc_bus_deploy::FILES` 内嵌 17 个 —— 两数不等不必然是病：装法可能另有产物）")
    bak, note2 = listdir_census(skills, r"cc-bus\.bak-")
    w(f"      备份目录（`cc-bus.bak-*`，`deploy_local_cc_bus` 留的）：{note2}")
w("")

# ---------------------------------------------------------------- 四 · settings.json 钩子

w("─" * 78)
w("【四】cc-bus 钩子 —— `<claude_dir>/settings.json`（`hooks_diag` 的口径，只读）")
w("─" * 78)
sj = CLAUDE_DIR / "settings.json"
d, detail = describe(sj)
w(f"  {d}  {sj}  —— {detail}")
if sj.exists():
    try:
        raw = sj.read_text(encoding="utf-8", errors="replace")
        v = json.loads(raw)
        hooks = v.get("hooks")
        if not isinstance(hooks, dict):
            w("  判不了  顶层有 JSON 但没有 `hooks` 对象 ⇒ 照 hooks_diag 口径是 NotInstalled")
        else:
            w(f"  hooks 段事件分母：{len(hooks)} 个（{', '.join(sorted(hooks))}）")
            for ev, needle in (("SessionStart", "cc-register"), ("Stop", "cc-bus-stop-hook")):
                blob = json.dumps(hooks.get(ev, []), ensure_ascii=False)
                got = needle in blob
                w(f"  {'在' if got else '不在'}  hooks.{ev} 里提到 `{needle}`：{got}")
                if got:
                    m = re.findall(r'"command"\s*:\s*"([^"]*)"', blob)
                    for c in m:
                        if needle in c:
                            w(f"        命令串逐字：{c}")
    except json.JSONDecodeError as e:
        w(f"  判不了  settings.json 不是合法 JSON：{e}")
    except OSError as e:
        w(f"  判不了  读不了：{e}")
w("")

# ---------------------------------------------------------------- 五 · ~/.local/bin

w("─" * 78)
w("【五】`~/.local/bin/` —— 旧那份 `ccm` 与 cc-bus 的 11 条软链都住这儿")
w("─" * 78)
lbin = HOME / ".local" / "bin"
names, note = listdir_census(lbin)
w(f"  {note}")
if names is not None:
    ccs = [n for n in names if n.startswith("cc-")]
    w(f"  `cc-*` 现打 {len(ccs)} 条（`tool_registry` 那条 touch 的注释写着「11 条 + cc-acct-iso ⇒ 计数偏大 1」）：")
    for n in ccs:
        d, detail = describe(lbin / n)
        w(f"      {d}  {n:<22} {detail}")
    for n in ("ccm", "cc-monitor", "claude"):
        d, detail = describe(lbin / n)
        w(f"  {d}  ~/.local/bin/{n}  —— {detail}")
w("")

# ---------------------------------------------------------------- 六 · ~/.cc-monitor

w("─" * 78)
w("【六】`~/.cc-monitor/` —— app **自己的**目录（后端自释放 + 账号别名文件）")
w("─" * 78)
ccm_dir = HOME / ".cc-monitor"
names, note = listdir_census(ccm_dir)
w(f"  {note}")
if names is not None:
    w(f"  逐条：{', '.join(names) if names else '（空）'}")
    binn, note2 = listdir_census(ccm_dir / "bin")
    w(f"  bin/：{note2}")
    if binn is not None:
        for n in binn:
            d, detail = describe(ccm_dir / "bin" / n)
            w(f"      {d}  {n:<40} {detail}")
        loc = [n for n in binn if n.startswith("cc-monitor-local-")]
        rem = [n for n in binn if n.startswith("cc-monitor-remote")]
        w(f"      ⇒ 本机自释放件（`cc-monitor-local-<build_id>`）现打 {len(loc)} 份；远端自部署件 {len(rem)} 份")
    d, detail = describe(ccm_dir / "account-aliases.sh")
    w(f"  {d}  ~/.cc-monitor/account-aliases.sh  —— {detail}")
w("")

# ---------------------------------------------------------------- 七 · cc-bus 运行期 / 账号库

w("─" * 78)
w("【七】运行期目录与账号库")
w("─" * 78)
for p, who in (
    (HOME / ".cc-bus", "cc-bus 运行期状态（inbox/名册/队列/日志）"),
    (HOME / ".claude-accts", "cc-acct-iso 账号库"),
):
    d, detail = describe(p)
    w(f"  {d}  {p}  —— {detail}   [{who}]")
    n2, note2 = listdir_census(p)
    w(f"        {note2}")
    if n2:
        w(f"        逐条：{', '.join(sorted(n2)[:30])}{' …' if len(n2) > 30 else ''}")
w("")

# ---------------------------------------------------------------- 八 · rc 文件（加 / 删两栏分开）

w("─" * 78)
w("【八】rc 文件 —— 🔴 `K34` 裁定三：**写可以给按钮，删要用户自己来** ⇒ 两栏分开量")
w("─" * 78)
RC = [".bashrc", ".zshrc", ".bash_profile", ".profile"]
FENCES = [
    ("# === cc-monitor BEGIN", "app 自己的 PowerShell/cc 块围栏（`profile_installer`）"),
    ("# === cc-monitor aliases BEGIN", "app 自己的账号别名 source 行围栏（`account_aliases`）"),
    ("# === ccm BEGIN", "远端那侧 `sftp::CCM_PROFILE_BEGIN` 形状（本机对照）"),
]
for rc in RC:
    p = HOME / rc
    d, detail = describe(p)
    w(f"  {d}  ~/{rc}  —— {detail}")
    if not p.is_file():
        continue
    res = grep_file(p, [f[0] for f in FENCES] + ["ccm", ".local/bin", "cc-acct-iso", "account-aliases"])
    if res is None:
        w("        判不了  读不了")
        continue
    hits, nlines = res
    w(f"        分母：{nlines} 行")
    for needle, who in FENCES:
        ln = hits[needle]
        w(f"        {'在' if ln else '不在'}  围栏 `{needle}`：{ln if ln else '零命中'}   [{who}]")
    for needle in ("ccm", ".local/bin", "cc-acct-iso", "account-aliases"):
        ln = hits[needle]
        w(f"        {'在' if ln else '不在'}  提到 `{needle}` 的行：{len(ln)} 行 {ln[:12]}")
    # 逐字把提到 ccm 的那几行印出来 —— 第 4 问「还有谁在用它」的直接证据
    if hits["ccm"]:
        try:
            lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
            for i in hits["ccm"][:20]:
                w(f"            L{i}: {lines[i-1].strip()[:160]}")
        except OSError:
            pass
w("")
w("  ⚠ 本节只量「rc 里今天有什么」。「谁能写进去 / 谁能删掉」是**代码侧**的读数，")
w("    住 `evidence/K-R57-B-port-census.out`，两者别混成一栏。")
w("")

# ---------------------------------------------------------------- 九 · 还有谁在用 ~/.local/bin/ccm

w("─" * 78)
w("【九】🔴 第 4 问 —— **还有谁在用 `~/.local/bin/ccm`**（现打，只读 grep）")
w("─" * 78)
CCM = HOME / ".local" / "bin" / "ccm"
d, detail = describe(CCM)
w(f"  被找的那个：{d}  {CCM}  —— {detail}")
if CCM.is_file():
    try:
        head = CCM.read_text(encoding="utf-8", errors="replace").splitlines()[:6]
        w("  头 6 行逐字：")
        for i, l in enumerate(head, 1):
            w(f"      L{i}: {l[:160]}")
    except OSError as e:
        w(f"  判不了  读不了头部：{e}")
w("")
w("  在下面这几片**只读**扫「提到 ccm 的文件」。每片先给分母（扫了几个文件）：")
SCAN = [
    (HOME / ".local" / "bin", "用户 PATH 上的脚本"),
    (CLAUDE_DIR / "skills", "已装的 skill（cc-bus 的 cc-spawn 就调 ccm）"),
    (HOME / ".cc-monitor", "app 自己的目录"),
]
RX = re.compile(r"\bccm\b")
for root, who in SCAN:
    if not root.exists():
        w(f"  [{who}] {root} —— 目录缺席，**分母不成立，不是 0**")
        continue
    scanned = 0
    hitfiles = []
    for dirpath, dirnames, filenames in os.walk(root):
        # 不跟符号链接出去；备份目录也扫（它们也可能还在被人用）
        for fn in filenames:
            fp = Path(dirpath) / fn
            try:
                if fp.is_symlink():
                    # 软链本身不读内容，读它指向的那份（同一份内容只算一次）
                    continue
                if fp.stat().st_size > 2_000_000:
                    continue
                txt = fp.read_text(encoding="utf-8", errors="ignore")
            except (OSError, ValueError):
                continue
            scanned += 1
            hl = [i + 1 for i, l in enumerate(txt.splitlines()) if RX.search(l)]
            if hl:
                hitfiles.append((fp, hl))
    w(f"  [{who}] {root}")
    w(f"      分母：读得动的普通文件 {scanned} 个（软链跳过、>2MB 跳过）")
    w(f"      命中 `\\bccm\\b` 的文件 {len(hitfiles)} 个")
    for fp, hl in sorted(hitfiles)[:40]:
        rel = str(fp).replace(str(HOME), "~")
        w(f"        · {rel}  （{len(hl)} 行：{hl[:8]}）")
w("")

# ---------------------------------------------------------------- 十 · app 本体装没装

w("─" * 78)
w("【十】app 本体今天在这台机器上装没装（决定「自释放」那条路走不走得到）")
w("─" * 78)
for cand in (
    Path("/usr/bin/cc-monitor"),
    Path("/usr/local/bin/cc-monitor"),
    Path("/opt/cc-monitor"),
    HOME / ".local" / "bin" / "cc-monitor",
    Path("/usr/lib/cc-monitor"),
):
    d, detail = describe(cand)
    w(f"  {d}  {cand}  —— {detail}")
p = shutil.which("cc-monitor")
w(f"  PATH 解析 `cc-monitor`：{p if p else '解析不到'}")
w("")

# ---------------------------------------------------------------- 十一 · skill 面对拍

w("─" * 78)
w("【十一】🔴 `K34` 点名的「装 skill」—— 盘上装着的 vs app 认识的 vs app 装得了的")
w("─" * 78)
SKILL_HOST = Path(__file__).resolve().parent.parent / "src-tauri" / "src" / "skill_host.rs"
declared = []
if SKILL_HOST.is_file():
    s = SKILL_HOST.read_text(encoding="utf-8")
    blk = re.search(r"pub const SKILLS: &\[SkillSpec\] = &\[(.*?)\n\];", s, re.S)
    if blk is None:
        w("  判不了  取不到 `SKILLS` const 块（「没查到」≠「没有」）")
    else:
        declared = re.findall(r'id:\s*"([^"]+)"', blk.group(1))
        inst = re.findall(r"install:\s*Install::(\w+)", blk.group(1))
        w(f"  app **认识**的 skill（`skill_host::SKILLS`）：{len(declared)} 条 {declared}")
        w(f"      各自的 install 档：{list(zip(declared, inst))}")
        w(f"      ⇒ 其中 `ManagedTool`（真装得了的）{inst.count('ManagedTool')} 条；"
          f"`NotSupported` {inst.count('NotSupported')} 条")
else:
    w("  判不了  找不到 `skill_host.rs`")
on_disk, note = listdir_census(skills)
if on_disk is None:
    w(f"  盘上装着的：{note} —— 本节的另一半分母不成立")
else:
    w(f"  盘上**装着**的 skill：{len(on_disk)} 条（分母 = `{skills}` 的目录项数）")
    if declared:
        w(f"  ⇒ 装着而 app **不认识**的：{sorted(set(on_disk) - set(declared))}")
        w(f"  ⇒ app 认识而盘上**没有**的：{sorted(set(declared) - set(on_disk))}")
w("")
w("  ⚠ 口径：「app 认识」= 它在 `SKILLS` 里有一条声明（于是界面能列它、能编它那个收件箱）。")
w("    「app 装得了」= 那条声明是 `ManagedTool(id)` 且 `TOOLS` 里那条真有落点。")
w("    两者都**不等于**「这个 skill 在这台机器上能跑」——那还要它自己的依赖齐。")
w("")

w("=" * 78)
w("读数完。三值分档，凡写「判不了」的都在同一行写了缺什么才判得了。")
w("=" * 78)

text = "\n".join(OUT) + "\n"
outp = Path(__file__).with_suffix(".out")
outp.write_text(text, encoding="utf-8")
print(text)
