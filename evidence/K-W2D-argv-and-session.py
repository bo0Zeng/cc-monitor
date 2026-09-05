#!/usr/bin/env python3
"""K-W2D `KW2D7` · 一次性 exec 撑不撑得住全景那一族查询 —— **要量的两个数**。

件文件 `§1 KW2D7` 点名要两个今天没有的数：
  ① 21 条命令里每条的**最大入参尺寸**，与 argv 上限对一遍；
  ② 一次全景会话真实发出的**查询条数与频率**（决定「每次一个新进程」到底贵多少）。

本量具给出的是这三组读数（各自的口径与射程写在每一节的输出里）：
  A · **argv 上限**（容器内实测，bisect）：单参数上限 · 整条 argv 的总上限。
      🔴 这两个是**两个不同的墙**，`plugin/invoke.rs` 的头注只记了前一个
      （「单参数上限 128 KiB / 200 KB 必炸、120 KB 能过」）。**入参怎么编码，决定撞哪一面墙。**
  B · **入参尺寸普查**（宿主上纯文本普查，不跑任何测试）：从 monitor 的命令签名抽出每条命令的
      入参形状，并把唯一一条**尺寸无上界**的（一组文件 + 行区间）按本仓真实文件表算出字节数，
      两种编码各算一份：**一个 JSON 参数** vs **一文件一参数**。
  C · **一次性 exec 的真实开销**（容器内实测）：拿 `KW2D2` 那趟编出来的 sidecar 探针，
      在一个已建好索引的夹具仓上重复问同一条查询，量**每次起一个新进程**的墙钟。

⚠ ②的后半（「真实会话的频率」）本量具**判不了**：那要跑真 app（本轮红线：不起真 daemon /
   不起真 app）。它给的是**静态扇出**——每个用户手势触发几条命令（从前端调用点数出来），
   与**每次 exec 的墙钟**。两者相乘是上界，不是「实测的会话曲线」。

用法
----
  python3 evidence/K-W2D-argv-and-session.py <工作树绝对路径> [<KW2D2 的 measure 目录>]
（给了第二个参数才跑 A 与 C —— 它们要容器 + 那趟编出来的二进制。）
"""

import json
import subprocess
import sys
from pathlib import Path

IMG = "ccmon-devbox:latest"

# code-picture 认的源码扩展名（vendor `lang.rs` 那 9 门 + 头文件）。
# ⚠ 这张表只用来**算入参字节数**，不用来判「引擎会不会解析它」——后者住 vendor，`C7` 不动。
CODE_EXTS = (".rs", ".py", ".js", ".mjs", ".cjs", ".ts", ".tsx", ".java", ".kt",
             ".c", ".h", ".cc", ".cpp", ".hpp", ".cs")

SKIP_DIRS = {"node_modules", "target", ".git", "dist", "coverage"}

# A · argv 上限探针：两面墙分别 bisect。
#   单参数：一个超长参数；总量：许多中等长度的参数。
#   ⚠ 判据是 `E2BIG`（`OSError.errno == 7`），不是「命令失败」——两者在终端上一模一样。
ARGV_PROBE = r"""
import os, errno
def ok(argv):
    try:
        pid = os.fork()
        if pid == 0:
            try:
                os.execv("/bin/true", ["true"] + argv)
            except OSError:
                os._exit(90)
            os._exit(91)
        _, st = os.waitpid(pid, 0)
        return os.WEXITSTATUS(st) == 0
    except OSError:
        return False
def bisect(mk, lo, hi):
    while lo + 1 < hi:
        mid = (lo + hi) // 2
        if ok(mk(mid)): lo = mid
        else: hi = mid
    return lo
one = bisect(lambda n: ["x" * n], 1, 4 << 20)
tot = bisect(lambda n: ["y" * 4096] * (n // 4096), 4096, 32 << 20)
print(f"ARGV 单参数上限={one} 字节")
print(f"ARGV 总量上限≈{tot} 字节（用 4 KiB 一个的参数堆上去；含 argv+环境的那本账）")
"""


def census_files(root: Path) -> list[str]:
    """本仓里 code-picture 会当源码看的那些文件（相对路径）。纯文本普查。"""
    out: list[str] = []
    stack = [root]
    while stack:
        d = stack.pop()
        for p in d.iterdir():
            if p.is_symlink():
                continue
            if p.is_dir():
                if p.name not in SKIP_DIRS:
                    stack.append(p)
                continue
            if p.suffix in CODE_EXTS:
                out.append(str(p.relative_to(root)))
    out.sort()
    return out


def section_b(wt: Path) -> None:
    print("=== B · 入参尺寸普查（宿主纯文本普查，量于给定工作树） ===")
    pan = (wt / "src-tauri" / "src" / "panorama.rs").read_text()
    attr = "#[tauri::" + "command]"
    # 生产段口径：剥掉 `//` 整行注释（裸 grep 会把注释里那一处也数进去 —— 22 vs 21）
    prod = "\n".join(l for l in pan.splitlines() if not l.lstrip().startswith("//"))
    n_cmd = prod.count(attr)
    print(f"命令条数（生产段）={n_cmd}  ⚠ 裸 grep 数到 {pan.count(attr)}"
          f"（多的那一处写在注释里）")
    files = census_files(wt)
    total = sum(len(f) for f in files)
    # 一个 JSON 参数：`{"files":[…],"ranges":[]}`
    as_json = len(json.dumps({"files": files, "ranges": []}, ensure_ascii=False))
    # 一文件一参数：每个文件一个 argv 条目（+1 是 NUL）
    as_argv_total = total + len(files)
    longest = max(len(f) for f in files)
    print(f"全仓源码文件数={len(files)}  路径总字节={total}  最长单条路径={longest}")
    print(f"最坏那条命令（一组文件 + 行区间）的入参编码：")
    print(f"  · 一个 JSON 参数      = {as_json} 字节  ← 撞的是**单参数**那面墙")
    print(f"  · 一文件一参数（合计） = {as_argv_total} 字节 / {len(files)} 个参数"
          f"  ← 撞的是**总量**那面墙，单参数最大只有 {longest} 字节")
    print("其余 20 条命令的入参形状（读签名得来）：仓路径 · 符号全限定 id · 文件路径 · "
          "搜索串 · 整数（depth/budget/limit）· 批注正文（人写）"
          " ⇒ 除批注正文外都是**单条短串**，与两面墙都差着几个数量级。")


def section_a_c(measure: Path) -> None:
    if not measure.exists():
        print(f"⚠ 跳过 A 与 C：{measure} 不存在（它们要容器 + KW2D2 那趟的产物）")
        return
    bin_rel = "target-side-full/x86_64-unknown-linux-musl/release/cp-sidecar-probe"
    if not (measure / bin_rel).exists():
        print(f"⚠ 跳过 C：{bin_rel} 不在 —— 先跑 K-W2D-sidecar-size.py")
    script = f"""
set -uo pipefail
echo "=== A · argv 上限（容器内实测） ==="
python3 - <<'PY'
{ARGV_PROBE}
PY
echo
echo "=== C · 一次性 exec 的开销（容器内实测） ==="
B=/m/{bin_rel}
[ -x "$B" ] || {{ echo "没有探针二进制，跳过"; exit 0; }}
F=/tmp/fixture-repo
rm -rf "$F"; mkdir -p "$F"
cp /m/repo-base/src-tauri/vendor/code-picture-core/src/*.rs "$F/"
echo "夹具仓：$(ls "$F" | wc -l) 个 .rs / $(du -sb "$F" | cut -f1) 字节"
S=$(date +%s%N); "$B" index "$F" > /tmp/idx.json; E=$(date +%s%N)
echo "index（真解析全仓）一次 = $(( (E-S)/1000000 )) ms  ⇒ $(cat /tmp/idx.json)"
for CMD in status search; do
  S=$(date +%s%N)
  for i in $(seq 1 21); do "$B" $CMD "$F" engine > /dev/null; done
  E=$(date +%s%N)
  echo "21 次「$CMD」= 每次一个新进程：合计 $(( (E-S)/1000000 )) ms · 均 $(( (E-S)/21000000 )) ms/次"
done
# 重活那一条的量纲：拿**整个仓副本**当被分析仓（619 个源码文件那一级）。
# ⚠ 索引落被分析仓（`store_dir: None` = 旧行为）⇒ 只在 scratch 的副本里落，不碰工作树。
R=/tmp/real-repo
rm -rf "$R"; cp -a /m/repo-base "$R"; rm -rf "$R/node_modules"
S=$(date +%s%N); "$B" index "$R" > /tmp/idx2.json; E=$(date +%s%N)
echo "index 整个仓副本一次 = $(( (E-S)/1000000 )) ms  ⇒ $(cat /tmp/idx2.json)"
S=$(date +%s%N); "$B" status "$R" > /dev/null; E=$(date +%s%N)
echo "同一仓上一次「status」（索引已在）= $(( (E-S)/1000000 )) ms"
"""
    subprocess.call(["docker", "run", "--rm", "--network", "none", "--user", "root",
                     "-v", f"{measure}:/m", "-e", "HOME=/root", IMG,
                     "bash", "-o", "pipefail", "-c", script])


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    wt = Path(sys.argv[1]).resolve()
    section_b(wt)
    print()
    if len(sys.argv) > 2:
        section_a_c(Path(sys.argv[2]).resolve())
    else:
        print("⚠ 没给 measure 目录 ⇒ A（argv 上限）与 C（exec 开销）没跑，"
              "**不许把没跑读成没问题**。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
