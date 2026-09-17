#!/usr/bin/env python3
"""K-R26 的两把**沙箱量具** —— 在 devbox 里跑「门禁没挂的那两样」。

住址：`<工作树>/evidence/K-R26-devbox.py`
被测对象**指向哪棵树**：由 `--wt` 给，默认
`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r26`（分支 `track/k-r26`，会核一次）。
⚠ 这一句是刻意的：同名量具被别人覆盖成「指向另一棵树」的那一族病（`brief` `5k`）——
让它**说得出自己在量哪棵树**。

# 为什么要有它（两样门禁都不管）

1. `e2e/daemon-cc-bus.sh` **不在 `scripts/gate.sh` 的九格里**
   （`grep -c daemon-cc-bus scripts/gate.sh` ⇒ 0），而 CI 对它有地板 50
   （`.github/workflows/ci.yml` 逐字 `bash e2e/assert-pass-floor.sh daemon-cc-bus 50`）。
   K-R26 给 `plugin::invoke::run` 加了 `env_clear()`，而那一族插件今天靠**继承**拿配置
   ⇒ 这一套是「有没有把能用的东西关死」的**唯一活体对照**。
2. 死值验每刀只想看「那一格红没红、报文点没点名」，跑整套门禁太贵
   ⇒ `daemon` 模式只跑 daemon 那一包的指定几条。
   ⚠ 它**不是门禁的替身**：交回的九格读数一律来自 `.claude/devbox/gate`。

# 红线

· 宿主上一条测试都不跑〔用 08-29〕⇒ 全程在 `ccmon-devbox:latest` 里，挂载与
  `.claude/devbox/gate` 逐条相同，默认 `--network none`。
· 容器里那套 e2e 自带 tmux shim（`-L <隔离 socket>`）+ 起飞前双向自检，
  而宿主的 `/tmp/tmux-1000` **没有挂进来** ⇒ 够不着用户那台 tmux server。
· 唯一的写是把刚构建出来的 daemon 二进制放进 `remote-daemon-proto/target/debug/`
  （那套 e2e 硬写了这个路径，没有 env 覆盖口），`target/` 在 `.gitignore` 里。跑完删掉。
· 「命令没跑起来」与「跑了 0 条」不许同形：构建失败 / 镜像不在一律 exit 3 并出声。

# 跑法

    python3 evidence/K-R26-devbox.py ccbus  --log <落点>
    python3 evidence/K-R26-devbox.py daemon --log <落点> [--filter <过滤词>]

退出码 = 被跑那件事的退出码（0 = 全绿）；3 = 台子自己没起来。
"""

import argparse
import subprocess
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
TARGETS = PROJ / ".claude" / "pm-targets"
CARGO_CACHE = "ccmon-cargo-registry"
IMAGE = "ccmon-devbox:latest"
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-r26"
WANT_BRANCH = "track/k-r26"

# ── 容器里跑的两段脚本（`bash -c` 的正文）─────────────────────────────────
# ⚠ 刻意留在这里而不是另开一个 `.sh`：全仓每个 shell 脚本都要么被 shellcheck 扫到、
#   要么登记豁免（`shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`），
#   而那张登记表在 `src-tauri/` 下，不在本轮写区里 —— 09-05 现打：新加两个
#   `evidence/*.sh` 当场把它打红，报文逐字点了这两个文件的名。
CCBUS_BODY = r"""
set -o pipefail
mkdir -p "$HOME/.claude/projects"
echo "== 1 构建 daemon 二进制 =="
cargo build --manifest-path remote-daemon-proto/Cargo.toml --bin cc-monitor-remote 2>&1 | tail -5
rc=${PIPESTATUS[0]}
if [ "$rc" -ne 0 ]; then echo "构建失败 rc=$rc —— 不许把它读成 e2e 的读数"; exit 3; fi
BIN="$CARGO_TARGET_DIR/debug/cc-monitor-remote"
[ -x "$BIN" ] || { echo "构建完了却没有 $BIN"; exit 3; }
mkdir -p remote-daemon-proto/target/debug
cp "$BIN" remote-daemon-proto/target/debug/cc-monitor-remote
echo "== 2 跑 e2e/daemon-cc-bus.sh =="
bash e2e/daemon-cc-bus.sh
e2e_rc=$?
rm -rf remote-daemon-proto/target
echo "E2E_EXIT=$e2e_rc"
exit "$e2e_rc"
"""

# ⚠ 包名是 `cc-monitor-remote`，目录名才是 `remote-daemon-proto`，而且它**刻意不在
#   任何 workspace 里**（那份 `Cargo.toml` 的头注逐字）⇒ 只能 `cd` 进去裸跑，不许 `-p`。
DAEMON_BODY = r"""
mkdir -p "$HOME/.claude/projects"
cd remote-daemon-proto || exit 3
if [ -n "$PWF_FILTER" ]; then
  cargo test -- --nocapture "$PWF_FILTER"
else
  cargo test
fi
"""


def docker_argv(wt: Path, tag: str, extra_env):
    argv = [
        "docker", "run", "--rm",
        "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGETS / tag}",
        "-e", "HOME=/home/zbl",
    ]
    for k, v in extra_env:
        argv += ["-e", f"{k}={v}"]
    argv += ["-w", str(wt), IMAGE, "bash", "-o", "pipefail", "-c"]
    return argv


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["ccbus", "daemon"])
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    ap.add_argument("--tag", default="k-r26")
    ap.add_argument("--filter", default="")
    ap.add_argument("--log", required=True)
    a = ap.parse_args()

    wt = Path(a.wt)
    if not (wt / "src-tauri").is_dir():
        print(f"不像工作树（没有 src-tauri/）：{wt}", file=sys.stderr)
        return 3
    branch = subprocess.run(
        ["git", "-C", str(wt), "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True, text=True,
    ).stdout.strip()
    if branch != WANT_BRANCH:
        print(f"这棵树的分支是 `{branch}`，本量具认的是 `{WANT_BRANCH}` —— "
              f"它此刻会去量另一棵树", file=sys.stderr)
        return 3
    if subprocess.run(["docker", "image", "inspect", IMAGE],
                      capture_output=True).returncode != 0:
        print(f"镜像 {IMAGE} 不存在 —— 不许退回宿主跑", file=sys.stderr)
        return 3

    if a.mode == "ccbus":
        if not (wt / "e2e" / "daemon-cc-bus.sh").is_file():
            print("这棵树里没有 e2e/daemon-cc-bus.sh", file=sys.stderr)
            return 3
        argv = docker_argv(wt, a.tag, []) + [CCBUS_BODY]
    else:
        argv = docker_argv(wt, a.tag, [("PWF_FILTER", a.filter)]) + [DAEMON_BODY]

    print(f"被测对象：{wt}（分支 {branch}） · 模式 {a.mode} · 落点 {a.log}")
    proc = subprocess.run(argv, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    Path(a.log).write_text(out, encoding="utf-8")
    print(out)
    print(f"---- 本趟落点：{a.log} · 退出码 {proc.returncode} ----")
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
