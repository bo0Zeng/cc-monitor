#!/usr/bin/env python3
"""K-R2 摸底 · `KR22` 的死值验：判「解析发生在**哪一侧**」，不判「结果回来了没有」。

件文件 `§3` 逐字要求：造一个「把文件拉回本机解析」的实现，**它必须红**。
09-04 实测：**造得出来**，而且这把尺子逮得住它。

先把「造不出反例」这个判断打一次（`§3` 要求的那一打）
---------------------------------------------------
顺手的说法是：「今天根本没有远端那一侧 ⇒ 反例造不出来。」**那是错的。**
这把尺子的**主语是本机侧的指纹**，而本机侧今天**完整存在**（引擎 + 索引落址 + 源文件）。
「远端」在反例里只需要**一个别的目录**来扮演。⇒ 造得出，**只是射程要写清楚**（见末节）。

三个指纹（都钉「在哪一侧发生」）
------------------------------
  ① 本机侧出现被分析仓的**源文件**吗（数文件 + 数字节）
  ② 本机侧出现该仓的**索引库** `.codepicture/` 吗
  ③ 从对侧收回来的字节**量纲**：O(符号数) 还是 O(仓规模)

09-04 实测读数（量于 `e1944e8`）
-------------------------------
  分母：夹具仓 **23 个源文件 / 189,178 字节**（vendor 引擎的 .rs + 前端 panorama 的 .ts）

  | 组                                  | 收回本机 | 本机侧源文件         | 本机侧 .codepicture | 判定 |
  |-------------------------------------|---------|---------------------|--------------------|------|
  | 绿：在代码所在地解析，只收 JSON       | 68 B    | 0 个 / 0 字节        | 0 处               | 🟢   |
  | 红：把源文件拉回本机再解析            | 68 B    | 23 个 / 189,178 字节 | 1 处               | 🔴   |

  🔴🔴 最要紧的一条：两组的 `result.json` 用 `cmp` 比过 —— **字节完全相同**
  （`{"files":23,"symbols":253,"unresolved_calls":1021,"parse_errors":0}`）。
  ⇒ 件文件 `§1 KR22` 那句「外部行为一模一样」**不是担心，是实测**。
    只钉「返回了非空结果」的判据，在这两组上给出的是**同一个答案**。

这把尺子**认不出**什么（射程，别读成全覆盖）
------------------------------------------
· 坏实现把源文件落 tmpfs / 内存、解析完就删 ⇒ 指纹①②都可能消失。
  ⇒ 落地那天①②要配③（量收回字节的**量纲**）。★ 同族先例：中转那条「逐块透传」**量时序不量内容**。
· 它判的是「源文件/索引库**出现在本机侧**」，**不判**「远端那台机器上真起了解析进程」。
· 本趟的「远端」由**同机另一个目录**扮演 ⇒ 买到的是**判据形状 + 阴性对照**，没买到端到端真跨机。

用法
----
  python3 evidence/K-R2-kr22-which-side.py <带引擎的 daemon 二进制> <夹具源目录…>
（那个二进制由 `K-R2-size-and-arch.py` 的变量组编出来，带 `--panorama-probe <仓>`）
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

EXTS = (".rs", ".ts")


def census(d: Path):
    """本机侧的三个指纹里的①②。"""
    files = [p for p in d.rglob("*") if p.is_file() and p.suffix in EXTS]
    idx = [p for p in d.rglob(".codepicture") if p.is_dir()]
    return len(files), sum(p.stat().st_size for p in files), len(idx)


def judge(d: Path, tag: str) -> bool:
    n, b, idx = census(d)
    print(f"[{tag}] 本机侧源文件={n} 个 / {b} 字节 · 本机侧 .codepicture={idx} 处")
    if n or idx:
        print(f"[{tag}] 🔴 红 —— 解析发生在**本机**（源文件或索引库出现在了本机侧）")
        return False
    print(f"[{tag}] 🟢 绿 —— 本机侧既无源文件也无索引库")
    return True


def probe(binary: Path, repo: Path, out: Path) -> None:
    with out.open("wb") as f:
        subprocess.call([str(binary), "--panorama-probe", str(repo)],
                        stdout=f, stderr=subprocess.DEVNULL, cwd=str(repo))


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    binary = Path(sys.argv[1]).resolve()
    srcs = [Path(p).resolve() for p in sys.argv[2:]]

    w = Path(tempfile.mkdtemp())
    remote, local_g, local_r = w / "remote-side" / "fixture-repo", w / "local-green", w / "local-red"
    for d in (remote, local_g, local_r):
        d.mkdir(parents=True)
    for s in srcs:
        for p in s.iterdir():
            if p.is_file() and p.suffix in EXTS:
                shutil.copy2(p, remote / p.name)

    n, b, _ = census(remote)
    print(f"分母：夹具仓 {n} 个源文件 / {b} 字节（住「代码所在地」{remote}）")

    print("\n=== 绿组：在代码所在地解析，只把结果收回本机 ===")
    probe(binary, remote, local_g / "result.json")
    g_bytes = (local_g / "result.json").read_bytes()
    print(f"收回本机 {len(g_bytes)} B：{g_bytes.decode(errors='replace').strip()}")
    green_ok = judge(local_g, "绿组")

    print("\n=== 红组：把源文件拉回本机再解析（外部行为一模一样的那个坏实现） ===")
    for p in remote.iterdir():
        if p.is_file():
            shutil.copy2(p, local_r / p.name)
    probe(binary, local_r, local_r / "result.json")
    r_bytes = (local_r / "result.json").read_bytes()
    print(f"收回本机 {len(r_bytes)} B：{r_bytes.decode(errors='replace').strip()}")
    print("★ 两组结果字节相同吗：" +
          ("**完全相同** ⇒ 只钉「结果回来了」的判据分不出两者" if g_bytes == r_bytes
           else "不同（夹具变了？口径要重讲）"))
    red_ok = judge(local_r, "红组")

    shutil.rmtree(w, ignore_errors=True)
    print()
    if green_ok and not red_ok:
        print("KR22-尺子：✅ 红绿都动 —— 反例造得出来，这把尺子逮得住它")
        return 0
    print("KR22-尺子：❌ 有一侧没动 —— 尺子在空转，别拿它当判据")
    return 1


if __name__ == "__main__":
    sys.exit(main())
