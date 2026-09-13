#!/usr/bin/env python3
"""`K-R101` 死值验刀具 —— 一趟一刀，切完还原，**取不到该取的东西就拒跑**。

# 三条纪律，都是前两件真踩出来的（`K-R86`/`K-R87`）

1. 🔴 **还原不许 `cp -a`** —— 连 mtime 一起还原 ⇒ cargo 判「没变」⇒ 沿用上一刀的产物
   ⇒ 你读到的是**上一刀的红**。这里用 `shutil.copyfile` ＋ `os.utime(None)`（新 mtime）。
2. 🔴 **fail-closed** —— `K-R87` 那轮 `cut.sh` 只认第 2 行的 `# FILES:`，某个变异写在第 4 行
   ⇒ **还原静默跳过**、下一刀叠在上一刀上。本刀具：锚点命中数与声明不符 ⇒ **抛异常退出**，
   一个字节都不改。**量具「静默跳过」比量具报错危险得多。**
3. 🔴 **切之前断言锚点恰好命中 N 次，切完打印「变异已落地」** —— 并把落地证据（改前/改后
   那一段的 md5）一起打出来，让「变异没生效」与「判据瞎了」这两种存活分得开。

# 住址

本文件住 `<worktree>/evidence/K-R101-cut.py`，被测对象**恒是它自己所在的那棵工作树**
（`REPO = 本文件的祖父目录`）—— 不接受路径参数，免得像 `5k` 那样「同一住址下先后住过
两份被测对象不同的量具」。

用法：
    python3 evidence/K-R101-cut.py list
    python3 evidence/K-R101-cut.py apply <id>
    python3 evidence/K-R101-cut.py restore
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BACKUP = REPO / "evidence" / ".kr101-cut-backup"
STATE = BACKUP / "state.json"


def md5(b: bytes) -> str:
    return hashlib.md5(b).hexdigest()[:12]


class Cut:
    """一刀 = 若干处 (文件, 锚点, 替换, 锚点应命中几次)。`kind` 只有 replace / delete。"""

    def __init__(self, cid: str, dod: str, why: str, edits: list[tuple]) -> None:
        self.cid, self.dod, self.why, self.edits = cid, dod, why, edits


CUTS: list[Cut] = [
    Cut(
        "M2", "KR101D1",
        "① 中途把 raw 丢掉 —— **展示那一层**把它扔了（字段还在类型里，正是件文件点名的失效方向）",
        [("replace", "src/account-usage.ts", "  pre.textContent = raw;", "  pre.textContent = \"\";", 1)],
    ),
    Cut(
        "M3", "KR101D1",
        "① 的另一处刀口：**取数那一层**把 raw 换成空串（IPC 上有、界面上没有）",
        [("replace", "src/account-usage.ts",
          '      ? { status: "screen", raw: result.raw ?? "" }',
          '      ? { status: "screen", raw: "" }', 1)],
    ),
    Cut(
        "M4", "KR101D1",
        "③ 把**空屏判成失败**（`captured=true` ∧ `raw==\"\"` ⇒ probe-failed）",
        [("replace", "src/account-usage.ts",
          '    outcome = result.captured\n      ? { status: "screen", raw: result.raw ?? "" }',
          '    outcome = result.captured && (result.raw ?? "") !== ""\n      ? { status: "screen", raw: result.raw ?? "" }', 1)],
    ),
    Cut(
        "M5", "KR101D2",
        "① 生产段又调回解析器（`fetchAccountUsage` 里把那一屏喂进 `parseUsageCapture`）",
        [("replace", "src/account-usage.ts",
          'import { LOCAL_ORIGIN } from "./accounts.ts";',
          'import { LOCAL_ORIGIN } from "./accounts.ts";\nimport { parseUsageCapture } from "./account-usage-parse.ts";', 1),
         ("replace", "src/account-usage.ts",
          '      ? { status: "screen", raw: result.raw ?? "" }',
          '      ? { status: "screen", raw: parseUsageCapture(result.raw ?? "").status }', 1)],
    ),
    Cut(
        "M6", "KR101D2",
        "③ **把解析器整份删掉** —— 本件是退役不是删除，这个方向也要拦",
        [("delete", "src/account-usage-parse.ts", None, None, 1)],
    ),
    Cut(
        "M7", "KR101D2",
        "③ 只删**冻结夹具**（解析器还在、测试还在，但绿灯从此证不了任何东西）",
        [("delete", "src/__fixtures__/usage-capture-2026-07-31.txt", None, None, 1)],
    ),
    Cut(
        "M8", "KR101D3",
        "墓碑第一样（**谁退的**：`R59` 逐字 ＋ 日期）被抹掉",
        [("replace", "src/account-usage-parse.ts",
          "解析层代码保留, 但是功能先退役", "（当时那句裁词）", 1)],
    ),
    Cut(
        "M9", "KR101D3",
        "墓碑第二样（**复活条件**）退化成空话",
        [("replace", "src/account-usage-parse.ts",
          " *   · **百分比**（一个数字，而不是一屏字）\n"
          " *   · **进度条**\n"
          " *   · **跨账号排序**\n"
          " *   · **「哪个号快满了」**",
          " *   · 以后可能要用", 1)],
    ),
    Cut(
        "M10", "KR101D3",
        "🔴 墓碑第三样（**那句会救人的话**：绿灯只证明对冻结夹具有效）被抹掉",
        [("replace", "src/account-usage-parse.ts",
          " * **「对那份 2026-07-31 的冻结夹具有效」，不证明「对真机 `/usage` 有效」。**",
          " * 它的测试今天仍然全绿。", 1)],
    ),
    Cut(
        "M11", "KR101D4",
        "① daemon 的 `SUBCOMMANDS` 上长出一条「整条探针」式的大动作 `--usage-probe`",
        [("replace", "remote-daemon-proto/src/main.rs",
          '    "--usage",\n];',
          '    "--usage",\n    "--usage-probe",\n];', 1)],
    ),
    Cut(
        "M12", "KR101D4",
        "② 编排登记表**塌成一条**（一个 owner 吃下整条探针）",
        [("replace", "src-tauri/src/account_usage.rs",
          '        "--oneshot-session",\n    ),',
          '        "--launch",\n    ),', 1)],
    ),
    Cut(
        "M13", "7u",
        "🔴 `7u`：**把本件的实现整个掏空，判据一条不动** —— 还有多少条新断言仍绿",
        [
            # ① 原文到不了界面（展示层掏空 ＋ 空屏那条说明也拿掉）
            ("replace", "src/account-usage.ts", "  pre.textContent = raw;", '  pre.textContent = "";', 1),
            ("replace", "src/account-usage.ts",
             '  if (raw === "") {\n'
             '    const note = document.createElement("div");\n'
             '    note.className = "usage-screen-empty";',
             '  if (raw === "\\u0000") {\n'
             '    const note = document.createElement("div");\n'
             '    note.className = "usage-screen-empty";', 1),
            # ② 取数那一层也把 raw 丢掉
            ("replace", "src/account-usage.ts",
             '      ? { status: "screen", raw: result.raw ?? "" }',
             '      ? { status: "screen", raw: "" }', 1),
            # ③ 墓碑三样全撤
            ("replace", "src/account-usage-parse.ts",
             "解析层代码保留, 但是功能先退役", "（当时那句裁词）", 1),
            ("replace", "src/account-usage-parse.ts",
             " *   · **百分比**（一个数字，而不是一屏字）\n"
             " *   · **进度条**\n"
             " *   · **跨账号排序**\n"
             " *   · **「哪个号快满了」**",
             " *   · 以后再说", 1),
            ("replace", "src/account-usage-parse.ts",
             " * **「对那份 2026-07-31 的冻结夹具有效」，不证明「对真机 `/usage` 有效」。**",
             " * 它的测试今天仍然全绿。", 1),
            # ④ 编排登记表塌成一条
            ("replace", "src-tauri/src/account_usage.rs",
             '        "--oneshot-session",\n    ),',
             '        "--launch",\n    ),', 1),
        ],
    ),
]

BY_ID = {c.cid: c for c in CUTS}


def _fail(msg: str) -> None:
    raise SystemExit(f"🔴 拒跑（fail-closed）：{msg}")


def apply(cid: str) -> None:
    """🔴 **一刀可以碰同一个文件的多处** —— 那正是第一版栽的地方，读一遍再改这里。

    第一版把「备份 ＋ 写盘」放在同一个循环里，于是同文件第二处的备份**盖住了第一处的原件**，
    而每一处又都是从**原文**算出来的 `after`（写回去时互相覆盖）⇒ 落地的只有最后一处、
    还原回来的是中间态。它**没有静默**（`touched` 里的重复项让 restore 当场 fail-closed 报错），
    但那已经是在收拾残局了。
    ⇒ 现在：**先按文件收齐全部替换、在同一份文本上依次施加**，备份**每个文件只取一次**。
    """
    if STATE.exists():
        _fail(f"上一刀还没还原（{json.loads(STATE.read_text())['cut']}）—— 先 restore")
    cut = BY_ID.get(cid) or _fail(f"没有这一刀：{cid}")
    BACKUP.mkdir(parents=True, exist_ok=True)

    # ── 先**全部**校验（在累加后的文本上逐处数命中），一处不合就一个字节都不改 ──────
    originals: dict[str, str] = {}
    staged: dict[str, str | None] = {}   # None = 这个文件本刀要删掉
    order: list[str] = []
    for kind, rel, old, new_s, want in cut.edits:
        f = REPO / rel
        if not f.is_file():
            _fail(f"{rel} 不在盘上 —— 锚点取不到")
        if rel not in originals:
            originals[rel] = f.read_text(encoding="utf8", errors="surrogateescape")
            staged[rel] = originals[rel]
            order.append(rel)
        if kind == "delete":
            staged[rel] = None
            continue
        cur = staged[rel]
        if cur is None:
            _fail(f"{rel} 本刀已被标记删除，后面又要在它上面替换 —— 刀写错了")
        hit = cur.count(old)
        if hit != want:
            _fail(
                f"{rel} 里锚点命中 {hit} 次，声明 {want} 次 —— 锚点漂了，本刀作废\n"
                f"  锚点：{old[:90]!r}"
            )
        staged[rel] = cur.replace(old, new_s, 1)

    # ── 备份（每文件一次）＋ 落地 ────────────────────────────────────────────
    for rel in order:
        f = REPO / rel
        dst = BACKUP / rel.replace("/", "__")
        shutil.copyfile(f, dst)          # 🔴 copyfile 不带 mtime，还原时再 touch
        if staged[rel] is None:
            f.unlink()
            print(f"  变异已落地  {rel}  删除（原 md5 {md5(dst.read_bytes())}）")
        else:
            f.write_text(staged[rel], encoding="utf8", errors="surrogateescape")
            os.utime(f, None)
            print(
                f"  变异已落地  {rel}  "
                f"{md5(originals[rel].encode('utf8', 'surrogateescape'))} → "
                f"{md5(staged[rel].encode('utf8', 'surrogateescape'))}"
            )
    STATE.write_text(json.dumps({"cut": cid, "files": order}, ensure_ascii=False))
    print(f"★ {cid}（{cut.dod}）：{cut.why}")


def restore() -> None:
    if not STATE.exists():
        _fail("没有在飞的变异 —— 没什么可还原的（这也是 fail-closed：别静默当成还原过了）")
    st = json.loads(STATE.read_text())
    for rel in st["files"]:
        src = BACKUP / rel.replace("/", "__")
        if not src.is_file():
            _fail(f"备份不见了：{rel} —— 不许静默跳过，手工从 git 取回")
        dst = REPO / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)   # 不带 mtime
        os.utime(dst, None)         # 🔴 新 mtime ⇒ cargo/vite 一定重编
        src.unlink()
        print(f"  已还原  {rel}  md5 {md5(dst.read_bytes())}  mtime 已刷新")
    STATE.unlink()
    print(f"★ {st['cut']} 已还原")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "list"
    if cmd == "list":
        for c in CUTS:
            print(f"{c.cid:>4}  {c.dod}  {c.why}")
    elif cmd == "apply":
        apply(sys.argv[2])
    elif cmd == "restore":
        restore()
    else:
        _fail(f"不认识的动作 {cmd}")
