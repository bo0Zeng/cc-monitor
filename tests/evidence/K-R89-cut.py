#!/usr/bin/env python3
"""`K-R89` 死值验刀具 —— 一趟一刀、fail-closed、逐刀自证。

用法（**在宿主上切/还原，跑门禁仍然只走沙箱那一条命令**）：

    python3 evidence/K-R89-cut.py list
    python3 evidence/K-R89-cut.py apply <刀名>
    python3 evidence/K-R89-cut.py restore

设计上被三条前面几件真踩出来的坑逼出来的：

- 🔴 **fail-closed**：每一处编辑先断言锚点**恰好命中 expect 次**，差一次当场退出、
  **一个字节都不落地**（`K-R87` 的 `cut.sh` 只认第 2 行 `# FILES:`，某变异写在第 4 行
  ⇒ 还原静默跳过）。
- 🔴 **同文件多处编辑，后写不许覆盖前写**（`K-R104` 踩过：首趟只落地一处，
  **而它照印「变异已落地」**）⇒ 同一份文件的多处编辑在**同一份内存文本**上顺序做完再写盘，
  并在写盘后**回读**逐处核对。
- 🔴 **不许 `cp -a` 还原**（`K-R88`：连 mtime 一起还原 ⇒ cargo 判「没变」⇒ 读到上一刀的红）
  ⇒ 还原是**把原文重新写一遍**（mtime 必变）。备份在
  `<仓根>/../.kr89-cut-backup/`（**仓外**，不进 git status）。

`MUT` 每一行：`(刀名, 挂哪条 dod, 一句话说它在切什么, [(相对路径, 锚点, 替换, 期望命中次数), …])`
"""

import hashlib
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BACKUP = ROOT.parent / ".kr89-cut-backup"

HIST = "src-tauri/src/history.rs"
CCMI = "src-tauri/src/backend/control/ccm_invocation.rs"
PARITY = "src-tauri/src/backend/control/launch_payload_parity.rs"
PLAN = "remote-daemon-proto/src/control/ccm/plan.rs"
RUN = "src/remote-launch-run.ts"

MUT = [
    # ── KR89D1：六格表有没有牙 ───────────────────────────────────────────────
    (
        "D1-say",
        "KR89D1",
        "只改**说法**不改行为：把「这台机没装 ccm」那一格从 StillFallsBack 写成 Closed",
        [
            (
                HIST,
                '            "这台机没装 ccm",\n            CellToday::StillFallsBack,',
                '            "这台机没装 ccm",\n            CellToday::Closed,',
                1,
            )
        ],
    ),
    (
        "D1-behave",
        "KR89D1",
        "只改**行为**不改说法（最小面）：让 Windows 那一支去读 tmux_name ⇒ 那一格不再是结构性",
        [(HIST, "        let _ = tmux_name;\n", "        let _kr89 = tmux_name;\n", 1)],
    ),
    # ── KR89D2：③ 本机继承那一格 ─────────────────────────────────────────────
    (
        "D2-reclose",
        "KR89D2",
        "把 ③ 退回短路（`None` 那一臂又 `Err`）—— 本件那一刀整个退掉",
        [
            (
                HIST,
                "        None => ci::CliAccount::Inherit,",
                '        None => return Err("本机未表态账号（继承环境）—— 变异".into()),',
                1,
            )
        ],
    ),
    (
        "D2-tobase",
        "KR89D2",
        "把「继承」偷换成「显式清空」：`Inherit` 渲染成 `--base`（#75 的病灶形状）",
        [
            (
                CCMI,
                "            CliAccount::Inherit => Some(vec![]),",
                '            CliAccount::Inherit => Some(vec!["--base".into()]),',
                1,
            )
        ],
    ),
    (
        "D2-zgate",
        "KR89D2",
        "绕过 `R08` 那道 `-z` 闸：`resolve_account` 不再尊重继承来的 CLAUDE_CONFIG_DIR",
        [
            (
                PLAN,
                "    if env\n        .inherited_config_dir\n        .as_deref()\n        .is_some_and(|v| !v.is_empty())\n    {",
                "    if false {",
                1,
            )
        ],
    ),
    # ── KR89D3：那条链上的消费者登记 ─────────────────────────────────────────
    (
        "D3-seat",
        "KR89D3",
        "拆掉 `remote-launch-run.ts` 那一处 `SESSION_BACKEND.attach` ⇒ 座的消费者少一处",
        [
            (
                RUN,
                'const attachCmd = SESSION_BACKEND.attach({ kind: "quoted", value: name });',
                "const attachCmd = `tmux attach -t ${JSON.stringify(`=${name}:`)}`;",
                1,
            )
        ],
    ),
    (
        "D3-fallback",
        "KR89D3",
        "拆掉生产主路那一处 `renderFallback(` ⇒ 「兜底渲染器仍是生产渲染器」那条依据没了",
        [
            (
                RUN,
                "  return renderFallback(plan);",
                '  return renderFallbackAlias(plan);',
                1,
            ),
            (
                RUN,
                'import { renderFallback } from "./launch-render-fallback";',
                'import { renderFallback as renderFallbackAlias } from "./launch-render-fallback";',
                1,
            ),
        ],
    ),
    # ── KR89D4：对拍不许变成自洽夹具 ─────────────────────────────────────────
    (
        "D4-selffix",
        "KR89D4",
        "把对拍的**左边**换成 Rust 自己算的那一份 ⇒ `got != want` 退化成 `x != x`",
        [
            (
                PARITY,
                "            let want = c.payload.clone();",
                "            let want_kr89 = c.payload.clone();",
                1,
            ),
            (
                PARITY,
                "            if got != want {",
                "            let want = got.clone();\n            let _ = want_kr89;\n            if got != want {",
                1,
            ),
        ],
    ),
    (
        "D4-rename",
        "KR89D4",
        "把整条对拍改名（= 悄悄把它从人群里拿走的最省事写法）",
        [
            (
                PARITY,
                "    fn rust_payload_rendering_matches_the_typescript_golden_byte_for_byte() {",
                "    fn rust_payload_rendering_matches_the_ts_golden_bytewise() {",
                1,
            )
        ],
    ),
    (
        "D4-unregister",
        "KR89D4",
        "只摘掉对拍那条的 `#[test]`（函数名原样留着）—— 判据只钉名字时它会活下来",
        [
            (
                PARITY,
                "    #[test]\n    fn rust_payload_rendering_matches_the_typescript_golden_byte_for_byte() {",
                "    #[allow(dead_code)]\n    fn rust_payload_rendering_matches_the_typescript_golden_byte_for_byte() {",
                1,
            )
        ],
    ),
    # ── 7u：把本件的实现整个退掉 ─────────────────────────────────────────────
    (
        "7u",
        "—",
        "把本件唯一的行为改动整个退掉（③ 回到短路 ＋ `Inherit` 那一臂拿掉）",
        [
            (
                HIST,
                "        None => ci::CliAccount::Inherit,",
                '        None => return Err("本机未表态账号（继承环境）—— 7u".into()),',
                1,
            )
        ],
    ),
]


def die(msg: str) -> None:
    print(f"❌ 拒跑（fail-closed）：{msg}", file=sys.stderr)
    sys.exit(3)


def by_name(name: str):
    for m in MUT:
        if m[0] == name:
            return m
    die(f"没有这一刀：{name}；有的是 {[m[0] for m in MUT]}")


def md5(t: str) -> str:
    return hashlib.md5(t.encode()).hexdigest()[:12]


def apply(name: str) -> None:
    if BACKUP.exists():
        die(f"备份目录已存在（{BACKUP}）—— 上一刀没还原干净，先 `restore`")
    _, dod, what, edits = by_name(name)
    # 先按文件聚合：同一份文件的多处编辑在**同一份内存文本**上顺序做完（`K-R104` 那个坑）。
    per_file: dict[str, list[tuple[str, str, int]]] = {}
    for rel, anchor, repl, expect in edits:
        per_file.setdefault(rel, []).append((anchor, repl, expect))
    staged: dict[str, tuple[str, str]] = {}
    for rel, es in per_file.items():
        p = ROOT / rel
        if not p.is_file():
            die(f"取不到被切的文件：{p}")
        orig = p.read_text()
        cur = orig
        for anchor, repl, expect in es:
            n = cur.count(anchor)
            if n != expect:
                die(
                    f"{rel}：锚点命中 {n} 次，期望 {expect} 次 —— 形状变了，一个字节都不落地。\n"
                    f"   锚点：{anchor[:80]!r}"
                )
            cur = cur.replace(anchor, repl)
        if cur == orig:
            die(f"{rel}：替换之后与原文逐字相同 —— 这一刀没有生效")
        staged[rel] = (orig, cur)
    BACKUP.mkdir(parents=True)
    for rel, (orig, cur) in staged.items():
        (BACKUP / rel.replace("/", "%")).write_text(orig)
        (ROOT / rel).write_text(cur)
    # 写盘之后**回读**逐处核对（不回读的话「落地了」只是一句自陈）。
    #
    # 🔴 **口径是「回读到的 == 我打算写的那份内存文本」，不是「锚点不见了」。**
    # 第一版写的是 `back.count(anchor) != 0 ⇒ die`，而**替换串里包含锚点**时
    # （`if got != want {` → `let want = got.clone(); … if got != want {`）那条恒真
    # ⇒ 一刀合法的变异被判成「后写覆盖了前写」。09-13 实打过一次：`D4-selffix` 当场拒跑，
    # 而它拒得**不对** —— 那不是覆盖，是我的核对口径把「包含」读成了「没生效」。
    # ⇒ 改成逐字节比对内存文本，`K-R104` 那个坑（后写覆盖前写）照样接得住：
    #   真被覆盖时回读到的必然不等于 `cur`。
    ok = []
    for rel, es in per_file.items():
        back = (ROOT / rel).read_text()
        if back != staged[rel][1]:
            die(f"{rel}：回读核对失败 —— 盘上那份不等于我打算写的那份（`K-R104` 那个坑）")
        for anchor, repl, expect in es:
            if repl not in back:
                die(f"{rel}：回读核对失败 —— 找不到落地后的串：{repl[:60]!r}")
            ok.append(f"{rel} ← {repl[:48]!r}")
    print(f"变异已落地：{name}（挂 {dod}）—— {what}")
    for line in ok:
        print(f"  · {line}")
    for rel, (orig, cur) in staged.items():
        print(f"  · {rel}  md5 {md5(orig)} → {md5(cur)}")


def restore() -> None:
    if not BACKUP.exists():
        die("没有备份目录 —— 没有可还原的刀")
    n = 0
    for f in sorted(BACKUP.iterdir()):
        rel = f.name.replace("%", "/")
        # 🔴 **重新写一遍原文**，不是 `cp -a`：mtime 必须变，否则 cargo 判「没变」。
        (ROOT / rel).write_text(f.read_text())
        print(f"已还原 {rel}  md5 {md5(f.read_text())}")
        n += 1
    shutil.rmtree(BACKUP)
    print(f"还原完成（{n} 份）")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        die("用法：list | apply <刀名> | restore")
    cmd = sys.argv[1]
    if cmd == "list":
        for name, dod, what, edits in MUT:
            print(f"{name:14s} {dod:8s} {what}  （{len(edits)} 处编辑）")
    elif cmd == "apply":
        if len(sys.argv) != 3:
            die("apply 要一个刀名")
        apply(sys.argv[2])
    elif cmd == "restore":
        restore()
    else:
        die(f"不认识的子命令：{cmd}")
