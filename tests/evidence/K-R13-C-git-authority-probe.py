#!/usr/bin/env python3
"""K-R13 实现拍（C）的量具 —— 「丙（问 git）的代价到底是多少」。

住址：<工作树>/evidence/K-R13-C-git-authority-probe.py
被测对象：`/home/zbl/文档/claudecode-frontend/.claude/worktrees/` 下**全部**工作树
          + 主树 `/home/zbl/文档/claudecode-frontend/cc-monitor`。
          （⚠ 本量具**不指向单独一棵树** —— 它扫的是整个盘面；
            要复跑请连这一行一起读，别以为它只量 k-r13。）

为什么要它：PM 在件文件 `§0a 四` 里逐字写「丙要在一条 #[test] 里 shell out 到 git ——
**那是新增的进程依赖，代价没量过**」，并把「先量它」列为实现拍的第一个动作。

跑法（宿主上跑本文件本身；容器那几节它自己 docker run 进沙箱）：
    python3 evidence/K-R13-C-git-authority-probe.py            # 全跑
    python3 evidence/K-R13-C-git-authority-probe.py --host     # 只跑宿主侧（不碰 docker）

分母口径（每一节自己再报一次）：
  · 「工作树」分母 = `.claude/worktrees/` 下**含 `src-tauri/` 的目录**个数，
    `.claude`（那条 shim 住的目录）不算。这个数**每轮都会变**（建树 / prune），
    件文件 §0a 五已经点过名 —— 别把它当常量。
"""

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
WTS = PROJ / ".claude" / "worktrees"
MAIN = PROJ / "cc-monitor"
IMAGE = "ccmon-devbox:latest"

# 丙的算式：`git rev-parse --path-format=absolute --git-common-dir` 再上跳两级。
GIT_ARGS = ["rev-parse", "--path-format=absolute", "--git-common-dir"]
# 独立的第二条权威路（P3 那一格用的就是它）：主工作树的住址，`worktree list` 第一条。
GIT_ARGS_2 = ["worktree", "list", "--porcelain"]


def run(argv, cwd=None, env=None):
    try:
        p = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)
    except FileNotFoundError as e:
        # ★ 这一形正是 Rust 侧 `Command::new("git")` 拿到的东西：
        #   **不是非零退出码，是 io::Error(NotFound)** —— 两条路都得 fail-closed。
        return -1, "", f"<起不来进程> {e}"
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def hr(title):
    print()
    print("=" * 78)
    print(title)
    print("=" * 78)


# ── A · 老算式 vs 丙 vs 独立权威，逐棵树对拍 ────────────────────────────────
def algo_old(manifest_dir: Path):
    """今天盘上的 workspace_cwd()：CARGO_MANIFEST_DIR 往上跳两级。"""
    return manifest_dir.parent.parent


def algo_bing(manifest_dir: Path):
    """丙：问 git 要 --git-common-dir，再上跳两级。推不出来 => None（fail-closed）。"""
    rc, out, err = run(["git", "-C", str(manifest_dir)] + GIT_ARGS)
    if rc != 0 or not out:
        return None, f"rc={rc} {err}"
    common = Path(out)
    if common.parent.parent == Path(common.anchor):
        return None, f"common-dir 层级不够：{common}"
    return common.parent.parent, out


def authority_independent(manifest_dir: Path):
    """第二条权威路（与丙**不同的 git 查询**）：worktree list 的第一条 = 主工作树。"""
    rc, out, err = run(["git", "-C", str(manifest_dir)] + GIT_ARGS_2)
    if rc != 0:
        return None, f"rc={rc} {err}"
    for line in out.splitlines():
        if line.startswith("worktree "):
            return Path(line[len("worktree "):]).parent, line
    return None, "worktree list 里一条 `worktree ` 都没有"


def section_a():
    hr("A · 逐棵树对拍：老算式 / 丙 / 独立权威（宿主）")
    trees = [MAIN]
    for d in sorted(WTS.iterdir()):
        if d.is_dir() and (d / "src-tauri").is_dir():
            trees.append(d)
    print(f"分母：{len(trees)} 棵（1 棵主树 + {len(trees)-1} 棵工作树；"
          f"人群 = 含 src-tauri/ 的目录，`.claude` 不算）")
    stats = {"old_right": 0, "bing_right": 0, "bing_failclosed": 0, "disagree": 0}
    fail_rows = []
    for t in trees:
        md = t / "src-tauri"
        old = algo_old(md)
        bing, bing_why = algo_bing(md)
        auth, auth_why = authority_independent(md)
        truth = auth  # 独立权威是本表的真值
        if truth is not None and old == truth:
            stats["old_right"] += 1
        if bing is None:
            stats["bing_failclosed"] += 1
            fail_rows.append((t.name, bing_why))
        elif truth is not None and bing == truth:
            stats["bing_right"] += 1
        else:
            stats["disagree"] += 1
            fail_rows.append((t.name, f"丙={bing} 权威={truth}"))
        # 也验一遍「老算式算出来的地方盘上有没有 .claude/planned-build」
    print(f"  老算式（parent().parent()）与权威一致 : {stats['old_right']}/{len(trees)}")
    print(f"  丙 与权威一致                        : {stats['bing_right']}/{len(trees)}")
    print(f"  丙 fail-closed（推不出来）           : {stats['bing_failclosed']}/{len(trees)}")
    print(f"  丙 与权威**不一致**（真错）          : {stats['disagree']}/{len(trees)}")
    for name, why in fail_rows:
        print(f"    · {name}: {why}")
    return stats


# ── B · 「盘上有」这条老观测手段在工作树里为什么恒真 ──────────────────────
def section_b():
    hr("B · 老算式算出来的落点：盘上有没有 `.claude/planned-build`（shim 的作用面）")
    rows = []
    for t in sorted(WTS.iterdir()):
        if not (t.is_dir() and (t / "src-tauri").is_dir()):
            continue
        old = algo_old(t / "src-tauri")
        p = old / ".claude" / "planned-build"
        rows.append((t.name, str(old), p.exists(), p.is_symlink()))
    n = len(rows)
    exists = sum(1 for r in rows if r[2])
    print(f"分母：{n} 棵工作树")
    print(f"  老算式落点存在 `.claude/planned-build` 的：{exists}/{n}")
    if rows:
        print(f"  落点（全体唯一值）：{sorted({r[1] for r in rows})}")
        print(f"  它是不是 symlink：{sorted({r[3] for r in rows})}")
    print("  ⇒ 这就是「盘上有」为什么在工作树里恒真：所有工作树的老落点是**同一个**"
          "目录，而 08-26 有人在那里补了一条 shim。")


# ── C · 代价：一次 git rev-parse 要多久（宿主 + 容器） ─────────────────────
def timeit(argv, cwd, n=30):
    # 先热一次（页缓存 / dentry），再量 n 次
    run(argv, cwd=cwd)
    t0 = time.perf_counter()
    for _ in range(n):
        run(argv, cwd=cwd)
    dt = time.perf_counter() - t0
    return dt / n * 1000.0


def section_c_host():
    hr("C1 · 代价（宿主）：一次 `git rev-parse --path-format=absolute --git-common-dir`")
    rc, ver, _ = run(["git", "--version"])
    print(f"  git: {shutil.which('git')}  {ver}")
    md = WTS / "k-r13" / "src-tauri"
    if not md.is_dir():
        md = MAIN / "src-tauri"
    ms = timeit(["git", "-C", str(md)] + GIT_ARGS, cwd=None, n=30)
    print(f"  平均 {ms:.2f} ms/次（n=30，量于 {md}）")
    ms2 = timeit(["git", "-C", str(md)] + GIT_ARGS_2, cwd=None, n=30)
    print(f"  第二条权威路 `worktree list --porcelain`：平均 {ms2:.2f} ms/次（n=30）")
    return ms, ms2


def docker_ok():
    rc, _, _ = run(["docker", "image", "inspect", IMAGE])
    return rc == 0


def section_c_box():
    hr("C2 · 代价（沙箱容器 —— 门禁真正跑的地方）")
    if not docker_ok():
        print("  ❌ 镜像不在，判不了")
        return None
    script = r"""
set -u
echo "which git: $(command -v git || echo '<没有>')"
git --version
cd /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r13/src-tauri 2>/dev/null || cd /home/zbl/文档/claudecode-frontend/cc-monitor/src-tauri
echo "--- 一次的输出 ---"
git rev-parse --path-format=absolute --git-common-dir; echo "rc=$?"
echo "--- 计时 n=30 ---"
git rev-parse --path-format=absolute --git-common-dir >/dev/null
s=$(date +%s%N); for i in $(seq 30); do git rev-parse --path-format=absolute --git-common-dir >/dev/null; done; e=$(date +%s%N)
echo "git-common-dir  平均 $(( (e-s)/30/1000 )) us"
git worktree list --porcelain >/dev/null
s=$(date +%s%N); for i in $(seq 30); do git worktree list --porcelain >/dev/null; done; e=$(date +%s%N)
echo "worktree list   平均 $(( (e-s)/30/1000 )) us"
echo "--- git 不在 PATH 上时（模拟「容器里没 git」）---"
env PATH=/nonexistent-path-for-this-probe git rev-parse --git-common-dir 2>&1; echo "rc=$?"
echo "--- 一棵「worktree 登记没了」的树（夹具：.git 指向不存在的 gitdir）---"
d=$(mktemp -d); printf 'gitdir: /nonexistent-gitdir-for-this-probe\n' > "$d/.git"
git -C "$d" rev-parse --path-format=absolute --git-common-dir 2>&1; echo "rc=$?"
rm -rf "$d"
"""
    rc, out, err = run([
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}", "-e", "HOME=/home/zbl",
        "-w", str(PROJ), IMAGE, "bash", "-c", script,
    ])
    print(out)
    if err:
        print("  [stderr]", err)
    return out


# ── D · 「拿不到 git 时的行为」在宿主上的形状 ─────────────────────────────
def section_d():
    hr("D · 拿不到 git / 登记没了：分别长什么样（宿主）")
    env = dict(os.environ, PATH="/nonexistent-path-for-this-probe")
    rc, out, err = run(["git", "rev-parse", "--git-common-dir"], env=env)
    print(f"  PATH 里没有 git：rc={rc} out={out!r} err={err!r}")
    print("  ⇒ Rust 侧 `Command::new(\"git\")` 在这一形下拿到的是 "
          "`io::Error(NotFound)`，不是非零退出码 —— 两条路都要 fail-closed。")
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / ".git").write_text("gitdir: /nonexistent-gitdir-for-this-probe\n")
        rc, out, err = run(["git", "-C", d] + GIT_ARGS)
        print(f"  worktree 登记没了（夹具）：rc={rc} err={err!r}")


def main():
    host_only = "--host" in sys.argv
    print(f"量于 {time.strftime('%Y-%m-%d %H:%M:%S')}")
    rc, head, _ = run(["git", "-C", str(WTS / "k-r13"), "rev-parse", "HEAD"])
    print(f"量于提交（k-r13 工作树 HEAD）：{head}")
    section_a()
    section_b()
    section_c_host()
    section_d()
    if not host_only:
        section_c_box()


if __name__ == "__main__":
    main()
