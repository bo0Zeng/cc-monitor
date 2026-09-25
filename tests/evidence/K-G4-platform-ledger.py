#!/usr/bin/env python3
"""`G4` 的判据本体 —— **承诺的平台 ↔ 门禁真跑的格**，两向对拍。

住址：`tests/evidence/K-G4-platform-ledger.py`（门禁第 22 格 `platform` 调它）

# 它买什么

`设计/01 §7.3` 逐字：「独立进程那个壳要为**每一个我们发布的平台**编得过，且这条要在门禁里。」
（「折进前端那个壳要能链接进前端」那一半随那一档放弃（`99 §1` V105）作废 —— 见下面 `P4` 的墓志。）
行数由 **条 63** 定：承诺的是**三格**（本机 Windows x86_64 · 远端 Linux · 本机 Linux x86_64），
(Windows, aarch64) **显式拒绝**。〔V132 · 09-25〕本机 (Linux, aarch64) **不承诺**（用户原话
「不承诺. 适配部分, 即os适配部分后面单独写单独做.」）—— 它不是零脚印（远端那格有 musl 字节），
所以不进 `REFUSED`，进 `NOT_PROMISED`：代码里 `byte_table::promised` 对它答「否」，由 monitor 侧
`byte_table_tests.rs::the_promise_face_in_the_ledger_equals_the_code` 把下面 `PROMISE_FACE` / `NOT_PROMISED`
与代码两向钉住（本文件是承诺面的唯一住址，那条判据读的就是这里）。

⇒ 本文件判三条：

  · `P1` **承诺表 ↔ 门禁格**两向集合相等 —— 承诺了却没门禁的格、有门禁却没登记的格，都红
  · `P2` 每一格登记的**逐字锚点**在 `gate.sh` 里 `count() == 1`（登记指得到真东西）
  · `P3` **显式拒绝的那格真的零脚印** —— `aarch64-pc-windows` 全仓命中必须是 0
  · ~~`P4`~~ 「壳-折」那一维有被检查的对象 —— 〔S5 · 第四波 · V105 清账〕**删了**：
    「折进前端进程」那一档已放弃，那一维没有对象，判它「有对象」等于替一个不建的东西守门。
    `[lib]` 本身还在 —— 它是 `4a`（库化）的产物，有自己的住址（`readonly_guard` 的
    `BACKEND_CORE_MODULES`），不靠这里活着。

# 🔴 反空真：绿从哪来

`P1` 是**集合相等**，不是「承诺表里每条都能找到」。后者在承诺表被清空时恒真。
死值验：摘掉任一格的登记 ⇒ `P1` 红；把锚点写错 ⇒ `P2` 红；
往仓里塞一个 `aarch64-pc-windows` 字样 ⇒ `P3` 红。

# ⚠ 诚实边界（写死，别读宽）

`cargo check` / `cargo zigbuild` 买的是「**编得过 / 编得出字节**」。
**买不到**「在那个平台上真的跑起来」，也买不到「MSVC ABI 上链接得起来」
（`check` 不链接；两格 Windows 用的是 `-gnu`，`ci.yml:267` 自陈不证 MSVC 那一格）。
真机行为那一维今天仍然是**判不了**，不是「通过」。
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GATE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "tests" / "scripts" / "gate.sh"

# ── 承诺表（条 63，用户 2026-09-18 逐字点完）──────────────────────────────
# 每格：(平台名, 它在 gate.sh 里那一格的名字, 逐字锚点, 这一格买到了什么)
PROMISED = [
    ("本机 Windows x86_64", "winchk", "run_gate winchk ",
     "`-p monitor` 的生产段 ＋ test 档过 `x86_64-pc-windows-gnu`"),
    ("本机 Windows x86_64", "winchk-backend", "run_gate winchk-backend ",
     "`src/backend` 那个 crate 过 `x86_64-pc-windows-gnu`（`--all-targets`）"),
    ("远端 Linux（musl 两个 arch）", "muslbuild", "run_gate muslbuild ",
     "后端在两个 musl target 上**编得出静态字节**（与 `release.yml` 同一套 zig 版本）"),
    ("本机 Linux x86_64", "cargo", "run_gate_sum cargo ",
     "host triple 上整个 workspace 编得过并跑得过测试"),
    ("本机 Linux x86_64", "backend", "run_gate backend ",
     "后端那个 crate 在 host triple 上编得过并跑得过测试"),
]

# ── 承诺面（`byte_table::promised` 的真相源；键 = (origin, OS, arch)，只列表 A 里有产线的格）──────
# 🔴 与代码两向相等（monitor `byte_table_tests.rs::the_promise_face_in_the_ledger_equals_the_code` 读这两张表）：
#    承诺了而代码不放行、代码放行而这里没写，都红。
PROMISE_FACE = [
    ("Local", "Windows", "x86_64"),
    ("Local", "Linux", "x86_64"),
    ("Remote", "Linux", "x86_64"),
    ("Remote", "Linux", "aarch64"),
]

# ── 不承诺（有产线、却不承诺；与 `REFUSED` 不同：它不是零脚印）────────────────────────
# 〔V132 · 09-25〕用户原话「不承诺. 适配部分, 即os适配部分后面单独写单独做.」
NOT_PROMISED = [
    ("Local", "Linux", "aarch64",
     "V132：本机 (Linux, aarch64) 不承诺；OS 适配以后单独写设计、单独做。起不来时出声「这台不在承诺里」"),
    ("Remote", "Windows", "x86_64",
     "条 63：远端 Windows 目标里有、现在不做（有本机那份原生字节，远端没验过）"),
]

# ── 显式拒绝（条 63 待点②：不含 arm64，零成本落地因为盘上零脚印）────────
REFUSED = [("(Windows, aarch64)", "aarch64-pc-windows")]


def tracked_files():
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"],
                         capture_output=True, text=True).stdout
    return [f for f in out.split("\0") if f]


def main():
    text = GATE.read_text(encoding="utf-8")
    fails = []

    # ── P1：两向集合相等 ────────────────────────────────────────────────
    # 盘上现打：`gate.sh` 里那些**跨 target / 跨壳**的格。认法与 `K-R80` 同源：
    # 只认行首调用（缩进着的是 `gate_selftest` 的探针，不是判定格）。
    on_disk = set()
    for m in re.finditer(r"^run_gate(?:_sum)? (\S+) ", text, re.M):
        name = m.group(1)
        if name in {n for _, n, _, _ in PROMISED}:
            on_disk.add(name)
    registered = {n for _, n, _, _ in PROMISED}
    if on_disk != registered:
        fails.append(
            f"P1 承诺表与 `gate.sh` 现打的格对不上："
            f"登记了而盘上没有 {sorted(registered - on_disk)} · "
            f"盘上有而登记没有 {sorted(on_disk - registered)} —— "
            f"⚠ 前者是**承诺的平台今天没有门禁**（条 63 点名的那三格之一失守）；"
            f"后者是登记陈了。两向都判，因为只判一向在表被清空时恒真。")

    # ── P2：逐字锚点唯一 ────────────────────────────────────────────────
    for plat, cell, anchor, _ in PROMISED:
        n = text.count(anchor)
        if n != 1:
            fails.append(f"P2 `{cell}`（{plat}）的逐字锚点在 `gate.sh` 里命中 {n} 次"
                         f"（应当恰好 1 次）：{anchor!r}")

    # ── P3：显式拒绝的那格真的零脚印 ────────────────────────────────────
    for label, needle in REFUSED:
        hits = []
        for f in tracked_files():
            p = ROOT / f
            try:
                if needle in p.read_text(encoding="utf-8", errors="replace"):
                    hits.append(f)
            except OSError:
                continue
        # 本文件自己写着那个字面量（它是判据），摘掉自己 —— 不靠 `file!()`，逐字写出来。
        hits = [h for h in hits if h != "tests/evidence/K-G4-platform-ledger.py"]
        if hits:
            fails.append(
                f"P3 {label} 是**显式拒绝**的平台（条 63 待点②），而盘上现打 {len(hits)} 处脚印："
                f"{hits[:5]} —— 要么那一票被推翻了（回来改这张表 ＋ 给它一条产线与一格门禁），"
                f"要么有人顺手加了个跑不起来的 target。**两种都要人回来裁。**")

    checks = 1 + len(PROMISED) + len(REFUSED)
    if fails:
        print(f"KG4D1: FAIL={len(fails)}")
        for f in fails:
            print(f"  ✗ {f}")
        return 1

    print(f"# `G4` 平台账本 —— 量于 `{GATE}`")
    print()
    print("| 平台 | 门禁哪一格 | 这一格买到了什么 |")
    print("|---|---|---|")
    for plat, cell, _, buys in PROMISED:
        print(f"| {plat} | `{cell}` | {buys} |")
    print()
    for label, needle in REFUSED:
        print(f"🚫 **{label}**：显式拒绝（条 63 待点②）· 全仓 `{needle}` 现打 **0** 处脚印")
    for route, os_, arch, why in NOT_PROMISED:
        print(f"⬜ **{route} ({os_}, {arch})**：不承诺 —— {why}")
    print()
    print("⚠ **买不到的，逐条写死**：`check`/`zigbuild` 买「编得过 / 编得出字节」，"
          "**不买**「在那个平台上真跑得起来」，也**不买** MSVC ABI 的链接"
          "（两格 Windows 走 `-gnu`；`ci.yml` 自陈不证那一格）。真机行为仍是**判不了**。")
    print()
    print(f"platform: {checks} passed（分母 = P1 两向集合相等 1 ＋ "
          f"P2 逐字锚点 {len(PROMISED)} 条 ＋ P3 显式拒绝 {len(REFUSED)} 格零脚印）")
    print("KG4D1: OK —— P1..P3 全过（⚠ 它判的是「门禁盖到了哪些平台」，"
          "**不判那些平台上真的跑得起来**）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
