#!/usr/bin/env python3
"""K-G6 `C1` 的量具：三道护栏的射程 / 性质行·人群行 / 「今天零使用」那一族的引用数。

用法（**被测对象由 argv 显式给出，别让它默认到别的树上**）：

    python3 evidence/K-G6-C1-reach.py <仓根绝对路径> [<第二个仓根，做前后对照>]

例：
    python3 evidence/K-G6-C1-reach.py /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-g6

它印四组读数，每组都带分母与量法：

  ① 判据名 / 符号名的**全仓**命中（文件数 · 次数）—— `KG63` 的可量目标就在这里。
     ⚠ 分母口径写死在输出里：`git ls-files` 列出的**被跟踪文件**，不是磁盘上的全部文件。
     ⚠ 「文件数」与「次数」是**两个数** —— 摸底那一轮报的 1 是**文件数**，
        而同一个符号在自己那份文件里可以出现十几次。两者别混。
  ② `ratchet_guard::PINS` 那几行的**整行相等 + 次数**（改护栏文件最容易踩的一格）。
  ③ 三道护栏的「性质行 / 人群行」标记各出现几次（`KG62` 钉的就是「各恰好一句」）。
  ④ 反例住址今天还在不在（`KG61` 的两张反例表的幽灵检查，脚本侧复核）。

给两个仓根时，① 会并排印出两边的数并标出差值 —— 那是「改完之后被引用数必须 > 1」这句话
唯一诚实的量法：**同一把尺子，量两棵树**。
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

# ── ① 「今天零使用」那一族 + 对照组 ────────────────────────────────────────────
# 每条：(名字, 它是什么, 它为什么在这张表里)
NAMES = [
    (
        "the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold",
        "判据名",
        "KG63 点名的那一条：摸底时全仓只命中它自己的定义处（文件数 1）",
    ),
    (
        "production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen",
        "判据名",
        "对照组：同族里被指得最多的那条",
    ),
    (
        "the_daemon_can_already_discover_homes_it_just_does_not_send_them",
        "判据名",
        "对照组：同族的反向锚",
    ),
    (
        "fallback_branches_must_not_fabricate_success",
        "判据名",
        "fallback_guard 的正题判据；摸底时同样只命中自己（文件数 1）",
    ),
    ("codex_turn_end_uuid", "符号名", "欠账①：今天没有任何判据钉它"),
    ("is_codex_turn_end", "符号名", "欠账②：同上"),
    ("negotiate", "符号名", "欠账③：plugin/probe.rs 的协商口，零生产调用"),
]

# ── ② ratchet_guard::PINS（现打自 remote-daemon-proto/src/ratchet_guard.rs） ──
PINS = [
    ("remote-daemon-proto/src/readonly_guard.rs", "const SPAWN_SITES_TODAY: usize = 9;", 1),
    ("remote-daemon-proto/src/readonly_guard.rs", "found.len(),", 1),
    ("remote-daemon-proto/src/readonly_guard.rs", "SPAWN_SITES_TODAY,", 1),
    ("remote-daemon-proto/src/readonly_guard.rs", "unregistered.is_empty(),", 1),
    ("remote-daemon-proto/src/no_timer_guard.rs", "REGISTERED_DURATION_USES.len(),", 1),
    (
        "remote-daemon-proto/src/no_timer_guard.rs",
        "const MIN_SCANNED_CODE_BYTES: usize = 80_000;",
        1,
    ),
    ("remote-daemon-proto/src/no_timer_guard.rs", "bytes >= MIN_SCANNED_CODE_BYTES,", 1),
]

# ── ③ KG62 的两行标记 ────────────────────────────────────────────────────────
GUARDS = [
    "remote-daemon-proto/src/readonly_guard.rs",
    "remote-daemon-proto/src/no_timer_guard.rs",
    "remote-daemon-proto/src/platform/fallback_guard.rs",
]
PROP_MARK = "//! - **" + "它守的性质是" + "**"
POPU_MARK = "//! - **" + "它扫的人群是" + "**"
# `readonly_guard.rs` 里从此不许出现的两个承重词（D1 收窄前那句绝对话）。
FORBIDDEN = ["必须" + "只读", "绝不" + "写"]

# ── ④ 反例住址 ───────────────────────────────────────────────────────────────
COUNTEREXAMPLES = [
    ("remote-daemon-proto/src/control/cc_bus.rs", 'run("cc-kill"', "readonly_guard 的反例"),
    (
        "remote-daemon-proto/src/observe/watcher.rs",
        "new_debouncer(",
        "no_timer_guard 的反例（依赖 crate 的带超时线程由这里拉起）",
    ),
    (
        "remote-daemon-proto/src/platform/paths.rs",
        "#[cfg(windows)]",
        "fallback_guard 反例甲：真 Windows 实现，人群内、通过",
    ),
    (
        "remote-daemon-proto/src/plugin/discover.rs",
        "#[cfg(not(unix))]",
        "fallback_guard 反例乙：来历地雷同形，人群外、通过",
    ),
]


def tracked_files(root: Path) -> list[Path]:
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        capture_output=True,
        check=True,
    ).stdout
    return [root / p.decode() for p in out.split(b"\0") if p]


def count_name(files: list[Path], needle: str) -> tuple[int, int, list[str]]:
    """返回 (命中的文件数, 总出现次数, 命中文件相对路径)。两个数**都要报**。"""
    hit_files: list[str] = []
    total = 0
    for f in files:
        try:
            text = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        n = text.count(needle)
        if n:
            total += n
            hit_files.append(str(f))
    return len(hit_files), total, hit_files


def section_one(roots: list[Path]) -> None:
    print("① 判据名 / 符号名的全仓命中（分母 = `git ls-files` 的被跟踪文件）")
    per_root = []
    for root in roots:
        files = tracked_files(root)
        per_root.append((root, files))
        print(f"   · 被测树 {root}  分母 = {len(files)} 份被跟踪文件")
    for name, kind, why in NAMES:
        cells = []
        for root, files in per_root:
            nf, nt, _ = count_name(files, name)
            cells.append(f"文件数={nf} 次数={nt}")
        print(f"   {kind} {name}")
        print(f"      {'   |   '.join(cells)}")
        print(f"      ↳ {why}")


def section_two(root: Path) -> None:
    print("\n② ratchet_guard::PINS —— 整行相等 + 次数（改护栏文件最容易踩的一格）")
    bad = 0
    for rel, line, want in PINS:
        p = root / rel
        if not p.exists():
            print(f"   ⚠ 语料不在：{rel}")
            bad += 1
            continue
        got = sum(1 for l in p.read_text(encoding="utf-8").splitlines() if l.strip() == line)
        flag = "ok " if got == want else "红 "
        if got != want:
            bad += 1
        print(f"   {flag}{rel}  `{line}`  实得 {got} / 应 {want}")
    print(f"   ⇒ 不合的 {bad} 条（分母 = 表里 {len(PINS)} 条）")


def section_three(root: Path) -> None:
    print("\n③ KG62 的两行标记（各应恰好 1 行）+ 收窄前承重词（readonly_guard 应 0）")
    for rel in GUARDS:
        text = (root / rel).read_text(encoding="utf-8")
        lines = text.splitlines()
        prop = sum(1 for l in lines if l.startswith(PROP_MARK))
        popu = sum(1 for l in lines if l.startswith(POPU_MARK))
        print(f"   {rel}: 性质行 {prop} · 人群行 {popu}")
    ro = (root / GUARDS[0]).read_text(encoding="utf-8")
    for w in FORBIDDEN:
        print(f"   readonly_guard 里 `{w}` 命中 {ro.count(w)} 次（应 0）")


def section_four(root: Path) -> None:
    print("\n④ 反例住址今天还在不在（脚本侧复核，判据侧另有幽灵检查）")
    for rel, frag, why in COUNTEREXAMPLES:
        p = root / rel
        ok = p.exists() and frag in p.read_text(encoding="utf-8")
        print(f"   {'在  ' if ok else '不在'} {rel}  `{frag}`   ↳ {why}")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    roots = [Path(a).resolve() for a in sys.argv[1:3]]
    for r in roots:
        if not (r / "remote-daemon-proto").is_dir():
            print(f"❌ 不像本仓的仓根（没有 remote-daemon-proto/）：{r}")
            return 3
    head = subprocess.run(
        ["git", "-C", str(roots[0]), "rev-parse", "HEAD"], capture_output=True, check=True
    ).stdout.decode().strip()
    print(f"# K-G6 C1 射程量具 · 主被测树 {roots[0]} @ {head}")
    section_one(roots)
    section_two(roots[0])
    section_three(roots[0])
    section_four(roots[0])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
