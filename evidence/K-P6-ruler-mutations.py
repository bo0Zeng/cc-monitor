#!/usr/bin/env python3
"""K-P6 量具 ④：**对前三把量具自己的死值验** —— 「更准」不是形容词，得有刀。

`brief` 第 7 条：每一刀变异先断言锚点恰好命中 N 次再改，并打印「变异已落地」。
`brief` 第 9 条：报「某一格有牙」时要说清那一刀的射程，并至少切一刀「只打该盖的最小面」。

`KP6D1` 的 acceptor 是 `实测`，而我给出的核心主张是
**「严格尺子比 PM 那把粗尺子准，准在标识符边界与词法状态两处」**。
这句话本身是一个读数 ⇒ 它也要被切一刀才算数。本量具切五刀。

## 台子怎么搭（刻意不在被测树上动手）

**不**在 `.claude/worktrees/k-p6` 上改任何源码。做法是：
把那 10 份含 `russh` 的文件拷进一个**一次性的临时 git 仓**（路径带本件件号，
`K-P6-ruler-mutations-<pid>`，`brief` 12「量具住址要能唯一定位到那一份」），
在**副本**上切刀。⚠ 副本是 `cp` 出来的新仓 + `git init`，**不是** `git worktree` 的拷贝
（`brief` 12c：工作树的 `.git` 是一行指回原仓的指针，在副本里跑 git 会写进**原树的暂存区**）。

## 五刀（每刀只打最小面，应当只动一个数）

| id | 切哪儿 | 锚点命中 | 应当 |
|---|---|---|---|
| R1 | 把 `sftp_pool.rs` 一处 `russh_sftp::protocol` 改成 `russh::protocol` | 1 | 严格 `russh` code **+1**、`russh_sftp` code **−1**；**粗尺子一动不动**（它本来就把这行算成 `russh` 代码行）⇒ 这一刀单独证明**标识符边界**那一格有牙 |
| R2 | 把 `ssh_source.rs` 的 `use russh::client;` 挪进一个字符串字面量 | 1 | 严格 code **−1**；**粗尺子一动不动** ⇒ 单独证明**字符串态**那一格有牙 |
| R3 | 把 `port_forward.rs:94` 那条**行尾注释**里的 `russh` 改成 `ssh` | 1 | **粗尺子代码行 −1**；严格 `russh` code **不变**（那一行本来就是 0）⇒ 单独证明**行尾注释**那一格正是两把尺子分岔的地方（PM 自己点名的那个粗口径） |
| R4 | 把 `mcp.rs` 的 `&russh_sftp::client::SftpSession` 整行改成注释 | 1 | 粗尺子代码行 **−1**、`russh_sftp` code **−1**；严格 `russh` **不变**（本来就是 0） |
| R5 | 在 `tmux.rs` 末尾加一句代码态 `russh::Disconnect` | 0→1 | 两把尺子**各 +1**（**反向控制**：证明两把都不是恒定值、没有空转 —— 上面四刀里有三刀是「某一把不动」，缺这一刀就分不清「不动」与「坏了」） |

⚠ **`strict_code` 与 `strict_code_lines` 在这几刀上必然同向同幅**（每刀只碰一行上的一处），
故期望表里两个都点名。**未点名的格子一格都不许动** —— 初版漏点了 `strict_code_lines`，
本量具当场把自己判红 4 刀；那正是这条规矩要买的东西，如实留在这里。

另加一刀给量具②（扼流点普查）：

| id | 切哪儿 | 应当 |
|---|---|---|
| R6 | 在 `tmux.rs` 里加一处**注释态**的 `connect_and_exec_cmd(` ＋ 一处**代码态**的 | 注释那处**数不到**，代码那处 18→19 |

⚠ **本量具不改被测树**。跑完它，`git -C <被测树> status --porcelain` 应当逐字不变。
"""

from __future__ import annotations

import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve()
TREE = HERE.parent.parent

RULER = HERE.parent / "K-P6-russh-ruler.py"
CENSUS = HERE.parent / "K-P6-dial-census.py"

FILES = [
    "account_usage.rs",
    "launch.rs",
    "mcp.rs",
    "port_forward.rs",
    "remote_write_registry.rs",
    "sftp_pool.rs",
    "sftp.rs",
    "ssh_source.rs",
    "structural_scan.rs",
    "tmux.rs",
]


def build_fixture(dst: pathlib.Path) -> None:
    (dst / "src-tauri" / "src").mkdir(parents=True)
    for f in FILES:
        shutil.copy2(TREE / "src-tauri" / "src" / f, dst / "src-tauri" / "src" / f)
    env = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
    run = lambda *a: subprocess.run(
        ["git", "-C", str(dst), *a], check=True, capture_output=True, env=env
    )
    run("init", "-q")
    run("config", "user.email", "kp6@example.invalid")
    run("config", "user.name", "kp6")
    run("add", "src-tauri/src")
    run("commit", "-qm", "fixture")


def measure(tree: pathlib.Path) -> dict:
    out = subprocess.run(
        [sys.executable, str(RULER), "--tree", str(tree), "--json"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    d = json.loads(out)
    return {
        "strict_code": d["totals_russh_by_state"]["code"],
        "strict_code_lines": d["strict_russh_code_line_sum"],
        "coarse_code_lines": d["coarse_code_line_sum"],
        "sftp_code": d["totals_russh_sftp_by_state"]["code"],
    }


def census(tree: pathlib.Path) -> int:
    out = subprocess.run(
        [sys.executable, str(CENSUS), "--tree", str(tree), "--json"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    d = json.loads(out)
    return sum(1 for r in d["found"]["connect_and_exec_cmd"] if r[2] == "调用")


def anchor_count(p: pathlib.Path, needle: str) -> int:
    return p.read_text("utf-8", "surrogateescape").count(needle)


def cut(p: pathlib.Path, old: str, new: str, want_hits: int) -> str:
    src = p.read_text("utf-8", "surrogateescape")
    got = src.count(old)
    assert got == want_hits, f"锚点 {old!r} 在 {p.name} 命中 {got} 次，期望 {want_hits} 次 —— 这一刀作废"
    p.write_text(src.replace(old, new, 1), "utf-8", errors="surrogateescape")
    return f"变异已落地（锚点命中 {got}，切第 1 处）"


def append(p: pathlib.Path, text: str) -> str:
    with p.open("a", encoding="utf-8", errors="surrogateescape") as fh:
        fh.write(text)
    return "变异已落地（追加，锚点 0 → 1）"


CUTS = [
    (
        "R1",
        "sftp_pool.rs",
        "改一处 `russh_sftp::protocol` → `russh::protocol`",
        lambda d: cut(d / "sftp_pool.rs", "russh_sftp::protocol::OpenFlags::READ", "russh::protocol::OpenFlags::READ", 1),
        {"strict_code": +1, "strict_code_lines": +1, "sftp_code": -1, "coarse_code_lines": 0},
    ),
    (
        "R2",
        "ssh_source.rs",
        "把 `use russh::client;` 挪进字符串字面量",
        lambda d: cut(d / "ssh_source.rs", "use russh::client;", 'const _KP6_R2: &str = "use russh::client;";', 1),
        {"strict_code": -1, "strict_code_lines": -1, "coarse_code_lines": 0},
    ),
    (
        "R3",
        "port_forward.rs",
        "把 `:94` 那条**行尾注释**里的 `russh` 改成 `ssh`（只碰注释，一个代码字节没动）",
        lambda d: cut(
            d / "port_forward.rs",
            "let session = Arc::new(session); // russh Handle 不 Clone → Arc 共享",
            "let session = Arc::new(session); // ssh Handle 不 Clone → Arc 共享",
            1,
        ),
        {"coarse_code_lines": -1},
    ),
    (
        "R4",
        "mcp.rs",
        "把 `&russh_sftp::client::SftpSession` 那一行整行注释掉",
        lambda d: cut(d / "mcp.rs", "    sftp: &russh_sftp::client::SftpSession,", "    // sftp: &russh_sftp::client::SftpSession,", 1),
        {"coarse_code_lines": -1, "strict_code": 0, "sftp_code": -1},
    ),
    (
        "R5",
        "tmux.rs",
        "末尾加一句代码态 `russh::Disconnect`（本来 0 处代码态）—— 反向控制",
        lambda d: append(d / "tmux.rs", "\nconst _KP6_R5: Option<russh::Disconnect> = None;\n"),
        {"strict_code": +1, "strict_code_lines": +1, "coarse_code_lines": +1},
    ),
]


def main() -> int:
    base_dir = pathlib.Path(tempfile.gettempdir()) / f"K-P6-ruler-mutations-{os.getpid()}"
    base_dir.mkdir(parents=True, exist_ok=True)
    w = 96
    print("=" * w)
    print(f"K-P6 · 量具死值验    台子={base_dir}")
    print(f"                     被测源 = {TREE}（**只读**，本量具不改它一个字节）")
    print("=" * w)

    baseline_dir = base_dir / "v0"
    build_fixture(baseline_dir)
    base = measure(baseline_dir)
    base_census = census(baseline_dir)
    print(
        f"【基线】（台子上那 10 份文件）严格 code={base['strict_code']} · "
        f"严格代码行={base['strict_code_lines']} · 粗代码行={base['coarse_code_lines']} · "
        f"russh_sftp code={base['sftp_code']} · connect_and_exec_cmd 调用={base_census}"
    )
    print(
        "  ⚠ 台子只装 10 份文件 ⇒ `connect_and_exec_cmd` 那个数**小于**全树的 18，"
        "这是分母不同，不是漂移。"
    )
    print("-" * w)

    bad = 0
    for cid, fname, what, do, want in CUTS:
        # brief 12c：**每一版一份新副本**，别在同一份上叠刀
        d = base_dir / cid
        build_fixture(d)
        note = do(d / "src-tauri" / "src")
        got = measure(d)
        delta = {k: got[k] - base[k] for k in base}
        ok = all(delta.get(k, 0) == v for k, v in want.items())
        # 没点名的格子必须**不动**
        untouched = {k: v for k, v in delta.items() if k not in want and v != 0}
        ok = ok and not untouched
        bad += 0 if ok else 1
        print(f"[{cid}] {fname} —— {what}")
        print(f"      {note}")
        print(
            f"      实测 Δ：严格code {delta['strict_code']:+d} · 严格代码行 {delta['strict_code_lines']:+d} · "
            f"粗代码行 {delta['coarse_code_lines']:+d} · sftp_code {delta['sftp_code']:+d}"
        )
        print(f"      期望 Δ：{want}")
        print(f"      ⇒ {'✅ 只动了该动的那一格' if ok else '❌ 不符'}"
              + (f"  未点名却动了：{untouched}" if untouched else ""))
        print("-" * w)

    # R6：量具② 的刀
    d6 = base_dir / "R6"
    build_fixture(d6)
    t = d6 / "src-tauri" / "src" / "tmux.rs"
    with t.open("a", encoding="utf-8", errors="surrogateescape") as fh:
        fh.write(
            "\n// 注释态：ssh_source::connect_and_exec_cmd(&cfg, &cmd) —— 本行不该被数到\n"
            "async fn _kp6_r6(cfg: &ssh_source::RemoteConfig, cmd: &str) {\n"
            "    let _ = ssh_source::connect_and_exec_cmd(cfg, cmd).await;\n"
            "}\n"
        )
    got6 = census(d6)
    ok6 = got6 == base_census + 1
    bad += 0 if ok6 else 1
    print("[R6] tmux.rs —— 同时加一处**注释态**与一处**代码态** `connect_and_exec_cmd(`")
    print("      变异已落地（追加 2 处字面命中，其中 1 处在注释里）")
    print(f"      实测：调用数 {base_census} → {got6}（Δ{got6 - base_census:+d}）；期望 Δ+1")
    print(f"      ⇒ {'✅ 注释那处确实没被数到' if ok6 else '❌ 不符'}")
    print("=" * w)
    print(f"六刀里不符 {bad} 刀")
    print(f"⚠ 台子留在 {base_dir}，跑完自己删：`rm -rf {base_dir}`")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
