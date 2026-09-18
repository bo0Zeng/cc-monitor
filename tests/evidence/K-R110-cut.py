#!/usr/bin/env python3
"""`K-R110` 的刀具。**一趟一刀（或一组）**：`apply <刀名>…` → 跑一趟沙箱门禁 → `restore`。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜）。
⚠ 与 `K-R106-cut.py` / `K-R101-cut.py` 是**不同的量具**（被测对象是不同的树/不同的件），
别互相照搬读数。

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**。
- 🔴 同一份文件的多处替换**在内存里做完再写一次盘**，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）⇒ `restore` 是**重写原文**。
- 备份落 `evidence/.K-R110-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。

刀名：
  k1  —— 往 `remote-daemon-proto/src/plugin/mod.rs` **今天漏出测试段的那 31 行**里插一处
         `invoke::run(` 字面量（`plugin_walk_fixture::this_fixture_never_ships` 的天花板
         **本该**逮到它：那条天花板逐字登记的合法命中只有 `layering_guard.rs` 一份）。
  k3  —— 在**另一份文件**（`remote-daemon-proto/src/readonly_guard.rs` **最后一个**测试模块、
         **最后一个 `#[test]` 之后**）造一个**同形**新反例：一段内容含列 0 `}` 的 `r#"…"#`。
         位置刻意选在最后一个 `#[test]` 之后 —— 与 `plugin/mod.rs` 同形（老守门人两样都看不见）。
  k4  —— 把本件新加的那条判据（`assert_test_module_ranges_are_brace_balanced` ＋ 它的两条自检 ＋ 它在
         `assert_tree_strips_clean` 里的调用点）**整段拿掉**（阴性对照）。
  k5  —— 把本件的**修法**退回（收尾针重新在**裸文本**上找 `\\n}`，不看词法掩码）。

用法：
  python3 evidence/K-R110-cut.py apply k1
  python3 evidence/K-R110-cut.py apply k3 k5
  python3 evidence/K-R110-cut.py restore
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BACKUP = ROOT / "evidence" / ".K-R110-cut-backup.json"

PLUGIN = "remote-daemon-proto/src/plugin/mod.rs"
ROGUARD = "remote-daemon-proto/src/readonly_guard.rs"
CORE = "src-tauri/crates/guard-core/src/lib.rs"

# ── k1：漏出区间里的探针 ──────────────────────────────────────────────────────
K1_ANCHOR = '''            "一张擦掉了名字的真码表只被认出 {caught} 行 —— 形状那两条此刻在空转"
        );
'''
K1_PATCH = K1_ANCHOR + '''        // KR110 刀①/②探针：这一行落在**今天漏出测试段的那 31 行**里。
        // 形状照 `layering_guard.rs` 那两处已登记的合成样本（字符串字面量，不起进程）。
        let kr110_probe = "invoke::run(";
        assert!(!kr110_probe.is_empty());
'''

# ── k3：同形新反例（另一份文件）────────────────────────────────────────────────
K3_ANCHOR = '''        assert!(
            blind > 0,
            "「看得见却判不了它写什么」合计 0 处 —— 盘上明明有起进程与远端 exec 两族，\\
             这张读数正在把自己说得比实际干净"
        );
'''
K3_PATCH = K3_ANCHOR + '''        // KR110 刀③：**同形**新反例 —— 一段内容含列 0 `}` 的原始字符串。
        // 刻意放在本文件**最后一个 `#[test]` 之后**：老守门人（残留 `#[test]` / 未剥的 `mod` 行）
        // 两样都看不见，与 `plugin/mod.rs` 那一处同形。
        let kr110_shape = r#"
fn synthetic() {
}"#;
        assert!(kr110_shape.contains("synthetic"));
'''

# ── k4：把新判据整段拿掉（阴性对照）──────────────────────────────────────────
K4_CALL_ANCHOR = """            // 第三半〔`K-R110` 09-13〕：区间**花括号净配平**。喂的是**原文**不是 `prod` ——
            // 它要判的正是「区间切在哪儿」，而 `prod` 已经是切完的产物。
            assert_test_module_ranges_are_brace_balanced(&who, &src);
"""
K4_CALL_PATCH = """"""

# ── k5：修法退回裸文本（收尾针重新在裸 `src` 上找）────────────────────────────
K5_ANCHOR = """    let masked = mask_all_literals(src);
    let hay: &str = masked.as_deref().unwrap_or(src);
"""
K5_PATCH = """    let masked = mask_all_literals(src);
    let hay: &str = src;
    let _ = &masked;
"""

KNIVES = {
    "k1": [(PLUGIN, K1_ANCHOR, K1_PATCH, 1)],
    "k3": [(ROGUARD, K3_ANCHOR, K3_PATCH, 1)],
    "k4": [(CORE, K4_CALL_ANCHOR, K4_CALL_PATCH, 1)],
    "k5": [(CORE, K5_ANCHOR, K5_PATCH, 1)],
}
# k4 的第二半（判据本体整段删掉）在运行期按标记块切，见 `strip_marked_block`。
K4_BLOCKS = [
    ("// ── KR110D1 新判据：切出来的每一段，花括号必须净配平 ──", "// ── /KR110D1 ──"),
    ("    // ── KR110D1-selftest ──", "    // ── /KR110D1-selftest ──"),
]


# 判据摘掉之后，头注里那三处 `[`…`]` 引用会变成**指不到的散文名字** ——
# monitor 的 `structural_scan::every_dead_name_named_in_the_prose_is_declared_dead`
# 当场逮住（09-13 实打，粗刀那一趟 `1468 passed; 1 failed`）。
# 阴性对照要问的是「刀③ 会不会红」，不是「摘判据会不会红」⇒ 精刀把这三处一起改成不带符号的措辞。
K4_PROSE = [
    ("/// ② 补 [`assert_test_module_ranges_are_brace_balanced`]（匹配单位换成**计数**，",
     "/// ② 补一条**新判据**（匹配单位换成**计数**，"),
    ("/// 守着这一条的是 [`assert_test_module_ranges_are_brace_balanced`]。",
     "/// 守着这一条的是本模块那条新判据。"),
    ("///   以及本模块的 [`assert_test_module_ranges_are_brace_balanced`]。",
     "///   以及本模块那条新判据。"),
]


def strip_marked_blocks(text: str) -> str:
    for beg, end in K4_BLOCKS:
        b = text.index(beg)
        e = text.index(end) + len(end) + 1
        text = text[:b] + text[e:]
    for a, b2 in K4_PROSE:
        assert text.count(a) == 1, a
        text = text.replace(a, b2, 1)
    # 删段留下的多余空行会让 `cargo fmt --check` 红 —— 那是刀的痕，不是判据的读数。
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    return text


def apply(names):
    if BACKUP.exists():
        print(f"❌ 已有备份 {BACKUP} —— 上一刀没还原，拒绝落地", file=sys.stderr)
        sys.exit(3)
    edits = {}
    for n in names:
        if n not in KNIVES:
            print(f"❌ 没有这把刀：{n}（有的是 {sorted(KNIVES)}）", file=sys.stderr)
            sys.exit(3)
        for rel, anchor, patch, want in KNIVES[n]:
            edits.setdefault(rel, []).append((n, anchor, patch, want))
    backup = {}
    newtext = {}
    for rel, ops in edits.items():
        p = ROOT / rel
        orig = p.read_text(encoding="utf8")
        backup[rel] = orig
        cur = orig
        for n, anchor, patch, want in ops:
            got = cur.count(anchor)
            if got != want:
                print(
                    f"❌ 刀 {n}：`{rel}` 里锚点命中 {got} 次（要 {want} 次）—— 一个字节都不落地",
                    file=sys.stderr,
                )
                sys.exit(3)
            cur = cur.replace(anchor, patch, want)
        if "k4" in [o[0] for o in ops]:
            for beg, end in K4_BLOCKS:
                if cur.count(beg) != 1 or cur.count(end) != 1:
                    print(
                        f"❌ 刀 k4：`{rel}` 里标记块 {beg!r} 不是恰好一对 —— 一个字节都不落地",
                        file=sys.stderr,
                    )
                    sys.exit(3)
            cur = strip_marked_blocks(cur)
        newtext[rel] = cur
    BACKUP.write_text(json.dumps(backup, ensure_ascii=False), encoding="utf8")
    for rel, cur in newtext.items():
        (ROOT / rel).write_text(cur, encoding="utf8")
    # 回读逐处核对
    for rel, cur in newtext.items():
        back = (ROOT / rel).read_text(encoding="utf8")
        assert back == cur, f"回读对不上：{rel}"
    for n in names:
        for rel, anchor, patch, want in KNIVES[n]:
            txt = (ROOT / rel).read_text(encoding="utf8")
            if patch == "":
                # 「删掉一段」这一形：落地形是空串，`count("")` 恒等于长度+1 ⇒ 改核**锚点已消失**。
                got = txt.count(anchor)
                print(f"变异已落地：刀 {n} · {rel} · 锚点已删除（残留 {got} 处，要 0）")
                if got != 0:
                    sys.exit(3)
                continue
            got = txt.count(patch)
            print(f"变异已落地：刀 {n} · {rel} · 落地形逐处命中 {got} 次（要 {want}）")
            if got != want:
                sys.exit(3)
    if "k4" in names:
        txt = (ROOT / CORE).read_text(encoding="utf8")
        for beg, _ in K4_BLOCKS:
            assert beg not in txt, beg
        assert "assert_test_module_ranges_are_brace_balanced" not in txt
        print(f"变异已落地：刀 k4 · {CORE} · 判据本体 ＋ 它的两条自检已整段移除")


def restore():
    if not BACKUP.exists():
        print("· 没有备份，盘上本来就是原文", file=sys.stderr)
        return
    backup = json.loads(BACKUP.read_text(encoding="utf8"))
    for rel, orig in backup.items():
        (ROOT / rel).write_text(orig, encoding="utf8")  # 重写原文 ⇒ mtime 必变
    BACKUP.unlink()
    print(f"· 已还原 {len(backup)} 份：{sorted(backup)}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(3)
    if sys.argv[1] == "apply":
        apply(sys.argv[2:])
    elif sys.argv[1] == "restore":
        restore()
    else:
        print(f"❌ 不认识：{sys.argv[1]}", file=sys.stderr)
        sys.exit(3)
