#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R24 下一拍㈠ · **locale** 这条轴的 A/B 量具。

── 尺子（逐字沿用前两拍那把，只换轴）─────────────────────────────────
  「一条判据，它的**判决**随一条**环境事实**翻转，而它**既不建立、也不检查**那条事实。」
  判法：**同一棵树、同一个提交、同一个镜像、同一份挂载、同一个网络口径（`none`）、
        同一个 `HOME` 与那句 `mkdir`，只换 locale 这一维，看哪些判决翻转。**
  🔴 刻意**不切成**「代码里出现了 `LANG` / `LC_ALL` / `LC_CTYPE`」—— 那是词表的形状，
     只认它记得的写法；上面这个是行为的形状，机器判得了。

── `LANG` / `LC_ALL` / `LC_CTYPE` 算一轴还是三轴（本量具的裁定 + 理由）────
  **算一轴，但格里带上「值挂在哪个变量上」这一维。** 三条理由：

  1. **它们不是三件独立的事实，是同一件事实的三个住址（带优先级）。** POSIX 对
     CTYPE 这一类的取值次序是 `LC_ALL` → `LC_CTYPE` → `LANG` 的第一个非空值。
     ⇒ 把它们当三条独立轴去数，等于把**同一条环境事实**数三遍，人群会虚胖三倍。
     ★ 这不是我从标准上抄来的孤证：**本仓自己的散文里就登记着同一条次序**
       （`remote-daemon-proto/src/control/gate.rs` 与 `src-tauri/src/tmux.rs` 的头注
       逐字写着「取 `LC_ALL` → `LC_CTYPE` → `LANG` 的第一个非空值」，
       `K-R12` 的 `evidence/K-R12-locale-lab.md` 是它的实测来源）。
  2. **但它们对「按名字读一个变量」的代码不等价**：一处 `env::var("LANG")` 在
     `LC_ALL=C.UTF-8`（`LANG` 未设）那一格上什么都读不到 —— 而按 POSIX，那一格的
     有效 locale 是 UTF-8 的。⇒ 这一维**不能省**，否则「0」买不到它听起来的那么多。
  3. ⇒ 折中：**人群按「有效 locale」这一条轴算**，而格的设计把三个住址各占一格
     （`L1` 挂 `LC_ALL`、`L3` 挂 `LANG`、`L4` 挂 `LC_CTYPE`），
     **翻转表逐格给**，读者要按三轴读也拿得到分格读数。

── 格 ───────────────────────────────────────────────────────────
  L0  三个都不设                          ← `.claude/devbox/gate` 今天的口径（基线）
                                            实测容器默认：`LANG`/`LC_ALL`/`LC_CTYPE` 全未设 ⇒ 有效 `POSIX`
  L1  LC_ALL=C.UTF-8 · LANG=C.UTF-8       ← 有效 locale 是 UTF-8，值挂在最高优先级那个住址
  L2  LC_ALL=C                            ← 有效 locale 明确是 C（与「未设」不同：设了但是 C）
  L3  LANG=zh_CN.UTF-8                    ← **宿主上用户的真实值**，而镜像里**没装这个 locale**
                                            （`locale -a` 只有 `C` / `C.utf8` / `POSIX`，`K-R12` 实测、本量具复打）
                                            ⇒ 这一格同时是「沙箱 ⇄ 宿主不等价」那一对里够得着的一半
  L4  LC_CTYPE=C.UTF-8                    ← 只挂中间那个住址（`LANG`/`LC_ALL` 都不设）

── 每格跑什么 ───────────────────────────────────────────────────
  ① `cargo` 那格 + ② `daemon` 那格：**逐条判决**（`--no-fail-fast`，否则首个红二进制
     之后的判据这一格根本没被判到，而「没判到」不等于「没翻转」）
  ③ `npm test`：只到**格**这一级（它自己只打得出一个数）
  ④ 四套 ccm e2e：走门禁自己那把尺子 `e2e/assert-pass-floor.sh <套件> <地板> exact`
     ⇒ 每套一个 `PASS=n`，判到**数**这一级
  ⚠ `pb check` 那一格**不并进人群**（它是门禁的一格、不是一条 `#[test]`；上两拍已这么记账）。

🔴 本量具**不改被测代码**、**不给容器放网络**（五格一律 `--network none`）、
   **不动 `.claude/devbox/gate`**。它是另起一个 `docker run`，逐字复刻 gate 的挂载与环境，
   **只把 locale 那一维换掉**。

🔴 **非空对照（没有它，「0」与「尺子没接上」在输出面上一模一样）**：
   `control` 这一趟在容器里跑一段**真随 locale 翻转**的探针（`sort` 的排序、`wc -m` 的
   字符计数、`printf` 的宽度），并把它在五格上的读数逐格打出来。
   ⇒ 只有当探针在五格上**真的给出不同答案**时，「Rust 那两格 0 条翻转」才算一个读数。

⚠ 为什么是 `.py` 而不是 `.sh`：`shell_lint_registry` 那条默认拒绝的判据 —— 仓里任何 `.sh`
  要么进 `ci.yml` 的 shellcheck 表达式、要么登记进它的 `EXEMPT`，而那两处都不在本件写区。
  （先例：`evidence/K-R24-home-axis-ab.py` · `evidence/K-R5-C-r7-door.py`。）

用法：
  python3 evidence/K-R24-D6-locale-axis-ab.py <输出目录> control
  python3 evidence/K-R24-D6-locale-axis-ab.py <输出目录> cells [rust|rest|all]
  python3 evidence/K-R24-D6-locale-axis-ab.py <输出目录> diff
"""
import os
import re
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
WT = PROJ + "/.claude/worktrees/k-r24c"
TARGETS = PROJ + "/.claude/pm-targets/k-r24c"
IMAGE = "ccmon-devbox:latest"
PB_WS = "backend-consolidation"

# 格 → 要往容器里塞的 locale 环境（空 dict = 一个都不设）
CELLS = [
    ("L0", {}),
    ("L1", {"LC_ALL": "C.UTF-8", "LANG": "C.UTF-8"}),
    ("L2", {"LC_ALL": "C"}),
    ("L3", {"LANG": "zh_CN.UTF-8"}),
    ("L4", {"LC_CTYPE": "C.UTF-8"}),
]

# ① ② 两条逐字抄自 `scripts/gate.sh` 的 cargo / daemon 两格，只多 `--no-fail-fast`
RUST_STEPS = [
    ("cargo", "cd src-tauri && cargo test --workspace --exclude code-picture-core "
              "--lib --no-fail-fast"),
    ("daemon", "cd remote-daemon-proto && cargo test --no-fail-fast"),
]
# ③ ④ 逐字抄自 `scripts/gate.sh` 的 npm 格与 `run_e2e` 四行
REST_STEPS = [
    ("npm", "npm test"),
    ("e2e-print-parity", "bash e2e/assert-pass-floor.sh ccm-print-parity 12 exact"),
    ("e2e-rbind-title", "bash e2e/assert-pass-floor.sh ccm-rbind-title 8 exact"),
    ("e2e-cli", "bash e2e/assert-pass-floor.sh ccm-cli 242 exact"),
    ("e2e-contract-parity", "bash e2e/assert-pass-floor.sh ccm-contract-parity 68 exact"),
]

# 非空对照：一段**真随 locale 翻转**的探针。三条都只用容器里现成的东西。
CONTROL = r"""
echo "locale 生效值：$(locale 2>/dev/null | grep -m1 LC_CTYPE)"
echo "locale -a：$(locale -a | tr '\n' ' ')"
printf 'B\na\n' | sort | tr '\n' ' ' | sed 's/^/sort(B,a)= /'; echo
printf '文档' | wc -m | sed 's/^/wc -m(文档)= /'
printf '文档' | wc -c | sed 's/^/wc -c(文档)= /'
python3 -c 'import locale,sys; print("python:", locale.getpreferredencoding(), sys.stdout.encoding)'
echo "env：LANG=[${LANG-<unset>}] LC_ALL=[${LC_ALL-<unset>}] LC_CTYPE=[${LC_CTYPE-<unset>}]"
"""


def docker_argv(env, script):
    """逐字复刻 gate 的 docker run，只多几个 locale 的 `-e`。"""
    argv = ["docker", "run", "--rm", "--network", "none",
            "-v", f"{PROJ}:{PROJ}",
            "-v", f"{SKILL}:{SKILL}:ro",
            "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={TARGETS}",
            "-e", "HOME=/home/zbl",
            "-e", f"PB_WS={PB_WS}"]
    for k, v in env.items():
        argv += ["-e", f"{k}={v}"]
    argv += ["-w", WT, IMAGE, "bash", "-o", "pipefail", "-c",
             'mkdir -p "$HOME/.claude/projects" && ' + script]
    return argv


def run_control(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    for cell, env in CELLS:
        log = os.path.join(out_dir, f"{cell}.control.log")
        with open(log, "wb") as fh:
            rc = subprocess.call(docker_argv(env, CONTROL),
                                 stdout=fh, stderr=subprocess.STDOUT)
        print(f"── 非空对照 · 格 {cell} · rc={rc} ──", flush=True)
        with open(log, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                print("   " + line.rstrip("\n"), flush=True)


def run_cells(out_dir, which):
    os.makedirs(out_dir, exist_ok=True)
    steps = {"rust": RUST_STEPS, "rest": REST_STEPS,
             "all": RUST_STEPS + REST_STEPS}[which]
    # 🔴 **串行** —— 几格共用同一个 CARGO_TARGET_DIR，并行会撞 cargo 的锁；
    #    而且并行会把负载这条轴混进来（那是 `D7` 那把尺子的轴，不是本册的）。
    for cell, env in CELLS:
        print(f"── 格 {cell} · {env or '（三个都不设）'} · --network none ──", flush=True)
        for name, cmd in steps:
            log = os.path.join(out_dir, f"{cell}.{name}.log")
            with open(log, "wb") as fh:
                rc = subprocess.call(docker_argv(env, cmd),
                                     stdout=fh, stderr=subprocess.STDOUT)
            print(f"   {name:<20} rc={rc} ⇒ {log}", flush=True)


def verdicts(path):
    """一份 cargo 全量输出里的**逐条判决**：{判据全名: ok|FAILED|ignored}。"""
    out = {}
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line.startswith("test ") or " ... " not in line:
                continue
            name, _, verdict = line[5:].partition(" ... ")
            verdict = verdict.strip()
            # cargo 并发跑时行可能被别的输出粘在后面 ⇒ 只认这三种干净收尾
            if verdict in ("ok", "FAILED", "ignored"):
                out[name.strip()] = verdict
    return out


PASS_RE = re.compile(r"合计 PASS=(\d+)")
NPM_RE = re.compile(r"Tests\s+(\d+) passed|(\d+) passed")


def cell_rust(out_dir, cell):
    got = {}
    for name, _ in RUST_STEPS:
        got.update(verdicts(os.path.join(out_dir, f"{cell}.{name}.log")))
    return got


def cell_rest(out_dir, cell):
    """③④ 只到「格 / 数」这一级：{步骤: (rc 有没有 0, 抓到的数)}。"""
    out = {}
    for name, _ in REST_STEPS:
        log = os.path.join(out_dir, f"{cell}.{name}.log")
        if not os.path.exists(log):
            continue
        text = open(log, encoding="utf-8", errors="replace").read()
        n = None
        m = PASS_RE.search(text)
        if m:
            n = int(m.group(1))
        else:
            nums = [int(x) for x in re.findall(r"(\d+) passed", text)]
            if nums:
                n = max(nums)
        out[name] = n
    return out


def run_diff(out_dir):
    base = cell_rust(out_dir, "L0")
    print(f"分母（格 L0 逐条采到的 Rust 判决）= {len(base)}")
    for cell, _ in CELLS[1:]:
        cur = cell_rust(out_dir, cell)
        both = set(base) & set(cur)
        flipped = sorted(n for n in both if base[n] != cur[n])
        only_a = sorted(set(base) - set(cur))
        only_c = sorted(set(cur) - set(base))
        print(f"\n── L0 ⇄ {cell} ── 采到 {len(cur)} 条 · 两边都有 {len(both)} 条"
              f" · **翻转 {len(flipped)} 条** · 只在 L0 有 {len(only_a)}"
              f" · 只在 {cell} 有 {len(only_c)}")
        for n in flipped:
            print(f"   翻转  {base[n]:>7} → {cur[n]:<7} {n}")
        for n in only_a[:20]:
            print(f"   缺席  L0有/{cell}无  {n}")
        for n in only_c[:20]:
            print(f"   新增  L0无/{cell}有  {n}")
    print("\n── ③④ 只到「格 / 数」这一级 ──")
    rows = {c: cell_rest(out_dir, c) for c, _ in CELLS}
    names = sorted({k for r in rows.values() for k in r})
    for name in names:
        vals = " · ".join(f"{c}={rows[c].get(name)}" for c, _ in CELLS)
        same = len({rows[c].get(name) for c, _ in CELLS}) == 1
        print(f"   {'逐格同值' if same else '**有格不同**'}  {name:<22} {vals}")


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out_dir, what = sys.argv[1], sys.argv[2]
    if what == "control":
        run_control(out_dir)
    elif what == "cells":
        run_cells(out_dir, sys.argv[3] if len(sys.argv) > 3 else "all")
    elif what == "diff":
        run_diff(out_dir)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
