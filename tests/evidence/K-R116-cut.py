#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R116 死值验的刀 —— 每一刀先断言锚点命中数，再落刀，再印「变异已落地」。

## 纪律（照 `references/brief.md` 第 7 · 11 · 12c 条）

- 🔴 **还原不许 `copy2` / `cp -a`**：`--revert` 是**重写原文**（`write_text`），
  mtime 必变 ⇒ `cargo` 一定重编，读到的不是上一刀的回声。
  本文件**一处 `shutil` 都没有**（`K-R115` 立的 `copy2` 那一格盯着 `evidence/*.py`）。
- 每一刀落刀前 `assert 锚点命中 == want`，对不上**一个字节都不改**、整趟放弃。
- 一趟只许一把刀在盘上：`--apply` 之前若备份还在，拒绝落刀。

## 刀

    d1-1   `KR116D1` ①  —— 把**判据**的 token 规则里 `-` 那一档掐掉
                           ⇒ `remote-daemon-proto` 这类标识符被误算进「该改」⇒ 闸必须红并点名
    d1-1b  `KR116D1` ①b —— 同一刀切在**量具**那一侧（`evidence/K-R116-ruler.py`）
                           ⇒ 分档读数必须变（`标识符·不改` 掉、`该改` 涨）
    d1-2   `KR116D1` ②  —— 阴性对照：d1-1 ＋ 把那条判据整条拿掉 ⇒ 不许红
    d1-3   `KR116D1` ③  —— 假红方向：往写区文档里加一处**正当**的代码标识符提及 ⇒ 必须不红
    d2-1   `KR116D2` ①  —— 在 `evidence/` 一份留档里把 `daemon` **整份批量替换**掉 ⇒ 必须红
    d2-1s  `KR116D2` ①s —— 最小面：同一份里**只换一处**（43 → 42）⇒ 仍必须红
    d2-2   `KR116D2` ②  —— 阴性对照：d2-1 ＋ 把那条判据整条拿掉 ⇒ 不许红
    d3-1   `KR116D3` ①  —— 把 `doc/INVARIANTS.md` 那句改回旧措辞
                           ⇒ 引它的 `sftp_move_ledger` 那条必须红
    d3-1m  `KR116D3` ①m —— 最小面版：d3-1 ＋ 同拍把那句登进 `EXEMPT`
                           ⇒ 措辞闸不再红，只剩引文那条红（证明 d3-1 的红分得开两个来源）
    d3-2   `KR116D3` ②  —— 把 `doc/IPC-PROTOCOL.md` §10 的标题改回旧措辞
                           ⇒ 引它的 `protocol_doc_guard` 那条必须红

## 跑法

    python3 evidence/K-R116-cut.py --list
    python3 evidence/K-R116-cut.py --apply <刀名>
    python3 evidence/K-R116-cut.py --revert
    python3 evidence/K-R116-cut.py --backtest     # KR116D1 ② 的阳性回测（不落刀）

⚠ 量具住址：本文件只属于 `K-R116`，被测对象是**它自己所在的那棵工作树**
（`Path(__file__).resolve().parents[1]`，跑的时候印出来）—— 不指别的树。
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / ".k-r116-cut-backup.json"

JUDGE = "src-tauri/src/doc_claim_registry.rs"
RULER = "evidence/K-R116-ruler.py"
INVARIANTS = "doc/INVARIANTS.md"
IPC = "doc/IPC-PROTOCOL.md"
ARCH = "doc/ARCHITECTURE.md"
FROZEN = "evidence/K-R113-deathvalue.md"


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def write(rel: str, text: str) -> None:
    # 🔴 重写原文 ⇒ mtime 必变。刻意不走 `shutil` 那一族（`K-R115` 的纪律）。
    (ROOT / rel).write_text(text, encoding="utf-8")


class Cutter:
    def __init__(self) -> None:
        self.orig: dict[str, str] = {}

    def patch(self, rel: str, old: str, new: str, want: int = 1) -> None:
        text = read(rel)
        got = text.count(old)
        assert got == want, f"锚点在 {rel} 命中 {got} 次，期望 {want} —— 一个字节都不改：{old[:70]!r}"
        self.orig.setdefault(rel, text)
        write(rel, text.replace(old, new))
        print(f"  变异已落地：{rel}  锚点命中 {got}  ← {old[:56]!r}")

    def cut_block(self, rel: str, start: str, end_marker: str, why: str, keep: str = "") -> None:
        """把 `start` 那一行到它之后第一处 `end_marker` 的整块删掉（拿掉一条判据）。

        ⚠ `keep` 是**补回去**的那一小截：判据是 `mod` 里最后一个 `fn`，
        它的收尾花括号与 `mod` 的收尾花括号连在一起 —— 整块删掉会把 `mod` 也拆了，
        那时红的是**编译**，不是「判据不在了」，阴性对照当场作废。
        〔09-14 实打过一次：`d1-2` 头一趟四格齐红（fmt/winchk/cargo/deadcode），
        那不是读数，是 CRASH。〕
        """
        text = read(rel)
        assert text.count(start) == 1, f"块首在 {rel} 命中 {text.count(start)} 次，期望 1"
        a = text.index(start)
        b = text.index(end_marker, a) + len(end_marker)
        self.orig.setdefault(rel, text)
        write(rel, text[:a] + keep + text[b:])
        print(f"  变异已落地：{rel}  拿掉 {b - a} 字节（{why}），补回 {len(keep)} 字节")

    def append(self, rel: str, tail: str) -> None:
        text = read(rel)
        self.orig.setdefault(rel, text)
        write(rel, text + tail)
        print(f"  变异已落地：{rel}  追加 {len(tail.encode())} 字节")

    def save(self, name: str) -> None:
        BACKUP.write_text(
            json.dumps({"cut": name, "files": self.orig}, ensure_ascii=False), encoding="utf-8"
        )
        print(f"  备份已落：{BACKUP.name}（{len(self.orig)} 份）")


# ── token 规则那一刀：把 `-` 从「可以吃进 token」的字符里拿掉 ──────────────────
#
# 拿掉之后 `remote-daemon-proto` 会被切成三段，中间那段是裸 `daemon`
# ⇒ 它从「标识符·不改」掉进「该改」。**这一刀证明的正是「尺子分得开两者」。**
JUDGE_TOKEN_OLD = """        while a > 0
            && (is_ident(s[a - 1])
                || (matches!(s[a - 1], b'-' | b'.' | b'/' | b':') && a >= 2 && is_ident(s[a - 2])))
        {
            a -= 1;
        }
        while b < s.len()
            && (is_ident(s[b])
                || (matches!(s[b], b'-' | b'.' | b'/' | b':')
                    && b + 1 < s.len()
                    && is_ident(s[b + 1])))
        {
            b += 1;
        }"""
JUDGE_TOKEN_NEW = """        while a > 0
            && (is_ident(s[a - 1])
                || (matches!(s[a - 1], b'.' | b'/' | b':') && a >= 2 && is_ident(s[a - 2])))
        {
            a -= 1;
        }
        while b < s.len()
            && (is_ident(s[b])
                || (matches!(s[b], b'.' | b'/' | b':')
                    && b + 1 < s.len()
                    && is_ident(s[b + 1])))
        {
            b += 1;
        }"""

RULER_TOKEN_OLD = """    while a > 0 and (_idc(s[a - 1]) or (s[a - 1] in "-./:" and a - 1 > 0 and _idc(s[a - 2]))):
        a -= 1
    while b < len(s) and (_idc(s[b]) or (s[b] in "-./:" and b + 1 < len(s) and _idc(s[b + 1]))):
        b += 1"""
RULER_TOKEN_NEW = """    while a > 0 and (_idc(s[a - 1]) or (s[a - 1] in "./:" and a - 1 > 0 and _idc(s[a - 2]))):
        a -= 1
    while b < len(s) and (_idc(s[b]) or (s[b] in "./:" and b + 1 < len(s) and _idc(s[b + 1]))):
        b += 1"""

WORDING_TEST_HEAD = "    /// ★★ 正题：**那 9 份散文里不许再有人读的 `daemon`**。\n    #[test]\n    fn no_prose_in_the_wording_sites_still_says_daemon() {"
FROZEN_TEST_HEAD = "    /// ★★ 正题：登记过的每一份，两个词的处数都不许掉。\n    #[test]\n    fn the_frozen_history_never_loses_a_daemon() {"
TEST_END = "\n    }\n}"

PIN_NEW = "**现措辞**：**后端不许改动用户既有数据"
PIN_OLD = "**现措辞**：**daemon 不许改动用户既有数据"
HEAD_NEW = "## 10. 远端后端 wire 协议"
HEAD_OLD = "## 10. 远端 daemon wire 协议"


def cut(name: str) -> None:
    c = Cutter()
    if name in ("d1-1", "d1-2"):
        c.patch(JUDGE, JUDGE_TOKEN_OLD, JUDGE_TOKEN_NEW)
    if name == "d1-1b":
        c.patch(RULER, RULER_TOKEN_OLD, RULER_TOKEN_NEW)
    if name == "d1-2":
        c.cut_block(JUDGE, WORDING_TEST_HEAD, TEST_END, "把措辞闸整条拿掉", keep="}")
    if name == "d1-3":
        c.append(
            ARCH,
            "\n<!-- KR116 假红探针：一处**正当**的代码标识符提及，本闸不许因它红 -->\n"
            "路由那一层的落点是 `remote-daemon-proto/src/relay/mod.rs`，"
            "起进程那条走 `daemon_launch.rs`，探活子命令是 `--daemon-probe`。\n",
        )
    if name in ("d2-1", "d2-2"):
        # 「批量替换」那一形：整份里 43 处一次换光。
        text = read(FROZEN)
        m = re.search(r"(?i)daemon", text)
        assert m, f"{FROZEN} 里一处 daemon 都没有 —— 换一份留档"
        c.patch(FROZEN, text[m.start() : m.end()], "后端", want=text.count(text[m.start() : m.end()]))
    if name == "d2-1s":
        # 🔴 最小面：**只换一处**。地板是逐文件的 ⇒ 43 → 42 也必须红。
        text = read(FROZEN)
        i = re.search(r"(?i)daemon", text).start()
        c.orig.setdefault(FROZEN, text)
        write(FROZEN, text[:i] + "后端" + text[i + 6 :])
        print(f"  变异已落地：{FROZEN}  只换 1 处（字节偏移 {i}），其余 42 处不动")
    if name == "d2-2":
        c.cut_block(JUDGE, FROZEN_TEST_HEAD, TEST_END, "把冻结面那道闸整条拿掉", keep="}")
    if name in ("d3-1", "d3-1m"):
        c.patch(INVARIANTS, PIN_NEW, PIN_OLD)
    if name == "d3-1m":
        c.patch(
            JUDGE,
            '    const EXEMPT: &[(&str, &str, &str)] = &[\n',
            '    const EXEMPT: &[(&str, &str, &str)] = &[\n'
            '        ("doc/INVARIANTS.md", "**现措辞**：**daemon 不许改动用户既有数据",\n'
            '         "d3-1m 这一刀临时登记：把射程收成「只让引文那条判据说话」"),\n',
        )
        c.patch(JUDGE, "const EXEMPT_HITS: usize = 15;", "const EXEMPT_HITS: usize = 16;")
    if name == "d3-2":
        c.patch(IPC, HEAD_NEW, HEAD_OLD)
    if not c.orig:
        raise SystemExit(f"🔴 没有这把刀：{name}")
    c.save(name)


def backtest() -> None:
    """`KR116D1` ② 的阳性回测：`doc/IPC-PROTOCOL.md` 那 159 处必须真的被尺子看见。"""
    sys.path.insert(0, str(ROOT / "evidence"))
    import importlib.util

    spec = importlib.util.spec_from_file_location("kr116_ruler", ROOT / RULER)
    r = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(r)
    before = subprocess.run(
        ["git", "show", f"897afec:{IPC}"], capture_output=True, cwd=ROOT, check=True
    ).stdout.decode()
    rows = r.classify(IPC, before)
    tally: dict[str, int] = {}
    for _, _, _, v, _ in rows:
        tally[v] = tally.get(v, 0) + 1
    print(f"· 被测树   ：{ROOT}")
    print(f"· 语料     ：`git show 897afec:{IPC}`（**改之前**那一份，{len(before)} 字节）")
    print(f"· 出现次数 ：{len(rows)}")
    for v in r.VERDICTS:
        print(f"     {v:<14} {tally.get(v, 0)}")
    assert len(rows) == 159, f"出现次数 {len(rows)} ≠ 159 —— 尺子没跑到那份文件上"
    assert tally.get("冻结·不许改", 0) == 0 and tally.get("写区外·未改", 0) == 0, (
        "那 159 处里有落进「冻结」或「写区外」的 —— 这份文件没进本轮人群，尺子没跑"
    )
    print("阳性回测：OK —— 159/159 全部落在**本轮写区**这一档里（0 冻结 · 0 写区外）")
    print(
        f"⚠ 逐处再分一层：{tally.get('该改', 0)} 处该改 · "
        f"{tally.get('标识符·不改', 0)} 处代码标识符 · {tally.get('登记例外·不改', 0)} 处登记例外 —— "
        "「159 处落进该改档」按**文件级**成立，按**逐处**不成立，两个口径别混"
    )


def revert() -> None:
    if not BACKUP.is_file():
        raise SystemExit("没有备份 —— 盘上没有刀")
    data = json.loads(BACKUP.read_text(encoding="utf-8"))
    for rel, text in data["files"].items():
        write(rel, text)
        print(f"  已还原：{rel}")
    BACKUP.unlink()
    print(f"刀 {data['cut']} 已撤（重写原文，mtime 必变 ⇒ cargo 一定重编）")


def main() -> int:
    ap = __import__("argparse").ArgumentParser()
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--apply")
    ap.add_argument("--revert", action="store_true")
    ap.add_argument("--backtest", action="store_true")
    a = ap.parse_args()
    print(f"[K-R116-cut] 被测树 = {ROOT}")
    if a.list:
        print(__doc__.split("## 刀")[1].split("## 跑法")[0])
        return 0
    if a.revert:
        revert()
        return 0
    if a.backtest:
        backtest()
        return 0
    if a.apply:
        if BACKUP.is_file():
            raise SystemExit(f"🔴 盘上已经有一把刀（{BACKUP.name}）—— 先 --revert")
        cut(a.apply)
        return 0
    ap.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
