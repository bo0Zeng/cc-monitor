#!/usr/bin/env python3
"""门禁的**前置条件**：仓里不许有第二份工作副本。

住址：`tests/evidence/K-W25-worktree-clean.py`（门禁第 25 格 `worktree-clean` 调它）

# 它治的病：2026-09-19 一天内绊了**三次**

本仓有**一族**判据的人群是「**走文件系统**」（`walk` / `read_dir` / `readdirSync` /
`find` / `eslint .`），而不是 `git ls-files`。现打至少 8 份文件里有这种取法。

只要仓内出现第二份工作副本（并发 agent 的 `git worktree`、死值验的变异副本、
备份目录、vendored checkout），它们的人群就**静默膨胀**。当天实测：

    git ls-files 的 `.sh`  =   55
    find 走出来的 `.sh`    = 1193   ← 1138 个在 `.claude/worktrees/` 里

后果不是「多扫了几个文件」，是**一片读不懂的红**：`shell_lint_registry`
（恒等断言）· `bus_identity_registry` · `cc_bus_deploy` · `eslint 基线 7 → 1383`
一起红，而它们和你这一拍改的东西**毫无关系**。三次都花了时间才认出来。

🔴 **更坏的那一半，今天没发作但它在**：上面那几条是**恒等**断言所以红得响。
   同族里凡是用**地板**（`>= N`）的，人群膨胀时**一声不吭地过去** ——
   「多扫了 1138 个文件」在地板下和「扫对了」长得一模一样。

⇒ 本格不修那一族（那是八处以上的改动），只做一件事：**让这个条件自己说话**，
  在那一片红之前先出声，并且逐字说清「这不是你的改动坏了」。

# ⚠ 射程（写死，别读宽）

· 它买的是「**文件系统与 git 对同一批扩展名看到的份数相等**」。
· **买不到**「所有判据的人群都对」—— 一份被 `.gitignore` 掉的源码文件同样会
  让走文件系统的判据多看一份，而本格按 `.gitignore` 的口径算，看不见它。
· **买不到**「worktree 一定是坏事」—— 并发开发时它是对的做法，
  只是**跑门禁那一刻**不该有。本格的诊断因此说的是「先清掉再跑」，不是「不许用」。
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# 抽样的扩展名：挑的是**那几条出事的判据真正在数的东西**，不是随手列的。
#   `.sh`  → `shell_lint_registry` · `bus_identity_registry` · `cc_bus_deploy`
#   `.mjs` → `eslint-baseline` · `node-suite-registry-guard`
#   `.rs`  → `readonly_guard` 的分区恒等（步 10）
#   `.ts`  → `tsc` 那一格的 `want`（`find src tests`）
EXTS = ["sh", "mjs", "rs", "ts"]

# 走文件系统时要跳过的：它们**本来就**不在 git 里，也不该被任何判据数进去。
SKIP = ["node_modules", ".build", "target", ".git"]


def tracked(ext):
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"],
                         capture_output=True, text=True).stdout
    return {f for f in out.split("\0") if f.endswith("." + ext)}


def on_disk(ext):
    found = set()
    stack = [ROOT]
    while stack:
        d = stack.pop()
        try:
            entries = list(d.iterdir())
        except OSError:
            continue
        for p in entries:
            if p.is_symlink():
                continue          # 软链进来的 node_modules 那种，不跟
            if p.is_dir():
                if p.name not in SKIP:
                    stack.append(p)
            elif p.suffix == "." + ext:
                found.add(str(p.relative_to(ROOT)))
    return found


def main():
    fails = []
    checked = 0
    for ext in EXTS:
        g, d = tracked(ext), on_disk(ext)
        checked += 1
        extra = sorted(d - g)
        if extra:
            where = {}
            for f in extra:
                where[f.split("/")[0] + "/"] = where.get(f.split("/")[0] + "/", 0) + 1
            top = " · ".join(f"{k}{v}" for k, v in sorted(where.items(), key=lambda x: -x[1])[:4])
            fails.append(
                f"`.{ext}`：git 认 {len(g)} 份，文件系统走出 {len(d)} 份，"
                f"**多出 {len(extra)} 份**（集中在：{top}）")
    if fails:
        # 🔴 〔订正 09-19〕**两种原因，诊断必须分开说。**
        #   第一版只说「清 worktree」—— 而它第二次红的原因是
        #   **我自己新建的三个文件还没 `git add`**。那时候「清 worktree」是一句误导的话。
        #   ⇒ 先现打分一次：多出来的东西里有没有**未跟踪的新文件**（`git status --porcelain` 的 `??`）。
        untracked = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "--others", "--exclude-standard", "-z"],
            capture_output=True, text=True).stdout
        new_files = [f for f in untracked.split("\0")
                     if f and f.rsplit(".", 1)[-1] in EXTS]
        print("W25: FAIL —— 文件系统与 git 对同一批扩展名看到的份数不等")
        for f in fails:
            print(f"  ✗ {f}")
        print()
        if new_files:
            print(f"🔴 **先看这一条：你有 {len(new_files)} 个新文件还没 `git add`。**")
            for f in new_files[:6]:
                print(f"     {f}")
            print("   那不是污染，是**还没入库**。`git add` 之后本条自己就绿了。")
            print("   ⚠ 顺带说明它为什么该红：走文件系统的那一族判据**已经在数它们了**，")
            print("     而 `git ls-files` 那一族还没有 ⇒ 两族此刻看到的是两个仓。")
            print()
        print("🔴 **若上面那条不适用，这多半不是你这一拍改坏了什么。** 走文件系统取人群的判据")
        print("   （`shell_lint_registry` · `bus_identity_registry` · `cc_bus_deploy` ·")
        print("    `eslint-baseline` · `readonly_guard` 的分区恒等 · `tsc` 的 `want`…）")
        print("   会把那些副本一起数进去，于是一片和你无关的红。")
        print("⇒ 处置：`git worktree list` 看一眼，`git worktree remove` 掉跑完的那几个；")
        print("   死值验的变异副本落 `/tmp`，**不要落在仓里**。")
        print("   ⚠ 一个真坑：刀具按 `<仓根>/../` 算「仓外」，而 **worktree 的上一级仍在主仓里** ——")
        print("     在 worktree 里跑刀具时，那个「仓外」保证是破的。")
        print()
        print("⚠ **同族里用地板（`>= N`）的判据今天不会红** —— 人群膨胀在地板下")
        print("   与「扫对了」长得一模一样。那是本格存在的真正理由。")
        return 1
    print(f"worktree-clean: {checked} passed"
          f"（分母 = 抽样的 {checked} 个扩展名 `{'` `'.join(EXTS)}`，"
          f"每个一条**恒等**断言：`git ls-files` 认的份数 == 文件系统走出的份数。"
          f"⚠ 抽样不是全集 —— 挑的是那几条真出过事的判据在数的东西；"
          f"⚠ 它买不到「所有判据的人群都对」，只买「仓里没有第二份工作副本」）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
