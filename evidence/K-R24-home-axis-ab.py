#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R24 下一拍㈡ · `HOME` 这条轴的 A/B 量具（+ 两个网络口径的门禁读数）。

── 尺子（逐字沿用上一拍那把，只换轴）──────────────────────────────────
  「一条判据，它的**判决**随一条**环境事实**翻转，而它**既不建立、也不检查**那条事实。」
  判法：**同一棵树、同一个提交、同一个镜像、同一份挂载、同一个网络口径，
        只换 `HOME` 这一条环境事实，看哪些判决翻转。**
  🔴 刻意**不切成**「代码里出现了 `HOME` / `dirs::home_dir()`」—— 那是词表的形状，
     只认它记得的写法；上面这个是行为的形状，机器判得了。

── 为什么是 `HOME` 这条轴 ──────────────────────────────────────────
  `.claude/devbox/gate` 头注**自己写着**〔`K-H2b` `D9` 08-29〕：容器里 `HOME=/home/zbl`
  **存在但几乎是空的**（只有 `文档/claudecode-frontend` 挂进来）⇒ 任何读真实家目录的判据，
  **沙箱与宿主未必同值**。⇒ 这条轴仓里有账，但从没人拿尺子量过它上面有几条。

── 四格怎么切的 ─────────────────────────────────────────────────
  A  HOME=/home/zbl           + mkdir $HOME/.claude/projects   ← `.claude/devbox/gate` 逐字那套（基线）
  B  HOME=/home/zbl           + **不** mkdir                    ← 只撤掉「门禁替判据建的那个目录」
  C  HOME=/tmp/home-elsewhere + mkdir $HOME/.claude/projects   ← 家目录换个地方（目录仍在）
  D  HOME=/tmp/home-elsewhere + **不** mkdir                    ← 两样一起换
  ⚠ C/D 那条轴其实动了**两样**：家目录的**路径**变了，而且项目不再落在它下面。
    这是这条轴本身的形状，**记在这里，别读成只动了路径**。

── 每格跑三趟，因为三趟看见的粒度不同 ────────────────────────────────
  ① `bash scripts/gate.sh`   —— 门禁自己那九格的裁决（npm / e2e / pb check 只到**格**这一级）
  ②③ 两条 cargo 命令的**全量逐条输出** —— Rust 那两格要**逐条**判决才看得出是哪一条翻的

🔴 本量具**不改 `.claude/devbox/gate`**、**不给门禁放网络**（四格一律 `--network none`）。
   它是**另起一个 docker run**，逐字复刻 gate 的挂载与环境，**只把 `HOME` 那一维换掉**。

⚠ 为什么是 `.py` 而不是 `.sh`：本树有一条 `shell_lint_registry` 的默认拒绝判据 ——
  任何新增 `.sh` 只要不进 `ci.yml` 的 shellcheck 表达式、又没登记进那个 `EXEMPT`，
  它当场点名。而那两处**都不在本件写区**。⇒ 量具写成 `.py`
  （先例：`evidence/K-R5-C-r7-door.py`）。★ 那条判据是先跑门禁**逮住我**的，不是我先想到的。

用法：
  python3 evidence/K-R24-home-axis-ab.py <输出目录> [gates|cells|all]
"""
import os
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
WT = PROJ + "/.claude/worktrees/k-r24b"
TARGETS = PROJ + "/.claude/pm-targets/k-r24b"
IMAGE = "ccmon-devbox:latest"
PB_WS = "backend-consolidation"

# (格, HOME 的值, 要不要复刻 gate 那句 mkdir)
CELLS = [
    ("A", "/home/zbl", True),
    ("B", "/home/zbl", False),
    ("C", "/tmp/home-elsewhere", True),
    ("D", "/tmp/home-elsewhere", False),
]

# (名字, 容器内要跑的命令) —— ② ③ 两条逐字抄自 `scripts/gate.sh` 的 cargo/daemon 两格
STEPS = [
    ("gate", "bash scripts/gate.sh"),
    ("cargo", "cd src-tauri && cargo test --workspace --exclude code-picture-core --lib"),
    ("daemon", "cd remote-daemon-proto && cargo test"),
]


def run_cell(cell, home, do_mkdir, out_dir):
    """一格：逐字复刻 `.claude/devbox/gate` 的 docker run，只换 HOME 那一维。"""
    pre = 'mkdir -p "$HOME/.claude/projects" && ' if do_mkdir else ""
    print(f"── 格 {cell}：HOME={home} · mkdir={'有' if do_mkdir else '无'} · --network none ──",
          flush=True)
    for name, cmd in STEPS:
        argv = [
            "docker", "run", "--rm",
            "--network", "none",
            "-v", f"{PROJ}:{PROJ}",
            "-v", f"{SKILL}:{SKILL}:ro",
            "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={TARGETS}",
            "-e", f"HOME={home}",
            "-e", f"PB_WS={PB_WS}",
            "-w", WT,
            IMAGE,
            "bash", "-o", "pipefail", "-c", pre + cmd,
        ]
        log = os.path.join(out_dir, f"{cell}.{name}.log")
        with open(log, "wb") as fh:
            rc = subprocess.call(argv, stdout=fh, stderr=subprocess.STDOUT)
        print(f"   {name:<7} rc={rc}  ⇒ {log}", flush=True)


def run_gates(out_dir):
    """交回要的两个网络口径 —— **走真的 `.claude/devbox/gate`**，不是复刻品。"""
    for net in ("none", "host"):
        env = dict(os.environ, PB_WS=PB_WS, DEVBOX_NET=net)
        log = os.path.join(out_dir, f"gate-{net}.log")
        print(f"── 门禁 {net} 口径（真 gate）──", flush=True)
        with open(log, "wb") as fh:
            rc = subprocess.call(
                [".claude/devbox/gate", WT, "k-r24b"],
                cwd=PROJ, env=env, stdout=fh, stderr=subprocess.STDOUT,
            )
        print(f"   rc={rc}  ⇒ {log}", flush=True)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out_dir = sys.argv[1]
    what = sys.argv[2] if len(sys.argv) > 2 else "all"
    os.makedirs(out_dir, exist_ok=True)
    # 🔴 **串行** —— 几格共用同一个 CARGO_TARGET_DIR，并行会撞 cargo 的锁。
    if what in ("gates", "all"):
        run_gates(out_dir)
    if what in ("cells", "all"):
        for cell, home, do_mkdir in CELLS:
            run_cell(cell, home, do_mkdir, out_dir)
    print("=== 全部跑完 ===", flush=True)


if __name__ == "__main__":
    main()
