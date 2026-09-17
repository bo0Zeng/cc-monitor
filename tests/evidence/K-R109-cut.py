#!/usr/bin/env python3
"""`K-R109` 的刀具。**一趟一刀**：`apply <刀名>` → 跑一趟沙箱门禁 → `restore`。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象，不许拿 `cwd` 猜）。
⚠ 与 `K-R105-cut.py` / `K-R106-cut.py` 是**三份不同的量具**（被测对象是三棵不同的树）。
   跑之前先看它印的 `被测对象` 那一行。

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**。
- 🔴 同一份文件的多处替换**在内存里做完再写一次盘**，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）
  ⇒ `restore` 是**重写原文**，mtime 必变。
- 备份落 `evidence/.K-R109-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。

两种刀，形状不同：
- **文本刀**：在终态上做定点替换（`d1-2` / `d2-1` / `d3-1`）。
- **退版刀**：把几份文件整份退回基点 `BASE`，别的留在终态（`d1-1` / `d1-3`）——
  它造的是一个**真实存在过的中间态**，不是我手捏的一个近似。
  ⚠ 退版用 `git show <sha>:<path>`（**只写工作树，不碰索引**），不用 `git checkout -- `。
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BACKUP = ROOT / "evidence" / ".K-R109-cut-backup.json"

# 本件的基点（`.dispatch.json` 写的那个）。退版刀退到它。
BASE = "b0953e5"

HIST = "src-tauri/src/history.rs"
LIB = "src-tauri/src/lib.rs"
PARITY = "src-tauri/src/parity_ledger.rs"
WIRE = "src-tauri/src/backend/control/launch_wire.rs"
CMDS = "src/ipc/commands.ts"
CMDSV = "src/ipc/commands.vitest.ts"
RUN = "src/remote-launch-run.ts"
RUNV = "src/remote-launch-run.vitest.ts"
INV = "doc/INVARIANTS.md"

SEAT_IMPORT = 'import { sanitizeRemoteLauncher } from "./shell-quote.ts";\n'
SEAT_LINE = '  const attachCmd = SESSION_BACKEND.attach({ kind: "quoted", value: name });\n'

ASK_BACKEND = """  let attachCmd: string;
  try {
    attachCmd = await commands.render_local_attach({ tmuxName: name });
  } catch (err) {"""

LEDGER_ROW = '        ("render_local_attach", "launch.render-attach", Side::Local),\n'

FALLTHROUGH = "    const r = await renderCliViaBackend(ctx, plan, probe);\n    if (r.ok) return r.cmd;"
NO_FALLTHROUGH = (
    "    const r = await renderCliViaBackend(ctx, plan, probe);\n"
    "    if (!r.ok) throw new Error(`掏空：不许回落到兜底 ${r.reason}`);\n"
    "    return r.cmd;"
)

# ── 刀谱 ──────────────────────────────────────────────────────────────────
# 文本刀：{文件: [(锚点, 替换, 该命中几次), …]}
TEXT_CUTS = {
    # `KR109D1` ②：进了注册但 `LEDGER` 不加（**四个数一起往下拧**，
    # 好让唯一可能红的东西是「双向相等」本身，而不是某个数没跟上）。
    "d1-2": {
        PARITY: [
            (LEDGER_ROW, "", 1),
            ('assert_eq!(LEDGER.len(), 148, "命令总数变了")',
             'assert_eq!(LEDGER.len(), 147, "命令总数变了")', 1),
            ('assert_eq!(sides.len(), 66, "能力总数变了")',
             'assert_eq!(sides.len(), 65, "能力总数变了")', 1),
            ('assert_eq!(asym.len(), 27, "不对称能力数变了")',
             'assert_eq!(asym.len(), 26, "不对称能力数变了")', 1),
            ('assert_eq!(kinds.get("natural"), Some(&12), "天然不对称条数变了")',
             'assert_eq!(kinds.get("natural"), Some(&11), "天然不对称条数变了")', 1),
            ("const EXPECTED_LOCAL_OR_BOTH: usize = 93;",
             "const EXPECTED_LOCAL_OR_BOTH: usize = 92;", 1),
        ],
    },
    # `KR109D2` ① 的**干净版**〔第二趟〕：整块换成座那一行，不留任何死代码。
    # 🔴 第一趟 `d2-1` 留了个 `if (false) { … }` 空壳 ⇒ eslint 基线从 7 涨到 8、
    #    `attachCmd` 变成「可能未赋值」把 5 个 vitest 文件一起拖红 —— 那几条红是**刀的伪影**，
    #    不是这条性质的牙。**本版只换那一块**，射程干净。
    "d2-1b": {
        RUN: [
            (SEAT_IMPORT, SEAT_IMPORT + 'import { SESSION_BACKEND } from "./session-backend.ts";\n', 1),
            ('  let attachCmd: string;\n  try {\n    attachCmd = await commands.render_local_attach({ tmuxName: name });\n  } catch (err) {\n    showActionFailureToast(\n      "已就地 resume，但接终端那一句渲不出来",\n      `${String(err)}\\n` +\n        "（接终端那一句归本机后端产，前端不再自己拼一条。" +\n        "刚才载荷已经键进去了，会话本身没事。）",\n    );\n    // ★ 与下面那条同一个道理：**就地 resume 已经成了**，attach 这一跳的成败不改变它。\n    return true;\n  }\n', '  const attachCmd = SESSION_BACKEND.attach({ kind: "quoted", value: name });\n', 1),
        ],
    },
    # `KR109D2` ①：把前端那一处改回问座要（**只改这一处 + 补回 import**，登记表一个字不动）。
    "d2-1": {
        RUN: [
            (SEAT_IMPORT, SEAT_IMPORT + 'import { SESSION_BACKEND } from "./session-backend.ts";\n', 1),
            (ASK_BACKEND, "  const attachCmd = SESSION_BACKEND.attach({ kind: \"quoted\", value: name });\n  if (false) {\n    try {\n      throw new Error(\"unreachable\");\n    } catch (err) {", 1),
        ],
    },
    # `KR109D3`（判 A）：把兜底那条路**掏空** —— 后端拒了不再落到座，直接抛。
    "d3-1": {
        RUN: [(FALLTHROUGH, NO_FALLTHROUGH, 1)],
    },
}

# 退版刀：{刀名: [退回 BASE 的文件, …]}
REVERT_CUTS = {
    # `KR109D1` ①：**`K-R106` 那个真实中间态** —— 只挂 `#[tauri::command]`，
    # 注册面（`generate_handler!` / 包装层 / 三个计数 / `LEDGER`）与接线全部退回基点。
    # 已知答案：`commands.vitest.ts` 的 `C04a` 必须红两条。
    "d1-1": [LIB, PARITY, CMDS, CMDSV, RUN, RUNV, WIRE, INV],
    # `KR109D1` ③：注册面全在（属性 + `generate_handler!` + 包装层 + `LEDGER`），
    # 而**前端一个调用方都没有** —— 接线与两张登记表退回基点。
    "d1-3": [RUN, RUNV, WIRE],
    # 「**把实现整个退掉**，还有多少条新断言仍绿」（`brief` 四·交回时必报那一条）：
    # **实现**那几份退回基点，**判据**那几份（两份 vitest · `launch_wire` 的 ⑤ · `INVARIANTS`）
    # 留在终态。⇒ 剩下的红/绿就是那几条新断言各自的牙。
    "d-hollow": [HIST, LIB, CMDS, PARITY, RUN],
    # 纯基点（**没有刀**，就是 `b0953e5` 本身）—— 只为量一趟 `dead_code` 的第 0 读。
    # ⚠ 它不该拿去跑门禁（跑出来就是基点的读数，`M0` 已经有了）。
    "d0-base": [HIST, LIB, CMDS, CMDSV, PARITY, RUN, RUNV, WIRE, INV],
}


def sh(args: list[str]) -> str:
    return subprocess.run(args, cwd=ROOT, capture_output=True, text=True, check=True).stdout


def save(rels: list[str]) -> None:
    if BACKUP.exists():
        raise SystemExit(f"❌ 已有备份 {BACKUP} —— 上一刀没还原，拒绝再 apply")
    BACKUP.write_text(json.dumps({r: (ROOT / r).read_text() for r in rels}, ensure_ascii=False))


def apply(name: str) -> None:
    if name in TEXT_CUTS:
        spec = TEXT_CUTS[name]
        save(list(spec))
        for rel, edits in spec.items():
            path = ROOT / rel
            src = path.read_text()
            for anchor, repl, want in edits:  # 先全量断言，再改
                got = src.count(anchor)
                if got != want:
                    BACKUP.unlink()
                    raise SystemExit(f"❌ {rel} 锚点命中 {got} 次（要 {want}）：{anchor[:60]!r} —— 一个字节都没落地")
            for anchor, repl, _ in edits:
                src = src.replace(anchor, repl)
            path.write_text(src)
            back = path.read_text()          # 回读逐处核对
            for anchor, repl, _ in edits:
                if repl and repl not in back:
                    raise SystemExit(f"❌ {rel} 写完回读找不到替换体 —— 多处替换互相覆盖了")
            print(f"  变异已落地：{rel}（{len(edits)} 处）")
    elif name in REVERT_CUTS:
        rels = REVERT_CUTS[name]
        save(rels)
        for rel in rels:
            (ROOT / rel).write_text(sh(["git", "show", f"{BASE}:{rel}"]))
            print(f"  变异已落地：{rel} 整份退回 {BASE}")
    else:
        raise SystemExit(f"没有这一刀：{name}（有的是 {sorted(TEXT_CUTS) + sorted(REVERT_CUTS)}）")
    print(f"★ 刀 `{name}` 已落地。现在跑一趟沙箱门禁，读完再 `restore`。")


def restore() -> None:
    if not BACKUP.exists():
        raise SystemExit("❌ 没有备份 —— 没有刀在身上，或者上一趟已经还原过了")
    for rel, text in json.loads(BACKUP.read_text()).items():
        (ROOT / rel).write_text(text)      # 重写原文，**mtime 必变**
        print(f"  还原：{rel}")
    BACKUP.unlink()
    print("★ 已还原。")


if __name__ == "__main__":
    print(f"被测对象 = {ROOT}")
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    if sys.argv[1] == "restore":
        restore()
    elif sys.argv[1] == "apply":
        apply(sys.argv[2])
    else:
        raise SystemExit(__doc__)
