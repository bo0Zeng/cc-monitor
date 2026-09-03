#!/usr/bin/env python3
"""K-G4 · C 实现拍：变异台 —— **在沙箱里**跑 `layering_guard` 那一族，宿主上零测试。

用法：
    python3 evidence/K-G4-C-sbx-cut.py <这一刀的标签>

它做三件事，缺一不可〔`brief` 第 7 · 第 8 条〕：
1. 把**锚点命中数**打出来。
   🔴 **诚实边界，别把这个数读大**：本脚本是在**切完之后**跑的 ⇒ 它数的是
   「这一刀切完，锚点还在不在」，**不是**「切之前它恰好唯一」。
   两种情形会让它印出与直觉相反的数，两种本轮都真发生了：
   - `M1` 印 **8** —— 我登记的锚点串（裸 `"control",`）本身在本文件就不唯一，
     那是**登记写松了**，而真正切的是一个两行的唯一串；
   - `M7` 印 **0** —— 那一刀改掉的**正是锚点自己**，切完当然找不到了。
   「切之前恰好一处」这件事本轮**由 `Edit` 工具兜底**：它对不唯一的 `old_string`
   直接报错、拒绝落地 ⇒ 每一刀切中的确实是唯一的一处。
   写出来是因为〔`brief` 113〕：表里记错一个锚点，下一轮谁也复不出这一刀。
2. 跑 `cargo test --bins layering_guard`，逐条印 `test ... ok/FAILED`；
3. 印**判定行数**（`test ... ok|FAILED` 那种行有几条）与**退出码** ——
   「判据没跑」与「跑了全绿」在终端上一模一样，只有这两个数分得开。

被测对象（写死，别跟着 cwd 漂）：
    /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-g4
沙箱与门禁同一个镜像 `ccmon-devbox:latest`、同一套挂载（照抄 `.claude/devbox/gate`），
差别只在入口命令：门禁跑 `scripts/gate.sh`，这里只跑那一族测试。
"""

import re
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
WT = PROJ + "/.claude/worktrees/k-g4"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
GUARD = WT + "/remote-daemon-proto/src/layering_guard.rs"
RELAY_MOD = WT + "/remote-daemon-proto/src/relay/mod.rs"

# 每一刀要断言的锚点：标签 -> (文件, 锚点串, 期望命中数)
ANCHORS = {
    "M0-baseline": (GUARD, "const RELAY_MUST_NOT_KNOW", 1),
    # ⚠ 第一趟这里写的是裸 `'"control",'` —— 它在本文件命中 **8** 次，不是 1 次。
    #   真正切的锚点是下面这个**两行**的串（`Edit` 对不唯一的串会直接报错，
    #   所以那一刀切的确实是唯一的一处），但**表里当时记错了** ⇒ 照那个记录复不出这一刀。
    #   〔`brief` 113：不记锚点，下一轮谁也复不出这一刀〕
    "M1": (GUARD, '"control",\n            "中转**不改变世界**', 1),
    "M2": (GUARD, "const RELAY_MUST_NOT_KNOW", 1),
    "M3": (GUARD, "const D2_REACHING_INTO_RELAY", 1),
    "M4": (GUARD, "const D3_PLUGIN_TO_RELAY", 1),
    "M5": (GUARD, "const D3_RELAY_TO_PLUGIN", 1),
    "M6": (GUARD, "fn violating_edges", 1),
    "M7": (GUARD, "files.len(),\n            tree,", 1),
    "M8": (RELAY_MOD, "mod bind_guard;", 1),
    "M9": (RELAY_MOD, "mod bind_guard;", 1),
    "M10": (GUARD, "const WHO_MAY_NOT_REACH_INTO_RELAY", 1),
}


def anchor_report(tag):
    if tag not in ANCHORS:
        print("· 本刀没登记锚点（%s）" % tag)
        return
    path, needle, want = ANCHORS[tag]
    with open(path, encoding="utf-8") as f:
        n = f.read().count(needle)
    ok = "OK" if n == want else "🔴 对不上"
    print("· 锚点 %r 于 %s 命中 %d 次（期望 %d）—— %s" % (needle, path.rsplit("/", 1)[-1], n, want, ok))


def run():
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", "%s:%s" % (PROJ, PROJ),
        "-v", "%s:%s:ro" % (SKILL, SKILL),
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", "CARGO_TARGET_DIR=%s/.claude/pm-targets/k-g4" % PROJ,
        "-e", "HOME=/home/zbl",
        "-w", WT,
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        "cd remote-daemon-proto && cargo test --bins layering_guard",
    ]
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


BAR = re.compile(r"^test [\w:]+ \.\.\. (ok|FAILED|ignored)", re.M)


def main():
    tag = sys.argv[1] if len(sys.argv) > 1 else "(未命名)"
    print("==== 这一刀：%s ====" % tag)
    anchor_report(tag)
    rc, out = run()
    bars = BAR.findall(out)
    for line in out.splitlines():
        if BAR.match(line) or "test result:" in line or line.startswith("error"):
            print("  " + line)
    print("· 判定行数 = %d（ok=%d FAILED=%d）" % (len(bars), bars.count("ok"), bars.count("FAILED")))
    print("· 退出码 = %d" % rc)
    if len(bars) == 0:
        print("  🔴 判定行 0 —— 按 CRASH 记，不许报「新红 0」〔brief 第 8 条〕")
    return 0


if __name__ == "__main__":
    sys.exit(main())
