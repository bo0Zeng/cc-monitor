#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R24 下一拍 · 第三条环境轴「**机器负载**」的量具（兼 `launch.rs:1460` ETXTBSY 的复现台）。

── 尺子（沿用前两拍那把，只换轴）─────────────────────────────────────
  「一条判据，它的**判决**随一条**环境事实**翻转，而它**既不建立、也不检查**那条事实。」

  🔴 **但这条轴与前两拍那两条（网络 · `HOME`）形状不同，量法必须跟着改** ——
  前两条是**确定性**的：同一棵树同一个提交，换了轴，那一条**每趟都翻**。
  负载不是：它翻的是**概率**。⇒ 在这条轴上「翻转」只能定义成
  **「在某个负载档位上，失败率非零」**，而一个读数必须带 `失败次数 / 试验次数`。
  ⚠ **单趟读数在这条轴上不是读数** —— 一趟绿证不了「不翻」（本量具存在的理由）。

── 负载怎么给（为什么是限核，不是「同时跑别的活」）───────────────────
  用 `docker run --cpus=<N>`（CFS 配额）压住容器能拿到的 CPU。
  三条理由：
  1. **可复现**：宿主上「同时有几棵树在编译」不是一个量得准的数；`--cpus` 是个刻度。
  2. **不动这台机器**（`K31`）：不需要把宿主真跑满。
  3. **压的正是那两条前提所依赖的东西** —— 都是「某件事在某个时刻之前来得及发生」：
     · relay 那条要「tee 那一路在断言之前至少被写过一次」（调度）
     · launch 那条要「exec 那一刻没有别的线程正卡在 fork 与 exec 之间」（fork 窗口）
     两者都随**可用 CPU 变少**而更容易不成立。

── 格 ───────────────────────────────────────────────────────────
  N0  无限制（16 核，= 门禁今天的口径）        ← 基线
  N1  --cpus=2
  N2  --cpus=1
  ⚠ 其余一切逐字复刻 `.claude/devbox/gate` 的 `docker run`（挂载 · `HOME` · `PB_WS` ·
    `--network none` · 同一个 `CARGO_TARGET_DIR`），**只换 `--cpus` 这一维**。

── 每格跑什么 ───────────────────────────────────────────────────
  一格 = 在**同一个容器**里把某个测试二进制重复跑 R 趟（省掉 R 次容器启动），
  每趟把 `test <名字> ... FAILED` 那几行与 panic 报文原样落进日志。
  ⇒ 输出是**每条判据的失败率**，不是一格的裁决。

🔴 本量具**不改任何被测代码**、**不给容器放网络**（三格一律 `--network none`）、
   **不动 `.claude/devbox/gate`**。它是另起一个 `docker run`。

⚠ 为什么是 `.py` 而不是 `.sh`：`shell_lint_registry` 那条默认拒绝的判据 —— 仓里任何 `.sh`
  要么进 `ci.yml` 的 shellcheck 表达式、要么登记进它的 `EXEMPT`，而那两处都不在本件写区。
  （先例：`evidence/K-R24-home-axis-ab.py` · `evidence/K-R5-C-r7-door.py`。）

用法：
  python3 evidence/K-R24-D7-load-axis-stress.py <输出目录> build
  python3 evidence/K-R24-D7-load-axis-stress.py <输出目录> run <格> <包> <轮数> [过滤串]
  python3 evidence/K-R24-D7-load-axis-stress.py <输出目录> rate <日志文件…>
    包 ∈ {monitor, daemon}      格 ∈ {N0, N1, N2}
"""
import os
import re
import subprocess
import sys
from collections import Counter

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
WT = PROJ + "/.claude/worktrees/k-r24c"
TARGETS = PROJ + "/.claude/pm-targets/k-r24c"
IMAGE = "ccmon-devbox:latest"
PB_WS = "backend-consolidation"

# 格 → 给 docker 的额外参数
CELLS = {
    "N0": [],
    "N1": ["--cpus", "2"],
    "N2": ["--cpus", "1"],
}

# 包 → (在哪个子目录, cargo 参数)
PKGS = {
    # 门禁 cargo 那一格里 `monitor` 那个二进制（`launch::tests` 住这里）
    "monitor": ("src-tauri", "cargo test -p monitor --lib"),
    # 门禁 daemon 那一格（`relay::server::tests` 住这里）
    "daemon": ("remote-daemon-proto", "cargo test"),
    # 门禁 npm 那一格里 `test:dom`（`vitest run`）那个套件 —— PM 09-04 夜第三个活体住这里
    "vitest": (".", "npx vitest run"),
}


def docker_argv(cell, script):
    """逐字复刻 gate 的 docker run，只多 `--cpus`（与去掉 `-w` 里的 gate.sh）。"""
    return (
        ["docker", "run", "--rm", "--network", "none"]
        + CELLS[cell]
        + [
            "-v", f"{PROJ}:{PROJ}",
            "-v", f"{SKILL}:{SKILL}:ro",
            "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={TARGETS}",
            "-e", "HOME=/home/zbl",
            "-e", f"PB_WS={PB_WS}",
            "-w", WT,
            IMAGE,
            "bash", "-o", "pipefail", "-c",
            'mkdir -p "$HOME/.claude/projects" && ' + script,
        ]
    )


def build(out_dir):
    """先把两个二进制编出来 —— 编译占的 CPU 会污染负载读数。"""
    os.makedirs(out_dir, exist_ok=True)
    for pkg, (sub, cmd) in PKGS.items():
        log = os.path.join(out_dir, f"build.{pkg}.log")
        argv = docker_argv("N0", f"cd {sub} && {cmd} --no-run")
        with open(log, "wb") as fh:
            rc = subprocess.call(argv, stdout=fh, stderr=subprocess.STDOUT)
        print(f"build {pkg:<8} rc={rc} ⇒ {log}", flush=True)


def run(out_dir, cell, pkg, rounds, filt=""):
    """一格 × 一个包 × R 趟。⚠ **R 趟在同一个容器里**，容器启动不进读数。"""
    os.makedirs(out_dir, exist_ok=True)
    sub, cmd = PKGS[pkg]
    full = f"{cmd} {filt}".strip()
    # `|| true`：一趟红了后面几趟还要跑（这就是要量的东西）；每趟前后打一行界标。
    script = (
        f"cd {sub} && for i in $(seq 1 {rounds}); do "
        f'echo "===== ROUND $i ====="; {full} 2>&1 || true; '
        f'done'
    )
    # 🔴 **过滤串要进文件名**〔本拍自抓，`brief` 第 12 条「量具住址要能唯一定位到那一份」〕：
    #    第一版的名字只带 `格.包.轮数` ⇒ 同一格同一包**换个过滤串**再跑一趟，
    #    就把上一趟的原始日志**整份覆盖**掉了（本拍真发生过一次：`a_wedged_tee_consumer`
    #    那 20 趟的原始日志被 `one_relay_process_serves_both_keys` 那 20 趟覆盖，
    #    读数只剩在跑它的那次任务输出里）。⇒ 名字里带上过滤串的**归一化形**。
    tag = "".join(c if c.isalnum() else "-" for c in filt).strip("-") or "all"
    log = os.path.join(out_dir, f"{cell}.{pkg}.r{rounds}.{tag[:60]}.log")
    argv = docker_argv(cell, script)
    print(f"── 格 {cell} · {pkg} · {rounds} 趟 · 过滤 {filt!r} ──", flush=True)
    with open(log, "wb") as fh:
        rc = subprocess.call(argv, stdout=fh, stderr=subprocess.STDOUT)
    print(f"   rc={rc} ⇒ {log}", flush=True)
    rate(out_dir, [log])


ROUND_RE = re.compile(r"^===== ROUND (\d+) =====$")
RESULT_RE = re.compile(r"^test result: (\w+)\. (\d+) passed; (\d+) failed")
PREMISE_CAP_RE = re.compile(r"上限 (\d+) 次")
# vitest 的收尾行形如 `Tests  1480 passed (1480)` / `Tests  2 failed | 1478 passed`
VITEST_RE = re.compile(r"Tests\s+(?:(\d+) failed \| )?(\d+) passed")
ANSI_RE = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def rate(out_dir, logs):
    """从一份「R 趟」日志里抠出**每条判据的失败次数 / 试验次数**。

    ⚠ 分母是**开了头的轮数**（`===== ROUND n =====` 的个数），不是 R ——
    容器被打断的话两者不同，而「没跑」与「跑了没红」在输出面上一模一样（本仓语料）。
    """
    for log in logs:
        rounds = 0
        results = Counter()          # 每趟的 test result 汇总
        failed = Counter()           # 判据名 → 失败次数
        panics = Counter()           # panic 报文逐字 → 次数
        # 「前提被破了一次」出声了几次（要 `-- --nocapture` 才看得见），**按上限分开数**。
        #
        # 🔴 为什么按「上限 N 次」分：那一行是**被测代码自己打的**，它把重试上限写在里面 ⇒
        #    上限 **50** = `launch.rs` 真判据那一处（`FAKE_TERM_ETXTBSY_TRIES`）⇒ **自然发生的竞态**；
        #    上限 **2**  = 那条「主动把前提破掉」的判据（腿②）自己造的 ⇒ **每趟恒定 2 次，不是读数**。
        #    ⚠ 不分开数就会把 34 次自造的加进人群里 —— 那是一次假读数（本量具第一版就这么错过一次）。
        premise = Counter()
        with open(log, "r", encoding="utf-8", errors="replace") as fh:
            for line in fh:
                # ⚠ vitest 的收尾行**带 ANSI 转义**（`Tests  \x1b[1m\x1b[32m1 passed`）——
                #   本量具第一版没剥它，于是 vitest 那一格「一条判决都抓不到」被打成了
                #   「失败判据一条都没有」。**那两句在输出面上长得一样**，正是本仓语料那族病。
                line = ANSI_RE.sub("", line.rstrip("\n"))
                if ROUND_RE.match(line):
                    rounds += 1
                    continue
                if "[K-R24] 前提被破了一次" in line:
                    cap = PREMISE_CAP_RE.search(line)
                    premise[cap.group(1) if cap else "?"] += 1
                if line.startswith("test ") and line.endswith(" ... FAILED"):
                    failed[line[5:-11].strip()] += 1
                m = RESULT_RE.match(line)
                if m:
                    results[(m.group(1), int(m.group(2)), int(m.group(3)))] += 1
                if "panicked at " in line:
                    panics[line.split("panicked at ", 1)[1]] += 1
                # vitest 那一格：它不打 `test result:` 那一行，判决面是这两种
                mv = VITEST_RE.search(line)
                if mv:
                    # ⚠ 组序：group(1) 是 failed（可缺）、group(2) 是 passed —— 别读反了。
                    results[("vitest", int(mv.group(2)), int(mv.group(1) or 0))] += 1
                if "Unhandled Error" in line or "Unhandled Errors" in line:
                    failed["vitest: Unhandled Error"] += 1
        print(f"\n=== {os.path.basename(log)} ===")
        print(f"开了头的轮数（分母）= {rounds}")
        for k, v in sorted(results.items(), key=lambda kv: -kv[1]):
            print(f"  test result {k[0]:<7} passed={k[1]:<5} failed={k[2]:<3} × {v} 趟")
        nat = premise.get("50", 0)
        made = premise.get("2", 0)
        print(f"  ETXTBSY 前提被破（出声计数，需 `-- --nocapture`）："
              f"**自然发生 {nat} 次 / {rounds} 趟**（上限 50 那一处）"
              f" · 判据自造 {made} 次（上限 2，腿② 恒定，不进人群）"
              f"{' · 上限认不出 ' + str(premise['?']) + ' 次' if premise.get('?') else ''}")
        if not failed:
            print("  失败判据：**一条都没有**")
        for name, n in failed.most_common():
            print(f"  失败 {n}/{rounds}  {name}")
        for msg, n in panics.most_common():
            print(f"  panic × {n}: {msg}")


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out_dir, what = sys.argv[1], sys.argv[2]
    if what == "build":
        build(out_dir)
    elif what == "run":
        cell, pkg, rounds = sys.argv[3], sys.argv[4], int(sys.argv[5])
        run(out_dir, cell, pkg, rounds, sys.argv[6] if len(sys.argv) > 6 else "")
    elif what == "rate":
        rate(out_dir, sys.argv[3:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
