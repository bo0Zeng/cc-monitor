#!/usr/bin/env python3
"""`K-R106` 的刀具。**一趟一刀**：`apply <刀名>` → 跑一趟门禁 → `restore`。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜）。
⚠ 与 `K-R105-cut.py` 是**两份不同的量具**（被测对象是两棵不同的树），别互相照搬读数。

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**
  （`K-R87` 的 `cut.sh` 静默跳过）。
- 🔴 **同文件多处编辑当心后写覆盖前写**（`K-R104`：首趟只落地一处，而它照印「变异已落地」）
  ⇒ 同一份文件的全部替换**在内存里做完再写一次盘**，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红，`K-R88`）
  ⇒ `restore` 是**重写原文**，mtime 必变。
- 备份落 `evidence/.K-R106-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。
"""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BACKUP = ROOT / "evidence" / ".K-R106-cut-backup.json"

HIST = "src-tauri/src/history.rs"
KILL = "src-tauri/src/backend/control/daemon_kill.rs"
WIRE = "src-tauri/src/backend/control/launch_wire.rs"
PARITY = "src-tauri/src/backend/control/launch_payload_parity.rs"
ARCH = "doc/ARCHITECTURE.md"
CONTRIB = "doc/CONTRIBUTING.md"
USAGE = "doc/账号用量-usage抓取方案.md"
SEAT = "src/session-backend.ts"

# ── 复用的片段 ────────────────────────────────────────────────────────────
ATTACH_ARM = (
    "        LocalPsAction::Attach => (String::new(), Some(ci::Action::Attach { name })),"
)
NEW_ARM = "        LocalPsAction::Attach => (String::new(), Some(ci::Action::New)),"

OLD_PATH_GATE = "        return Err(OLD_PATH_CANNOT_ATTACH.into());"
OLD_PATH_OPEN = '        return Ok(LocalLaunchChoice::Fixed("cc".to_string()));'

SPAWN_GATE = """        if matches!(action, LocalPsAction::Attach) {
            return Err(ATTACH_IS_NOT_A_SPAWN.into());
        }
"""

NAME_ANCHOR = """        return Err(NO_TMUX_NAME.into());
    };
    let sanitized = sanitize_launcher(launcher)?;"""
# ⚠ **第一版这把刀活了下来，而那不是判据瞎** —— 它的替换是 `format!("ccm-oneshot-{name}")`，
#    而 `name` 本身以 `-cc` 结尾 ⇒ `ccm-oneshot-s1abcdef-cc` **仍然过得了 `gate_core`**
#    （后缀形那一支只看结尾，不看前面挂了什么）。⇒ 换成**整名替换**成 `K-R87` 那个真形状。
#    这条现打本身值得报：**`ccm-oneshot-` 这个前缀不足以让一个名字掉出 Gate 2。**
NAME_R87 = """        return Err(NO_TMUX_NAME.into());
    };
    let name_owned = format!("ccm-oneshot-{}", "usage");
    let name = name_owned.as_str();
    let sanitized = sanitize_launcher(launcher)?;"""

SEAT_ROW = """        (
            "src/session-backend.ts",
            CreationVerdict::UpstreamValidated,
            "它只是**渲染器**：名字由上游 `mintTmuxName` 产、由 `src/shell-quote.ts::isValidNewTmuxName` 校验（见 `VALIDATORS`）",
        ),
"""

DOC_JUDGE_WIDE = "                if text.contains(needle.as_str()) {"
DOC_JUDGE_NARROW = (
    '                if rel.ends_with("IPC-PROTOCOL.md") && text.contains(needle.as_str()) {'
)

ARCH_TODAY = "今天**盘上只有后端这一条**；回潮闸住 `tmux_daemon_gate_guard.rs`"
ARCH_WRITEBACK = "今天仍是**过渡期回落**；回潮闸住 `tmux_daemon_gate_guard.rs`"

CONTRIB_TODAY = "> ⇒ **今天要改 kill / send-keys 的行为，盘上只有 Rust 那一条路可改。**"
CONTRIB_WRITEBACK = (
    "> ⇒ 那条 shell 串今天仍是 C7 过渡期回落。"
)

USAGE_TODAY = "那条一次性 ssh 的第二条路 **`K-R72`（09-12）整块删了**，今天盘上只有后端这一条"
USAGE_WRITEBACK = "那条一次性 ssh 今天仍是 C7 过渡期回落"

SHADOW_ANCHOR = "            if got != want {"
SHADOW_CUT = "            let want = got.clone();\n            if got != want {"

HARDEN_BLOCK = """        for side in ["let want", "let got"] {
            let n = body.matches(side).count();
            assert_eq!(
                n, 1,
                "对拍函数体里 `{side}` 出现了 {n} 次（只许 1 次）。\\n\\
                 **两次 = 有人用同名遮蔽把这条对拍的一边换掉了** —— 上面那两条\\n\\
                 「那一行还在」的断言对这一形是瞎的（原来那一行还在，而它已经不参与比较了）。\\n\\
                 ⚠ 真要重构变量名，把本条一起改；但先想清楚：改完之后，\\n\\
                 「左边取自入库夹具、右边跑生产命令」这两件事还有谁在看着。"
            );
        }
"""

# `刀名 -> [(文件, 锚点, 替换, 期望命中数)]`；`("<DELETE>", 路径, "", 1)` 表示删文件。
CUTS = {
    # ── KR106D1 ───────────────────────────────────────────────────────────
    # ② 那一臂改成产 `new`：动词不再是 attach ⇒ **另起一条 claude**。
    "D1-newarm": [(HIST, ATTACH_ARM, NEW_ARM, 1)],
    # ③ 重演 `K-R87`：渲染路自己给容器名加一个**两形都不命中**的前缀。
    "D1-shape": [(HIST, NAME_ANCHOR, NAME_R87, 1)],
    # ④ 旧路那道 fail-closed 打开：它会给 attach 渲出一个**拉起器**。
    "D1-oldpath": [(HIST, OLD_PATH_GATE, OLD_PATH_OPEN, 1)],
    # ④ 第二半：`launch_local` 那道闸拿掉 ⇒ attach 被 spawn 掉（stdio 全 null）。
    "D1-spawn": [(HIST, SPAWN_GATE, "", 1)],
    # ── KR106D2 ───────────────────────────────────────────────────────────
    # 真删座 ＋ 撤它那条创建路径 —— 这一刀吐的就是「今天删掉会怎样」的现打链。
    "D2-droppath": [(KILL, SEAT_ROW, "", 1), ("<DELETE>", SEAT, "", 1)],
    # 最小面：创建路径还在，只是它的理由**不再点名**那条校验器 ⇒ 校验器落单（`D2` 第③刀）。
    "D2-orphan": [
        (
            KILL,
            "由 `src/shell-quote.ts::isValidNewTmuxName` 校验（见 `VALIDATORS`）",
            "由上游那个校验器校验",
            1,
        )
    ],
    # ── KR106D3 ───────────────────────────────────────────────────────────
    # ③ 在一份耐久文档里**把那句话写回去** ⇒ 必须红。
    "D3-writeback": [(ARCH, ARCH_TODAY, ARCH_WRITEBACK, 1)],
    # ① 阴性对照：**人群退回加固前那一份**（判定只看 `IPC-PROTOCOL.md`）＋ 同样写回
    #    ⇒ 预期**全绿** = 「只改文档不扩人群」买不到任何东西。
    "D3-narrow": [
        (KILL, DOC_JUDGE_WIDE, DOC_JUDGE_NARROW, 1),
        (ARCH, ARCH_TODAY, ARCH_WRITEBACK, 1),
    ],
    # ── KR106D4 ───────────────────────────────────────────────────────────
    # ③ 逐字复刻 `K-R105` 那把「同名遮蔽」的刀（原行一字不动）⇒ 必须红。
    "D4-shadow": [(PARITY, SHADOW_ANCHOR, SHADOW_CUT, 1)],
    # ① 阴性对照：**把加固退回「子串存在性」**（拿掉那段计数）＋ 同一把遮蔽刀
    #    ⇒ 预期**全绿** = 那一红确实是加固买的，不是别处顺手接住的。
    "D4-unharden": [
        (WIRE, HARDEN_BLOCK, "", 1),
        (PARITY, SHADOW_ANCHOR, SHADOW_CUT, 1),
    ],
    # ── 7u：把本件实现整个退掉（逐处掏空，不用 `git checkout`）─────────────
    "7u": [
        (HIST, ATTACH_ARM, NEW_ARM, 1),
        (HIST, OLD_PATH_GATE, OLD_PATH_OPEN, 1),
        (HIST, SPAWN_GATE, "", 1),
        (ARCH, ARCH_TODAY, ARCH_WRITEBACK, 1),
        (CONTRIB, CONTRIB_TODAY, CONTRIB_WRITEBACK, 1),
        (USAGE, USAGE_TODAY, USAGE_WRITEBACK, 1),
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
            die(f"{a} 里锚点命中 {got} 次，期望 {n} —— 锚点漂了，整刀不落地\n锚点：{b[:90]!r}")
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
        die("用法：K-R106-cut.py apply <刀名> | restore | list")
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
