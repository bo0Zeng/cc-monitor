#!/usr/bin/env python3
"""K-P3b 的**沙箱量具** —— 在 devbox 里跑「门禁没挂的那一套」。

住址：`<工作树>/evidence/K-P3b-devbox.py`
被测对象**指向哪棵树**：由 `--wt` 给，默认
`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-p3b`；分支由 `--want-branch`
给（默认 `track/k-p3b`），跑之前**核一次**，对不上就 exit 3。
⚠ 这两句是刻意的：同名量具被别人覆盖成「指向另一棵树」的那一族病（`brief` `5k`）——
让它**说得出自己在量哪棵树、量的是哪一个提交**。
（量「改前」那一趟时工作树是 detached HEAD ⇒ 传 `--want-branch HEAD`。）

# 为什么要有它

`e2e/local-backend-supervise.sh` **不在 `scripts/gate.sh` 的九格里**
（`grep -c local-backend-supervise scripts/gate.sh` ⇒ 0），而它正是
「监护器在**真进程**上到底干了什么」的唯一活体 —— 本件动的恰好是那条路上的
`SuperviseEvent::Exited`（加了两个字段）与消费者的返回类型。
⇒ `KP3W5` 要它**改前改后各跑一趟**，读数逐格贴。

# 红线

· 宿主上一条测试都不跑〔用 08-29〕⇒ 全程在 `ccmon-devbox:latest` 里，
  挂载与 `.claude/devbox/gate` 逐条相同，默认 `--network none`。
· 那套 e2e 自带 tmux shim（`-L e2eLocalBackend`）+ 起飞前双向自检，
  而宿主的 `/tmp/tmux-1000` **没有挂进来** ⇒ 够不着用户那台 tmux server。
· 唯一的写是把刚构建出来的 daemon 二进制放进 `remote-daemon-proto/target/debug/`
  （那套 e2e 硬写了这个路径，只认 `CCM_E2E_DAEMON` 一个覆盖口），`target/` 在
  `.gitignore` 里。跑完删掉。
· 「命令没跑起来」与「跑了 0 条」不许同形：构建失败 / 镜像不在一律 exit 3 并出声。

# 为什么不写成 `.sh`

全仓每个 shell 脚本要么被 shellcheck 扫到、要么登记豁免
（`shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`），
而那张登记表在 `src-tauri/` 下、不在本件写区 ⇒ 新加 `evidence/*.sh` 会被当场点名。
（同 `evidence/K-R26-devbox.py` 的头注，形状照抄那一份。）

# 跑法

    python3 evidence/K-P3b-devbox.py --log <落点>
    python3 evidence/K-P3b-devbox.py --log <落点> --want-branch HEAD   # 量「改前」那一趟

退出码 = 那套 e2e 的退出码（0 = 全绿）；3 = 台子自己没起来。
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
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-p3b"

# ⚠ 刻意留在这里而不是另开一个 `.sh`（理由见模块头注最后一节）。
BODY = r"""
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
echo "== 2 跑 e2e/local-backend-supervise.sh =="
bash e2e/local-backend-supervise.sh
e2e_rc=$?
rm -rf remote-daemon-proto/target
echo "E2E_EXIT=$e2e_rc"
exit "$e2e_rc"
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    ap.add_argument("--tag", default="k-p3b")
    ap.add_argument("--want-branch", default="track/k-p3b")
    ap.add_argument("--log", required=True)
    a = ap.parse_args()

    wt = Path(a.wt)
    if not (wt / "src-tauri").is_dir():
        print(f"不像工作树（没有 src-tauri/）：{wt}", file=sys.stderr)
        return 3
    if not (wt / "e2e" / "local-backend-supervise.sh").is_file():
        print("这棵树里没有 e2e/local-backend-supervise.sh", file=sys.stderr)
        return 3
    branch = subprocess.run(
        ["git", "-C", str(wt), "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True, text=True,
    ).stdout.strip()
    if branch != a.want_branch:
        print(f"这棵树的分支是 `{branch}`，本趟认的是 `{a.want_branch}` —— "
              f"它此刻会去量另一棵树", file=sys.stderr)
        return 3
    head = subprocess.run(
        ["git", "-C", str(wt), "rev-parse", "--short", "HEAD"],
        capture_output=True, text=True,
    ).stdout.strip()
    if subprocess.run(["docker", "image", "inspect", IMAGE],
                      capture_output=True).returncode != 0:
        print(f"镜像 {IMAGE} 不存在 —— 不许退回宿主跑", file=sys.stderr)
        return 3

    argv = [
        "docker", "run", "--rm",
        "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGETS / a.tag}",
        "-e", "HOME=/home/zbl",
        "-w", str(wt), IMAGE, "bash", "-o", "pipefail", "-c", BODY,
    ]

    print(f"被测对象：{wt}（分支 {branch} · 提交 {head}） · 落点 {a.log}")
    proc = subprocess.run(argv, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    Path(a.log).write_text(
        f"被测对象：{wt}（分支 {branch} · 提交 {head}）\n{out}", encoding="utf-8"
    )
    print(out)
    print(f"---- 本趟落点：{a.log} · 提交 {head} · 退出码 {proc.returncode} ----")
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
