#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R67 摸底量具（第二把）—— **代码仓那一侧**的现打读数，一条命令重打全部。

用法：  python3 K-R67-code-readings.py [代码工作树根]   # 默认 = 本文件的上一级
被测对象：cc-monitor 工作树（**不是**计划仓；计划仓那一侧由 `K-R67-census.py` 量）。

# 它买什么

`K-R67-依赖脊柱与顺序.md` 里每一个关于**代码**的数，在这里都有一行可重打的读数。
每一行自己印出**分母怎么数的**；`?` 表示这一行没量到（路径不在 ⇒ 当场喊，不静默算 0）。

⚠ **只读。** 一个字节都不写被测树。
"""
import os
import re
import subprocess
import sys
import time


def sh(cmd, cwd):
    p = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    return p.returncode, p.stdout.rstrip("\n")


def main() -> int:
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    rc, head = sh("git rev-parse HEAD", root)
    rc2, tree = sh("git rev-parse 'HEAD^{tree}'", root)
    rc3, dirty = sh("git status --porcelain", root)
    print("量于 %s · 被测对象 %s" % (time.strftime("%Y-%m-%d %H:%M:%S %z"), root))
    print("  HEAD=%s  HEAD^{tree}=%s" % (head, tree))
    if not dirty:
        print("  工作树：干净")
    else:
        ls = dirty.splitlines()
        ev = [l for l in ls if "evidence/" in l]
        print("  工作树：%d 项未提交（其中 evidence/ 下 %d 项 —— 本件的产出就住那里）" % (len(ls), len(ev)))
        for l in ls:
            print("      %s" % l)
    print()

    def line(tag, val, how):
        print("【%s】%s\n    分母/量法：%s" % (tag, val, how))

    # ① monitor 侧 backend/ 的能力线
    bd = os.path.join(root, "src-tauri", "src", "backend")
    if not os.path.isdir(bd):
        line("本机后端边界", "? 目录不在", bd)
    else:
        subs = sorted(d for d in os.listdir(bd) if os.path.isdir(os.path.join(bd, d)))
        n = sum(len(fs) for _, _, fs in os.walk(bd))
        line("本机后端边界 src-tauri/src/backend/",
             "子目录 %s · 文件 %d 项 · observe/ %s" % (subs, n, "在" if "observe" in subs else "🔴 不在"),
             "os.walk 全部文件（含 fixtures）；子目录 = 一层 isdir")

    # ② daemon 侧的能力线（对照）
    dd = os.path.join(root, "remote-daemon-proto", "src")
    if not os.path.isdir(dd):
        line("后端本体", "? 目录不在", dd)
    else:
        subs = sorted(d for d in os.listdir(dd) if os.path.isdir(os.path.join(dd, d)))
        line("后端本体 remote-daemon-proto/src/", "能力线 %s" % subs, "一层 isdir")

    # ③ gate.sh 行数（K-R22 停车理由里那个数的第三次重打）
    g = os.path.join(root, "scripts", "gate.sh")
    line("scripts/gate.sh 行数", (str(sum(1 for _ in open(g, encoding="utf-8"))) if os.path.exists(g) else "? 不在"),
         "wc -l 口径；停车理由 parked_全文.K-R22 写 478→627，09-05 订正 891")

    # ④ 项目里有没有 pb.py（K-R11 停车理由）
    # ⚠ 作用域：要在**项目根**上跑，不是在工作树上跑 —— 本仓最高频的一类错就是尺子的作用域对不上。
    proj = root
    while os.path.basename(proj) != "claudecode-frontend" and os.path.dirname(proj) != proj:
        proj = os.path.dirname(proj)
    if os.path.basename(proj) != "claudecode-frontend":
        line("项目里 pb.py 命中", "? 找不到项目根", "从 %s 往上找名为 claudecode-frontend 的目录，没找到 ⇒ 这一格不出数" % root)
    else:
        rc, out = sh("find . -name pb.py -not -path '*/node_modules/*' 2>/dev/null | wc -l", proj)
        line("项目里 pb.py 命中", out, "find -name pb.py，排 node_modules；**跑于项目根** %s" % proj)

    # ⑤ 「装 ccm」这件事在闭集里有几个落点（§D-1 ②）
    #    这一格刻意不数「提到 local/bin 的行」—— 那数的是提及，不是安装口。
    rc, out = sh(r"""grep -n 'destination: ToolDestination::' src-tauri/src/tool_registry.rs""", root)
    decls = [l for l in out.splitlines() if l.strip()]
    ccm = [l for l in decls if "/ccm" in l]
    localccm = [l for l in ccm if "LocalHome" in l]
    line("闭集 TOOLS 里的 destination 声明", "共 %d 条；其中落点是 `…/ccm` 的 %d 条；其中**本机**的 %d 条"
         % (len(decls), len(ccm), len(localccm)),
         "grep 'destination: ToolDestination::' src-tauri/src/tool_registry.rs 逐行；"
         "分母 = 那张闭集表里显式写了 destination 的行。⇒ `ccm` 只有远端一个落点，本机零个")
    for l in ccm:
        print("      · %s" % l.strip())
    rc, out = sh(r"""grep -n 'CCM_CLI_REMOTE_PATH: ' src-tauri/src/sftp.rs""", root)
    line("远端 ccm 的落点常量", out or "? 零命中", "grep 'CCM_CLI_REMOTE_PATH: ' src-tauri/src/sftp.rs")
    rc, out = sh(r"""grep -n 'exec {} ccm' src-tauri/src/sftp.rs""", root)
    line("远端 ccm 入口 shim（三行、零实现）", out or "? 零命中", "grep 'exec {} ccm' src-tauri/src/sftp.rs::ccm_entry_shim")

    # ⑥ 别名生成器吐的那一行（§D-1 ③）
    rc, out = sh(r"""grep -n '() { ccm' src/launcher-diagnostics.ts""", root)
    line("别名生成器吐的行", out or "? 零命中", "grep '() { ccm' src/launcher-diagnostics.ts::buildAliasLine")

    # ⑦ record_death 的生产调用点（§C-3 第 1 条）
    rc, out = sh(r"""grep -n 'DEATH_RECORD_SITES' src-tauri/src/daemon_policy.rs | head -3""", root)
    line("record_death 生产调用点登记表", out or "? 零命中", "grep DEATH_RECORD_SITES src-tauri/src/daemon_policy.rs")

    # ⑧ spawn_detached / process_group（§C-3 第 7 条：推翻 P2d 那条依据）
    rc, out = sh(r"""grep -n 'fn spawn_detached\|process_group(0)\|fn reap_detached' src-tauri/src/local_daemon.rs""", root)
    line("本机 daemon 脱离那条路", out or "🔴 零命中",
         "grep 'fn spawn_detached|process_group(0)|fn reap_detached' src-tauri/src/local_daemon.rs；"
         "P2d〔control-parity〕当年判「结构性做不到」的依据是这几个词命中 0")

    # ⑨ hello 帧的 commands 集（§D-3：同一道跨仓门）
    rc, out = sh(r"""grep -n 'inbound::COMMANDS' remote-daemon-proto/src/wire.rs | head -3""", root)
    line("hello 帧 commands 的取值空间", out or "? 零命中", "grep inbound::COMMANDS remote-daemon-proto/src/wire.rs")

    # ⑩ 本机读面棘轮里还剩几条 reader（§F 第 4 波 b）
    f = os.path.join(root, "src-tauri", "src", "local_read_surface_registry.rs")
    if os.path.exists(f):
        txt = open(f, encoding="utf-8").read()
        n = len(re.findall(r'^\s*"reader",\s*$', txt, re.M))
        line("local_read_surface_registry 里 `reader` 条数", str(n),
             '正则 ^\\s*"reader",\\s*$ 逐行数（登记表里每条工具占一行）；'
             "`hub`/`payload`/`remote`/`non-read` 不进这个分母")
    else:
        line("local_read_surface_registry", "? 文件不在", f)

    # ⑪ 终端集成那一块的宿主闸（§D-1 旁证 / §F 第 7 波）
    rc, out = sh(r"""grep -n -A3 'CC_INTEGRATION_HOST_OS: readonly HostOs' src/settings/panel.ts""", root)
    line("设置面板「终端集成」的宿主闸", out or "? 零命中", "grep CC_INTEGRATION_HOST_OS src/settings/panel.ts")

    return 0


if __name__ == "__main__":
    sys.exit(main())
