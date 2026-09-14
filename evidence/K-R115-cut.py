#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R115 死值验的刀 —— 每一刀先断言锚点命中数，再落刀，再印「变异已落地」。

## 纪律（照 `references/brief.md` 第 7 · 11 · 12c 条）

- 🔴 **还原不许 `copy2` / `cp -a`**：本脚本的 `--revert` 是**重写原文**
  （`write_text`），mtime 必变 ⇒ `cargo` 一定重编。
  ⚠ 这条纪律本身就是 `KR115D1` 的题面 —— 本文件自己不许犯它，
  而且它现在**有机器在拦**了（`evidence/K-R115-ruler.py`，门禁 `copy2` 那一格）。
- 每一刀落刀前 `assert 锚点命中 == want`，对不上**一个字节都不改**、整趟放弃。
- 一趟只许有一把刀在盘上：`--apply` 之前若备份还在，拒绝落刀。

## 刀

    d1-1   `KR115D1` ①  —— 把 `kg3-c1-cuts.py` 的**还原那一跳**改回 `copy2`
    d1-2   `KR115D1` ②  —— 阴性对照：d1-1 ＋ 把门禁那一格整格拿掉
    d1-3   `KR115D1` ③  —— 假红方向：新增一处**正当用途**的 `copy2`（拷读数文件进临时目录）
    d2-1   `KR115D2` ①  —— 造一处 `dead_code`（monitor 生产段加一个没人调的 fn）
    d2-2   `KR115D2` ②  —— 加了格而**自述格数不跟**（15 → 13）
    d3-1   `KR115D3` ①  —— 只改共用段的**其中一处**
    d3-2   `KR115D3` ②  —— 阴性对照：d3-1 ＋ 把同步判据整段拿掉
    d4-1   `KR115D4` ①  —— 把一行 `Side::Remote` 改成 `Side::Both`（签过字那一栏里的一行）
    d4-1b  `KR115D4` ①b —— 换一行改（`RemoteOnly` 那一档，验直方图两侧都拦得住）
    d4-2   `KR115D4` ②  —— 阴性对照：d4-1 ＋ 把本笔判据整段拿掉
    d4-3   `KR115D4` ①c —— **派发跳换档而 `Side` 不动**（本笔真正那颗牙）
    d4-4   `KR115D4` ②c —— 阴性对照：d4-3 ＋ 把本笔判据整段拿掉

## 跑法

    python3 evidence/K-R115-cut.py --list
    python3 evidence/K-R115-cut.py --apply <刀名>
    python3 evidence/K-R115-cut.py --revert
"""

import json
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / ".k-r115-cut-backup.json"

RULER = "evidence/K-R115-ruler.py"
GATE = "scripts/gate.sh"
KG3 = "evidence/kg3-c1-cuts.py"
GUARD = "remote-daemon-proto/src/guard_support.rs"
SCAN = "src-tauri/src/structural_scan.rs"
LEDGER = "src-tauri/src/parity_ledger.rs"
PATHS = "src-tauri/src/paths.rs"
HOOKSD = "src-tauri/src/hooks_diag.rs"
PROBE = "evidence/kr115-probe-reading-copy.py"

# 正当用途的 `copy2`：把一份**读数文件**拷进临时目录。目的地在树外 ⇒ 判据必须**不红**。
PROBE_SRC = '''#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""一处**正当用途**的 `shutil.copy2`：把一份读数文件拷进临时目录再读。

`KR115D1` 死值验第 ③ 刀（假红方向）用它：判据判的是「还原被测源码那一跳」，
不是「源码里有没有 copy2 这个词」⇒ 本文件必须**不红**。
"""

import shutil
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main() -> int:
    reading = HERE / "K-R115-deathvalue.md"
    box = Path(tempfile.mkdtemp(prefix="kr115probe-"))
    shutil.copy2(reading, box / "reading.md")
    print((box / "reading.md").stat().st_size)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
'''

DEAD_FN = '''
fn kr115_probe_that_nobody_calls(x: usize) -> usize {
    x + 1
}
'''


def sub(path, old, new, want=1):
    """文本替换刀：锚点必须恰好命中 `want` 次。"""
    def run(files):
        txt = files[path]
        n = txt.count(old)
        if n != want:
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里锚点命中 {n} 次，应当 {want} 次 —— 一个字节都不改")
        files[path] = txt.replace(old, new)
        return f"{path}：锚点命中 {n} 次（应 {want}）· 替换 1 处"
    return run


def append(path, text):
    def run(files):
        files[path] = files[path] + text
        return f"{path}：在文件末尾追加 {len(text)} B"
    return run


def create(path, text):
    def run(files):
        files[path] = text
        return f"{path}：**新建**（{len(text)} B）"
    return run


def drop_fn(path, name):
    """把一个 `#[test] fn <name>` 连同它头上的 `///` 文档整段删掉。

    收尾判据是**列 4 的 `    }`** —— 与本仓「列 0 收尾」那一族同一条口径，
    只是这几个 fn 住在 `mod tests` 里、缩进一级。
    """
    def run(files):
        lines = files[path].split("\n")
        decl = [i for i, l in enumerate(lines) if l.startswith(f"    fn {name}(")]
        if len(decl) != 1:
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里 `fn {name}(` 命中 {len(decl)} 次，应当 1 次")
        i = decl[0]
        while i > 0 and (lines[i - 1].lstrip().startswith("///")
                         or lines[i - 1].lstrip().startswith("#[")
                         or lines[i - 1].lstrip().startswith("//")):
            i -= 1
        j = decl[0]
        while j < len(lines) and lines[j] != "    }":
            j += 1
        if j >= len(lines):
            raise SystemExit(f"🔴 拒绝落刀：`{path}` 里 `fn {name}` 找不到列 4 的收尾 `}}`")
        cut = j - i + 1
        files[path] = "\n".join(lines[:i] + lines[j + 1:])
        return f"{path}：`fn {name}` 连头注整段删掉 {cut} 行"
    return run


def compose(*steps):
    def run(files):
        return " ｜ ".join(s(files) for s in steps)
    return run


# ── 刀表 ────────────────────────────────────────────────────────────────────
COPY2_BACK = sub(
    KG3,
    """        shutil.copyfile(keep, GATE)
        os.utime(GATE, None)""",
    """        shutil.copy2(keep, GATE)""",
)
DROP_CELL = sub(
    GATE,
    """run_gate copy2 '`evidence/*.py` 现打""",
    """: skip-cell '`evidence/*.py` 现打""",
)
FLIP_ONE_NOTE = sub(
    SCAN,
    "    /// ① `K-R110`（09-13）现打它**当时就没红** —— 那 31 行漏进生产段，而守门人看不见；",
    "    /// ① `K-R110`（09-13）现打它当时没红 —— 那 31 行漏进生产段，而守门人看不见；",
)
FLIP_SIDE = sub(
    LEDGER,
    '        ("capture_remote_pane", "tmux.manage", Side::Remote),',
    '        ("capture_remote_pane", "tmux.manage", Side::Both),',
)
FLIP_SIDE_B = sub(
    LEDGER,
    '        ("list_remote_tmux", "tmux.manage", Side::Remote),',
    '        ("list_remote_tmux", "tmux.manage", Side::Both),',
)
SELF_COUNT_BACK = sub(GATE, "# │ 〔自述·格数〕15 格", "# │ 〔自述·格数〕13 格")
# 🔴 `KR115D4` 真正的那颗牙：**派发跳换了档，而 `Side` 一个字不动**。
#    这一形就是 `K-R112` 撞见的那个病（改了行为不回来改这一行），
#    而 `d4-1` / `d4-1b` 那两刀**证不了它是本笔的牙** —— 现打：把 `Side` 改成 `Both`
#    时，本仓早就有的 `local_or_both_commands_take_no_remote_only_parameter`
#    也会红（那两条命令都吃 `origin:`）。本刀绕开它：`Side` 那一栏一个字节都不改。
DISPATCH_FLIP = sub(
    HOOKSD,
    """pub async fn diagnose_remote_cc_bus_hooks(origin: String) -> Result<HooksReport, String> {
    use tokio::io::AsyncReadExt;""",
    """pub async fn diagnose_remote_cc_bus_hooks(origin: String) -> Result<HooksReport, String> {
    use tokio::io::AsyncReadExt;
    let _ = crate::inbound_client::client_for(&origin);""",
)

CUTS = {
    "d1-1": ("KR115D1 ① 还原那一跳改回 copy2 ⇒ 门禁 copy2 那一格必须红", COPY2_BACK),
    "d1-2": ("KR115D1 ② 阴性对照：d1-1 ＋ 门禁那一格整格拿掉 ⇒ 一条都不红",
             compose(COPY2_BACK, DROP_CELL)),
    "d1-3": ("KR115D1 ③ 假红方向：新增一处正当用途的 copy2 ⇒ 必须不红", create(PROBE, PROBE_SRC)),
    "d2-1": ("KR115D2 ① 造一处 dead_code ⇒ deadcode 那一格必须红", append(PATHS, DEAD_FN)),
    "d2-2": ("KR115D2 ② 自述格数不跟（15 → 13）⇒ K-R80 那把尺子必须红", SELF_COUNT_BACK),
    "d3-1": ("KR115D3 ① 只改共用段的其中一处 ⇒ 同步判据必须红", FLIP_ONE_NOTE),
    "d3-2": ("KR115D3 ② 阴性对照：d3-1 ＋ 同步判据整段拿掉 ⇒ 不红",
             compose(FLIP_ONE_NOTE, drop_fn(GUARD, "the_two_strip_clean_notes_stay_one_sentence"))),
    "d4-1": ("KR115D4 ① 把一行 Side::Remote 改成 Both（候选那一档）⇒ 必须红", FLIP_SIDE),
    "d4-1b": ("KR115D4 ①b 换一行改（RemoteOnly 那一档）⇒ 必须红", FLIP_SIDE_B),
    "d4-2": ("KR115D4 ② 阴性对照：d4-1 ＋ 本笔判据整段拿掉 ⇒ 不红",
             compose(FLIP_SIDE, drop_fn(LEDGER, "the_remote_side_column_is_signed_off"))),
    "d4-3": ("KR115D4 ①c 派发跳换档而 `Side` 不动 ⇒ **只有本笔那条判据**红", DISPATCH_FLIP),
    "d4-4": ("KR115D4 ②c 阴性对照：d4-3 ＋ 本笔判据整段拿掉 ⇒ 不红",
             compose(DISPATCH_FLIP, drop_fn(LEDGER, "the_remote_side_column_is_signed_off"))),
}


def do_apply(name: str) -> int:
    if BACKUP.exists():
        print(f"🔴 拒绝落刀：`{BACKUP.name}` 还在 —— 上一刀没还原。先 `--revert`。")
        return 3
    if name not in CUTS:
        print(f"🔴 没有这把刀：{name}")
        return 2
    why, step = CUTS[name]
    touched = [KG3, GATE, SCAN, GUARD, LEDGER, PATHS, HOOKSD]
    files = {f: (ROOT / f).read_text(encoding="utf-8") for f in touched}
    before = dict(files)
    note = step(files)
    changed = {f: t for f, t in files.items() if before.get(f) != t}
    if not changed:
        print("🔴 一处都没改到 —— 刀空转了，不许当成落地")
        return 3
    # 备份：只记**被改到**的那几份的原文 ＋ 新建了哪几份
    created = [f for f in changed if f not in before]
    BACKUP.write_text(
        json.dumps({"cut": name,
                    "orig": {f: before[f] for f in changed if f in before},
                    "created": created},
                   ensure_ascii=False),
        encoding="utf-8",
    )
    for f, t in changed.items():
        (ROOT / f).write_text(t, encoding="utf-8")
        os.utime(ROOT / f, None)          # 🔴 mtime 必须是现在（纪律 ㉒）
    print(f"变异已落地：{name} —— {why}")
    print(f"  {note}")
    print(f"  动到的文件（{len(changed)} 份）：{sorted(changed)}")
    return 0


def do_revert() -> int:
    if not BACKUP.exists():
        print("🔴 没有备份可还原 —— 盘上要么本来就是干净的，要么有人手改过。自己核。")
        return 3
    d = json.loads(BACKUP.read_text(encoding="utf-8"))
    for f, t in d["orig"].items():
        # 🔴 **重写原文**，不是 `copy2` / `cp -a`：mtime 必变 ⇒ cargo 一定重编。
        (ROOT / f).write_text(t, encoding="utf-8")
        os.utime(ROOT / f, None)
        print(f"已还原：{f}（重写原文，mtime 已推到现在）")
    for f in d.get("created", []):
        p = ROOT / f
        if p.exists():
            p.unlink()
            print(f"已删掉：{f}（本刀新建的）")
    BACKUP.unlink()
    print(f"（这一趟退掉的是：{d['cut']}）")
    return 0


def main() -> int:
    # 🔴 占位：本文件**刻意不用** `shutil` 的复制族 —— `--revert` 是重写原文
    #    （`write_text` ＋ `os.utime`），mtime 必变。同 `K-R106-cut.py` 那一行的先例。
    _ = shutil
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    a = sys.argv[1]
    if a == "--list":
        for k, (why, _) in CUTS.items():
            print(f"{k:7s} {why}")
        return 0
    if a == "--revert":
        return do_revert()
    if a == "--apply" and len(sys.argv) == 3:
        return do_apply(sys.argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
