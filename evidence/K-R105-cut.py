#!/usr/bin/env python3
"""`K-R105` 的刀具。**一趟一刀**，`apply <刀名>` 之后跑一趟门禁，再 `restore`。

住址：本文件住在 `.claude/worktrees/k-r105/evidence/`；`ROOT` = **它所在的那棵树的仓根**。
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜。）

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**
  （`K-R87` 的 `cut.sh` 静默跳过）。
- 🔴 **同文件多处编辑当心后写覆盖前写**（`K-R104`：首趟只落地一处，而它照印「变异已落地」）
  ⇒ 本文件把同一份文件的全部替换**在内存里做完再写一次盘**，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红，`K-R88`）
  ⇒ `restore` 是**重写原文**，mtime 必变。
- 备份落 `evidence/.K-R105-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。
"""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BACKUP = ROOT / "evidence" / ".K-R105-cut-backup.json"

INVAR = "doc/INVARIANTS.md"
WIRE = "src-tauri/src/backend/control/launch_wire.rs"
KILL = "src-tauri/src/backend/control/daemon_kill.rs"
PARITY = "src-tauri/src/backend/control/launch_payload_parity.rs"
SSH = "src-tauri/src/ssh_source.rs"
ROUTE = "src-tauri/src/backend/control/daemon_route.rs"
TABS = "src/tabs.ts"
SEAT = "src/session-backend.ts"

# `刀名 -> [(文件, 锚点, 替换, 期望命中数)]`；`替换 is None` 表示删文件（另行处理）。
CUTS = {
    # ── KR105D1 ───────────────────────────────────────────────────────────
    # ① 改**行为**不改答案：让 monitor 那棵树的生产段真的发一次 `create-or-attach`
    #    ⇒ 派生判词 `部分切` → `全切`，而文档还写着 `部分切`。
    "D1-behave1": [
        (
            ROUTE,
            "pub(crate) fn no_channel(origin: &str) -> Routed {\n",
            "pub(crate) fn no_channel(origin: &str) -> Routed {\n"
            '    let _k_r105 = "create-or-attach";\n',
            1,
        )
    ],
    # ③ 改**行为**不改答案：把 `daemonless` 那一档的一个载体加回来
    #    ⇒ 派生判词 `已退役` → `那一档还在`。
    "D1-behave3": [
        (
            SSH,
            "fn record_verified_build(origin: &str, build_id: &str) {\n",
            "fn record_verified_build(origin: &str, build_id: &str) {\n"
            '    let _k_r105 = "daemonless_stream_loop";\n',
            1,
        )
    ],
    # 改**说法**不改行为：只把文档里 ③ 的判词换成闭集里的另一个。
    "D1-say": [(INVAR, "〔现打③〕已退役", "〔现打③〕那一档还在", 1)],
    # ── KR105D2 ───────────────────────────────────────────────────────────
    # 说法≠现打：`remote-launch.ts` 那格从 `Off` 改成 `On`（生产调用方其实是 0）。
    "D2-say": [
        (
            WIRE,
            '            ],\n            Reach::OffProductionPath,\n            "只有 `remote-launch.test.ts`',
            '            ],\n            Reach::OnProductionPath,\n            "只有 `remote-launch.test.ts`',
            1,
        )
    ],
    # 事实变了：真把一个 builder 接进生产 ⇒ `remote-launch.ts` 从 Off 变 On。
    "D2-behave": [
        (
            TABS,
            'import { mintSessionTmuxName } from "./remote-launch";',
            'import { mintSessionTmuxName, buildAttachCmd } from "./remote-launch";\n'
            "export const kr105Probe = (n: string): string => buildAttachCmd(n);",
            1,
        )
    ],
    # 纪律⑱：刀不许连量具一起砍 —— 把尺子B 整张表从人群里拿走（改名）。
    "D2-unregister": [
        (WIRE, "TS_FALLBACK_REACH", "TS_FALLBACK_REACH_OFFTABLE", 9),
    ],
    # ── KR105D4 ───────────────────────────────────────────────────────────
    # 最小面：创建路径还在，只是它的理由**不再点名**那条校验器 ⇒ 校验器落单。
    "D4-orphan": [
        (
            KILL,
            "由 `src/shell-quote.ts::isValidNewTmuxName` 校验（见 `VALIDATORS`）",
            "由上游那个校验器校验",
            1,
        )
    ],
    # 粗刀（也是 `KR105D3` 的实证）：真删座 + 撤它那条创建路径。
    "D4-droppath": [
        (
            KILL,
            '        (\n            "src/session-backend.ts",\n            CreationVerdict::UpstreamValidated,\n            "它只是**渲染器**：名字由上游 `mintTmuxName` 产、由 `src/shell-quote.ts::isValidNewTmuxName` 校验（见 `VALIDATORS`）",\n        ),\n',
            "",
            1,
        ),
        ("<DELETE>", SEAT, "", 1),
    ],
    # 复核「被守的那条对拍本身不是 `x == x`」（`K-R89` 逮到的活体形状，本件 §5 点名要先确认）。
    "D4-selffix": [
        (
            PARITY,
            "            if got != want {",
            "            let want = got.clone();\n            if got != want {",
            1,
        )
    ],
    # ── 7u：把本件实现整个退掉（逐处掏空，不用 `git checkout`）─────────────
    "7u": [
        (INVAR, "〔现打①〕部分切", "〔已退回〕①", 1),
        (INVAR, "〔现打②〕前端仍产 attach", "〔已退回〕②", 1),
        (INVAR, "〔现打③〕已退役", "〔已退回〕③", 1),
    ],
}


def die(msg: str) -> None:
    print(f"❌ {msg}", file=sys.stderr)
    sys.exit(3)


def apply(name: str) -> None:
    if name not in CUTS:
        die(f"没有这把刀：{name}（有的是 {sorted(CUTS)}）")
    if BACKUP.exists():
        die(f"{BACKUP} 还在 —— 上一刀没还原，拒跑")
    edits = CUTS[name]
    # ── 第一遍：只读，逐处断言锚点命中数。差一处就整刀不落地。
    per_file: dict[str, list[tuple[str, str, int]]] = {}
    deletes: list[str] = []
    for a, b, c, n in edits:
        if a == "<DELETE>":
            p = ROOT / b
            if not p.is_file():
                die(f"要删的 {b} 不在盘上")
            deletes.append(b)
            continue
        p = ROOT / a
        if not p.is_file():
            die(f"读不到 {a}")
        got = p.read_text(encoding="utf8").count(b)
        if got != n:
            die(f"{a} 里锚点命中 {got} 次，期望 {n} —— 锚点漂了，整刀不落地\n锚点：{b[:80]!r}")
        per_file.setdefault(a, []).append((b, c, n))
    # ── 落盘：同一份文件的全部替换**在内存里做完再写一次**（治「后写覆盖前写」）。
    backup: dict[str, str | None] = {}
    for rel, reps in per_file.items():
        p = ROOT / rel
        orig = p.read_text(encoding="utf8")
        backup[rel] = orig
        s = orig
        for old, new, n in reps:
            s = s.replace(old, new, n)
        p.write_text(s, encoding="utf8")
    for rel in deletes:
        p = ROOT / rel
        backup[rel] = p.read_text(encoding="utf8")
        p.unlink()
    BACKUP.write_text(json.dumps(backup, ensure_ascii=False), encoding="utf8")
    # ── 回读逐处核对：新串在、旧锚点按预期消失/减少。
    for rel, reps in per_file.items():
        s = (ROOT / rel).read_text(encoding="utf8")
        for old, new, n in reps:
            if new and new not in s:
                die(f"回读失败：{rel} 里没有新串 —— 变异**没有**落地")
            if s.count(old) != 0 and old in new:
                pass  # 新串里含旧串（如加前缀），不判
            elif s.count(old) != 0:
                die(f"回读失败：{rel} 里旧锚点还剩 {s.count(old)} 处")
    for rel in deletes:
        if (ROOT / rel).exists():
            die(f"回读失败：{rel} 没删掉")
    print(f"✅ 变异已落地：{name}")
    for rel in sorted(set(list(per_file) + deletes)):
        print(f"   · {rel}")


def restore() -> None:
    if not BACKUP.exists():
        die("没有备份 —— 没有刀在身上")
    backup = json.loads(BACKUP.read_text(encoding="utf8"))
    for rel, orig in backup.items():
        p = ROOT / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(orig, encoding="utf8")  # 重写原文，**不是 cp -a**：mtime 必变
    BACKUP.unlink()
    print(f"✅ 已还原 {len(backup)} 份")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        die("用法：K-R105-cut.py apply <刀名> | restore | list")
    if sys.argv[1] == "list":
        for k, v in CUTS.items():
            print(f"{k:14s} {len(v)} 处编辑")
    elif sys.argv[1] == "restore":
        restore()
    elif sys.argv[1] == "apply":
        apply(sys.argv[2])
    else:
        die(f"不认识 {sys.argv[1]}")
    _ = shutil  # 占位：本文件刻意不用 shutil.copy 系（禁 cp -a）
