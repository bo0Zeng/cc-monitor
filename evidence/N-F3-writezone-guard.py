#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""N-F3 量具：**写区外零改动**的三个口径（`NF3D6`）。

住址：`evidence/N-F3-writezone-guard.py`（本件独占的名字）。
被测对象与基点**由 argv 给出**，脚本里没有硬编码任何一棵树：

    python3 evidence/N-F3-writezone-guard.py <工作树绝对路径> <基点 sha>

三个口径，各印一次：
  ① `git status --porcelain`      —— 有没有未提交/未跟踪的东西
  ② **逐文件 md5**（不许用 `--stat`）—— 工作区那一份 vs 基点那一份，逐个比
  ③ `git diff <基点> -- <写区外路径>` —— 差集为空要带**退出码**，不是「没输出」

🔴 口径②必须用 `git ls-files -z`：**裸 `git ls-files` 会把非 ASCII 路径加引号转义**，
   本仓路径全是中文，`N-G2` 那把尺子初版因此读出 5 份假 `DIFF`（`INBOX` `NG2j`）。
   本脚本走 `-z` + `split(b"\\0")`，路径原样是字节，不经过任何 shell 引用。

⚠ 「`--stat` 是瞎的」**有射程**：只对「改动落在新增块里」那一形成立，改既有行时它看得见。
   所以这里不拿 `--stat` 当口径②，而是真的逐文件哈希。

# 写区（与 `.dispatch.json` 那条登记 + PM 补充里那条新建文件协议逐字对齐）

前缀/整名任一命中即算写区内；**其余全部**是写区外，进分母。
"""

import hashlib
import subprocess
import sys

# 写区：`.dispatch.json` 的四项代码仓路径 + PM 补充给的「src/ 下、名字带 first-run」的口子。
WRITE_EXACT = {
    "src/main.ts",
    "src/styles.css",
    "src/settings/accounts-section.ts",
}
WRITE_PREFIX = ("evidence/",)


def in_write_zone(path: str) -> bool:
    if path in WRITE_EXACT:
        return True
    if path.startswith(WRITE_PREFIX):
        return True
    # 新建文件协议：只许在 `src/` 下、文件名带 `first-run`。
    if path.startswith("src/") and "first-run" in path.rsplit("/", 1)[-1]:
        return True
    return False


def md5(data: bytes) -> str:
    return hashlib.md5(data).hexdigest()


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    wt, base = sys.argv[1], sys.argv[2]

    def git(*a: str, text: bool = True):
        return subprocess.run(
            ["git", "-C", wt, *a], capture_output=True, text=text, check=False
        )

    print(f"· 被测对象 {wt}  · 基点 {base}")

    print("\n=== 口径① git status --porcelain ===")
    r = git("status", "--porcelain")
    print(r.stdout if r.stdout.strip() else "(空)")
    print(f"退出码 {r.returncode}")

    print("\n=== 口径② 逐文件 md5（git ls-files -z，写区外全体）===")
    raw = subprocess.run(
        ["git", "-C", wt, "ls-files", "-z"], capture_output=True, check=False
    ).stdout
    paths = [p.decode("utf-8") for p in raw.split(b"\0") if p]
    outside = [p for p in paths if not in_write_zone(p)]
    inside = [p for p in paths if in_write_zone(p)]
    diffs, missing = [], []
    for p in outside:
        cur = subprocess.run(
            ["git", "-C", wt, "show", f":{p}"], capture_output=True, check=False
        )
        # `:path` = 暂存区那一份；工作区那一份直接读盘（两者都要与基点相同）。
        try:
            with open(f"{wt}/{p}", "rb") as fh:
                disk = fh.read()
        except OSError:
            missing.append(p)
            continue
        old = subprocess.run(
            ["git", "-C", wt, "show", f"{base}:{p}"], capture_output=True, check=False
        )
        if old.returncode != 0:
            missing.append(p)  # 基点上没有这份 ⇒ 它本身就是一处新增，单列
            continue
        if md5(disk) != md5(old.stdout) or md5(cur.stdout) != md5(old.stdout):
            diffs.append(p)
    print(f"跟踪文件总数 {len(paths)} = 写区内 {len(inside)} + 写区外 {len(outside)}")
    print(f"写区外 md5 与基点不同的：{len(diffs)}")
    for p in diffs:
        print(f"  DIFF {p}")
    print(f"写区外「基点上没有 / 盘上读不到」的：{len(missing)}")
    for p in missing:
        print(f"  ONLY-ONE-SIDE {p}")

    print("\n=== 口径③ git diff <基点> -- <写区外路径> ===")
    # 用 pathspec 排除法：全体 - 写区，等价于「只看写区外」。
    excludes = [f":(exclude){p}" for p in sorted(WRITE_EXACT)]
    excludes += [f":(exclude){p}**" for p in WRITE_PREFIX]
    excludes += [":(exclude)src/*first-run*", ":(exclude)src/**/*first-run*"]
    r3 = git("diff", base, "--", ".", *excludes)
    print(r3.stdout if r3.stdout.strip() else "(空)")
    print(f"退出码 {r3.returncode}")
    # 「差集为空」要有一个**非空对照**，否则「命令没跑」与「跑了是空」在终端上同形。
    r3n = git("diff", base, "--", "src/main.ts")
    print(
        "非空对照（同一条命令指向写区内的 src/main.ts）："
        f"{len(r3n.stdout.splitlines())} 行，退出码 {r3n.returncode}"
    )

    bad = bool(r.stdout.strip()) or diffs or missing or r3.stdout.strip()
    print(f"\n判定：{'红 —— 写区外有改动' if bad else '绿 —— 三个口径一致说写区外零改动'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
