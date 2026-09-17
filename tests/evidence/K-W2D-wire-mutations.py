#!/usr/bin/env python3
"""K-W2D 接线那一拍的死值验台（`W1`–`W17` ＋ 全断刀 `W18`）。

口径（一次写清，别在别处再写第二份）
------------------------------------
* **被测对象是工作树的副本**，不是工作树本身：
  `.claude/pm-targets/k-w2d-wire-mut/repo`（`.git` / `node_modules` / `target` / `dist` 不拷）。
  拷之前删掉副本里的 `.git` —— 工作树的 `.git` 是一行指回原仓的指针，
  在副本里跑 `git` 会写进**原树的暂存区**。本台一次 `git` 都不跑，删掉只是断了那条路。
* **一切都在沙箱里跑**（`ccmon-devbox:latest`，`--network none`）。本台自己不起 docker：
  它就跑在沙箱里，由调用方把它放进去。
* **判定行口径**：daemon 侧 = 整份 `cargo test`（不过滤）印出来的那一行 `test result:`；
  monitor 侧 = `cargo test -p monitor --lib byte_cap` 的那一行。
  **核的是「判定行里的 passed+failed 总数还是不是入场那个数」**，不是「看它等不等于 0」：
  编译失败那一版判定行**根本不出现**，那是 CRASH，不是「新红 N」。
* 每一刀：**先断言锚点恰好命中 N 次**（表里写死那个 N），再改，打印「变异已落地」，
  再跑，再**原样还原**并重打一次入场读数。
* 「新红」= 这一刀红掉的测试名集合减去入场就红的那些（入场是全绿，所以等于红掉的全集）。

🔴 一条实测出来的使用纪律：**这台不许与别的 daemon 测试并跑**
--------------------------------------------------------------
09-10 夜栽过一次：本台在副本上跑的同时，我在工作树上另起了一趟 `cargo test`。
两趟都在同一个容器镜像里、`HOME` 同值，而 daemon 那条常驻监听口的端口是**从家目录 hash 出来的**
（`listen.rs`，`K-P1`）⇒ **两趟抢同一个端口**，`relay::server` 那一族 41 条线程
齐齐卡在 `inet_csk_accept`，整趟挂死 14 分钟、CPU 0%。
那**不是**被测代码的缺陷，是两趟自己撞的 —— 但它长得和「某条测试会卡死」一模一样。
⇒ 跑本台的时候，**别的什么都别跑**。

用法
----
    python3 evidence/K-W2D-wire-mutations.py            # 全跑
    python3 evidence/K-W2D-wire-mutations.py W2 W7      # 只跑点名的几刀

它只读工作树、只写副本与 stdout；**一个字节都不改工作树**。
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w2d-wire")
COPY = Path("/home/zbl/文档/claudecode-frontend/.claude/pm-targets/k-w2d-wire-mut/repo")
SKIP = {".git", "node_modules", "target", "dist", ".pm-targets"}

DAEMON = "remote-daemon-proto/src"
ACQUIRE = f"{DAEMON}/sidecars/codepicture/acquire.rs"
FETCH = f"{DAEMON}/sidecars/codepicture/fetch.rs"
SIDECARS_MOD = f"{DAEMON}/sidecars/mod.rs"
READONLY = f"{DAEMON}/readonly_guard.rs"
CAPS = "src-tauri/src/byte_cap_registry.rs"

RESULT_RE = re.compile(r"^test result: (\w+)\. (\d+) passed; (\d+) failed")
FAILED_RE = re.compile(r"^    (\S+)$")


@dataclass
class Edit:
    """一处编辑：`file` 里把 `old` 换成 `new`，并断言 `old` 恰好命中 `hits` 次。"""

    file: str
    old: str
    new: str
    hits: int = 1


@dataclass
class Cut:
    tag: str
    says: str  # 切的是**哪一句声称**
    edits: list[Edit]
    suite: str = "daemon"  # daemon | caps
    expect_empty: bool = False  # 这一刀是**空刀候选**（射程读数，不是失败）
    anchors: list[str] = field(default_factory=list)


# ── 两个套件 ────────────────────────────────────────────────────────────────
SUITES = {
    "daemon": ["bash", "-o", "pipefail", "-c", "cd remote-daemon-proto && cargo test --offline 2>&1"],
    "caps": [
        "bash",
        "-o",
        "pipefail",
        "-c",
        "cd src-tauri && cargo test --offline -p monitor --lib byte_cap 2>&1",
    ],
}


def sync_copy() -> None:
    """把工作树**整棵**同步进副本（跳过 SKIP）。每刀之间只还原改过的文件，这一步只跑一次。"""
    if COPY.exists():
        shutil.rmtree(COPY)
    COPY.parent.mkdir(parents=True, exist_ok=True)

    def ignore(_dir: str, names: list[str]) -> set[str]:
        return {n for n in names if n in SKIP}

    shutil.copytree(WT, COPY, ignore=ignore, symlinks=True)
    # 副本里绝不许留下指回原仓的那行指针。
    for stray in (COPY / ".git",):
        if stray.is_file():
            stray.unlink()
        elif stray.is_dir():
            shutil.rmtree(stray)
    assert not (COPY / ".git").exists(), "副本里还有 .git —— 断不干净就别往下跑"


def run(suite: str) -> tuple[str, int, list[str]]:
    """跑一个套件，回 (判定行, 退出码, 红掉的测试名)。判定行缺席 ⇒ 回空串（CRASH）。"""
    env = dict(os.environ)
    # 🔴 副本有**自己的** target 目录：与工作树共用一份会让两边互相把对方的产物顶掉，
    #    而那种「上一趟的产物」正是本仓栽过的假读数来源。
    env["CARGO_TARGET_DIR"] = str(COPY.parent / "target")
    env["CARGO_TERM_COLOR"] = "never"
    p = subprocess.run(SUITES[suite], cwd=COPY, capture_output=True, text=True, env=env)
    out = p.stdout + p.stderr
    line = ""
    for l in out.splitlines():
        if RESULT_RE.match(l):
            line = l.strip()
    failed: list[str] = []
    grabbing = False
    for l in out.splitlines():
        if l.strip() == "failures:":
            grabbing = True
            continue
        if grabbing:
            m = FAILED_RE.match(l)
            if m:
                failed.append(m.group(1))
            elif l.strip() and not l.startswith(" "):
                grabbing = False
    return line, p.returncode, sorted(set(failed))


def total_of(line: str) -> int | None:
    m = RESULT_RE.match(line)
    if not m:
        return None
    return int(m.group(2)) + int(m.group(3))


def counts_of(line: str) -> tuple[int, int] | None:
    """判定行里的 `(passed, failed)`。**刻意不含那个耗时**：它每趟都不一样。"""
    m = RESULT_RE.match(line)
    if not m:
        return None
    return int(m.group(2)), int(m.group(3))


def apply(edits: list[Edit]) -> dict[Path, str]:
    """落一刀，回 `{路径: 这一刀之前的原文}` 好还原。锚点命中数对不上就直接炸 —— 变异没落地比读数错更该停。

    🔴 **每个文件的原文只存第一份**〔09-10 自查逮到的台子缺陷〕：
    原先存成 `list[(路径, 那一刻的内容)]`，而全断刀那一类**一刀改同一个文件十处** ⇒
    列表里十条全指同一个文件、内容一条比一条新，还原时顺序写回去，**最后落下的是倒数第二版**。
    ⇒ 还原不干净，而**收尾重打那一格当场把它逮住了**（daemon 627/9，与入场 636/0 对不上）。
    那一格存在的理由就是这个：**台子自己也会坏，而坏了的台子出的每个数都不算数。**
    """
    saved: dict[Path, str] = {}
    for e in edits:
        p = COPY / e.file
        src = p.read_text(encoding="utf-8")
        n = src.count(e.old)
        if n != e.hits:
            restore(saved)
            raise SystemExit(f"❌ 锚点命中 {n} 次，表里写的是 {e.hits} 次：{e.file}\n   锚点：{e.old[:90]!r}")
        saved.setdefault(p, src)
        p.write_text(src.replace(e.old, e.new), encoding="utf-8")
        print(f"   变异已落地：{e.file}（锚点命中 {n} 次）")
    return saved


def restore(saved: dict[Path, str]) -> None:
    for path, text in saved.items():
        path.write_text(text, encoding="utf-8")


# ── 刀表 ────────────────────────────────────────────────────────────────────
CUTS: list[Cut] = [
    Cut(
        "W1",
        "acquire 头注：「HTTP 那一跳……真开 socket、真写请求」——请求真的发出去了",
        [Edit(ACQUIRE, "    if s.write_all(req.as_bytes()).is_err() {", "    if false {")],
        anchors=["get_over 的 write_all 那一支"],
    ),
    Cut(
        "W2",
        "RESPONSE_HEAD_BYTE_CAP 头注：「两个量分两个数，共用一个会静默截断」",
        [Edit(ACQUIRE, "let ceiling = RESPONSE_HEAD_BYTE_CAP + cap + 1;", "let ceiling = cap + 1;")],
        anchors=["get_over 的 ceiling"],
    ),
    Cut(
        "W3",
        "Face 头注：「非 200 有自己的一格，不许被当成拿到了」",
        [Edit(ACQUIRE, "    if code != 200 {", "    if false {")],
        anchors=["get_over 的状态码那一关"],
    ),
    Cut(
        "W4",
        "status_of 的 docstring：「读不出来就回 None —— **不许当成 200**」",
        [
            Edit(
                ACQUIRE,
                "pub fn status_of(head: &[u8]) -> Option<u16> {",
                "pub fn status_of(head: &[u8]) -> Option<u16> {\n    if true {\n        return Some(200);\n    }",
            )
        ],
        anchors=["status_of 函数头"],
    ),
    Cut(
        "W5",
        "get_over 的 docstring：「超了立刻停手并回 Oversize，一个字节都不落盘」",
        [
            Edit(
                ACQUIRE,
                "    (within_cap(body.len() as u64, cap), body)",
                "    (Transport::Got(body.len() as u64), body)",
            )
        ],
        anchors=["get_over 的上限那一关"],
    ),
    Cut(
        "W6",
        "land 的 docstring：「可执行位在**新建那一刻**给」",
        [Edit(ACQUIRE, ".create_new(true).mode(0o755)", ".create_new(true).mode(0o644)")],
        anchors=["land 的 mode"],
    ),
    Cut(
        "W7",
        "land 的 docstring：「🔴 只准新增：不截断、不追加、不覆盖」",
        [Edit(ACQUIRE, ".write(true).create_new(true).mode(0o755)", ".write(true).truncate(true).create(true).mode(0o755)")],
        anchors=["land 的 O_EXCL"],
    ),
    Cut(
        "W8",
        "fetch 头注二：「刚拉回来的那份对不上 ⇒ 到此为止，不重试、不落盘」",
        [Edit(ACQUIRE, "    integrity_verdict(verify_bytes(&bytes, sha256))?;", "")],
        anchors=["obtain 的校验那一步"],
    ),
    Cut(
        "W9",
        "fetch 头注四：「『没算过』不是『验过、相符』」——盘上那份要现算一遍才敢用",
        [
            Edit(
                ACQUIRE,
                "        Step::Verify => use_or_fetch(\n            Landed::Present,\n            Integrity::Checked(read_landed(&path, sha256)),\n        ),",
                "        Step::Verify => Step::Use,",
            )
        ],
        anchors=["obtain 的 Verify 那一支"],
    ),
    Cut(
        "W10",
        "look 的 docstring + fetch 头注：「问不出来不许读成上面任何一个确定答案」",
        [Edit(ACQUIRE, "        Err(_) => Landed::Unknown,", "        Err(_) => Landed::Missing,")],
        anchors=["look 的 Err 那一支"],
    ),
    Cut(
        "W11",
        "trust_roots 的 docstring：「判据要打**真正装进去的那一份**」",
        [
            Edit(
                ACQUIRE,
                "        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),",
                "        roots: Vec::new(),",
            )
        ],
        anchors=["trust_roots"],
    ),
    Cut(
        "W12",
        "KW2D5 逐字预言的那一红：「sidecar = 落地当天写面白名单**必然红，红得对** ——"
        "那一红就是有人在这张表上签了字」。把那条签字摘掉，看它红不红",
        [
            Edit(
                READONLY,
                '            "sidecars/codepicture/acquire.rs",\n',
                '            "sidecars/codepicture/acquire_NOT_SIGNED.rs",\n',
            )
        ],
        anchors=["白名单表里 sidecar 那条登记的住址"],
    ),
    Cut(
        "W13",
        "同上头注：「每一条都得写清它写什么（`why` 有长度地板）」——幽灵检查那一格的最小刀",
        [
            Edit(
                READONLY,
                '            "G2 / `--fork-session`：用 `O_EXCL` 在 projects 目录里新建一份**此前不存在**的 \\\n'
                "             jsonl，不改、不覆盖、不删任何既有文件 —— 这正是 `D1` 收窄后那条铁律的误差项\",",
                '            "短",',
            )
        ],
        anchors=["白名单表里 fork_write 那条的 why"],
    ),
    Cut(
        "W14",
        "sidecars/mod.rs 头注：「那一行只许静默死码这一种、只许有这一处」",
        [Edit(SIDECARS_MOD, "#![allow(dead_code)]", "#![allow(unused)]")],
        anchors=["层级 allow"],
    ),
    Cut(
        "W15",
        "byte_cap_registry：「表里的数字必须与源码一致」——上限那个数有住址了",
        [Edit(ACQUIRE, "pub const ASSET_BYTE_CAP: u64 = 64 * 1024 * 1024;", "pub const ASSET_BYTE_CAP: u64 = 32 * 1024 * 1024;")],
        suite="caps",
        anchors=["ASSET_BYTE_CAP 的声明"],
    ),
    # ── `W21`–`W25`：**给全断刀之后仍绿的那几格补单断**〔`brief` 第 9 条 · `7u`〕──────
    #
    # 头一趟全断刀跑完，19 格新断言里**有 6 格一次都没红过**。
    # 「仍绿不等于仪式，它的牙可能在别的刀上」—— 而那句话要成立，得真有那把刀。
    # 下面五刀就是去把那几格逐个打一遍；剩下那一格（`look` 的 `Err` 支）是**判据自己没盖到**，
    # 由 `a_stat_that_fails_is_unknown_not_missing` 补上，再由 `W10` 打。
    Cut(
        "W21",
        "get_over 的 docstring：「把响应体收回来」——收回来的**真的是对面那份字节**",
        [Edit(ACQUIRE, "    let body = all.split_off(cut);", "    let body = vec![0u8; 1];")],
        anchors=["get_over 取响应体那一行"],
    ),
    Cut(
        "W22",
        "Net 的注释：「连不上……处置与拆不动一致」——连不上**不许变成一次成功**",
        [
            Edit(
                ACQUIRE,
                "            Err(_) => return (Transport::Offline, Vec::new()),\n        };\n        get_over(",
                "            Err(_) => return (Transport::Got(0), Vec::new()),\n        };\n        get_over(",
            )
        ],
        anchors=["Net::get 里 dial 失败那一支"],
    ),
    Cut(
        "W23",
        "`SCHEMES` 的 docstring：「认得的两种协议。**闭集只有这一个住址**」"
        "——不认识的协议不许被当成认识",
        [
            Edit(
                ACQUIRE,
                "        let (_, tls, default_port) = match SCHEMES.iter().find(|(s, ..)| *s == scheme) {",
                "        let (_, tls, default_port) = match SCHEMES.iter().find(|(s, ..)| *s == scheme || true) {",
            )
        ],
        anchors=["Target::parse 的协议那一关"],
    ),
    Cut(
        "W24",
        "land 的 docstring + `Landing` 头注：「无写权限有自己的一格，别的 IO 错落另一格」",
        [
            Edit(
                FETCH,
                "        std::io::ErrorKind::PermissionDenied => Landing::Denied,",
                "        std::io::ErrorKind::PermissionDenied => Landing::Io,",
            )
        ],
        anchors=["classify_write_error 的 Denied 那一支"],
    ),
    Cut(
        "W25",
        "`Unusable` 的 docstring：「那**一句**话。每个成员恰好一句」——三种坏法不许说同一句",
        [
            Edit(
                ACQUIRE,
                '                "代码全景 sidecar 起来了，但在期限内没答完 —— 这一趟按失败算。".to_string()',
                '                format!("代码全景 sidecar 落到盘上了，但起不起来：{}", "x")',
            )
        ],
        anchors=["Unusable::sentence 的 TimedOut 那一支"],
    ),
    Cut(
        "W19",
        "🔴 **本拍「没做到」那一条的现打证据**：件文件 `KW2D5` 要的「起进程点表签一次字」"
        "＝把 `SPAWN_SITES_TODAY` 从 9 改成 10，而 `ratchet_guard::PINS` 逐字钉着那一整行。"
        "这一刀就是那一改本身 —— 看谁红",
        [Edit(READONLY, "const SPAWN_SITES_TODAY: usize = 9;", "const SPAWN_SITES_TODAY: usize = 10;")],
        anchors=["SPAWN_SITES_TODAY 的声明行"],
    ),
    Cut(
        "W20",
        "判据⑧的另一半：「只许有这**一处**」（`W14` 打的是「只许静默死码这一种」）",
        [
            Edit(
                ACQUIRE,
                "pub const ASSET_BYTE_CAP: u64 = 64 * 1024 * 1024;",
                "#[allow(dead_code)]\npub const ASSET_BYTE_CAP: u64 = 64 * 1024 * 1024;",
            )
        ],
        anchors=["acquire.rs 顶上再挂一个 allow"],
    ),
    Cut(
        "W16",
        "【空刀候选】ask 的 docstring：「期限靠 `timeout` 当命令前缀交给子进程」"
        "——我**没有**为「那个秒数真的传下去了」立过判据",
        [Edit(ACQUIRE, "    let done: Done = match invoke::run(bin, args, deadline_secs, &[]) {", "    let done: Done = match invoke::run(bin, args, 0, &[]) {")],
        expect_empty=True,
        anchors=["ask 里那次 invoke::run"],
    ),
    Cut(
        "W17",
        "【空刀候选】get_over 写的那几个请求头——我只钉了请求行与 `Host:`，别的一个都没钉",
        [Edit(ACQUIRE, "User-Agent: cc-monitor-remote\\r\\n", "")],
        expect_empty=True,
        anchors=["请求里的 User-Agent 头"],
    ),
]

# ── 全断刀 `W18`：把 acquire 的生产函数整片换成恒答一张脸 ──────────────────
W18 = Cut(
    "W18",
    "【全断刀 · 7u】把 acquire 那几处「动世界」的实现整片退掉，看还有几条新断言仍绿",
    [
        Edit(ACQUIRE, "    if s.write_all(req.as_bytes()).is_err() {", "    if false {"),
        Edit(ACQUIRE, "let ceiling = RESPONSE_HEAD_BYTE_CAP + cap + 1;", "let ceiling = cap + 1;"),
        Edit(ACQUIRE, "    if code != 200 {", "    if false {"),
        Edit(
            ACQUIRE,
            "pub fn status_of(head: &[u8]) -> Option<u16> {",
            "pub fn status_of(head: &[u8]) -> Option<u16> {\n    if true {\n        return Some(200);\n    }",
        ),
        Edit(ACQUIRE, "    (within_cap(body.len() as u64, cap), body)", "    (Transport::Got(body.len() as u64), body)"),
        Edit(ACQUIRE, ".create_new(true).mode(0o755)", ".create_new(true).mode(0o644)"),
        Edit(ACQUIRE, "        Err(_) => Landed::Unknown,", "        Err(_) => Landed::Missing,"),
        Edit(ACQUIRE, "        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),", "        roots: Vec::new(),"),
        Edit(ACQUIRE, "    integrity_verdict(verify_bytes(&bytes, sha256))?;", ""),
        Edit(
            ACQUIRE,
            "        Step::Verify => use_or_fetch(\n            Landed::Present,\n            Integrity::Checked(read_landed(&path, sha256)),\n        ),",
            "        Step::Verify => Step::Use,",
        ),
    ],
    anchors=["acquire 生产段 10 处"],
)


def main() -> int:
    want = set(sys.argv[1:])
    print(f"· 同步副本：{COPY}")
    sync_copy()

    entry: dict[str, tuple[str, int]] = {}
    for suite in ("daemon", "caps"):
        line, rc, failed = run(suite)
        n = total_of(line)
        if n is None or failed:
            print(f"❌ 入场读数不干净（{suite}）：判定行={line!r} rc={rc} 红={failed}")
            return 2
        entry[suite] = (line, n)
        print(f"· 入场（{suite}）：{line}")

    rows: list[str] = []
    for cut in list(CUTS) + [W18]:
        if want and cut.tag not in want:
            continue
        print(f"\n── {cut.tag} ──  切的声称：{cut.says}")
        saved = apply(cut.edits)
        try:
            line, rc, failed = run(cut.suite)
        finally:
            restore(saved)
        base_line, base_n = entry[cut.suite]
        n = total_of(line)
        if n is None:
            verdict = "CRASH（判定行没出现 ⇒ 多半没编过，这一刀作废）"
        elif n != base_n:
            verdict = f"CRASH（判定行总数 {n} ≠ 入场 {base_n} ⇒ 台子变了，不是读数）"
        else:
            verdict = f"新红 {len(failed)}"
        mark = ""
        if cut.expect_empty:
            mark = " 【空刀，预期如此】" if not failed else " 【原以为是空刀，实测有牙】"
        print(f"   判定行：{line or '(缺席)'}   rc={rc}   {verdict}{mark}")
        for f in failed:
            print(f"     红：{f}")
        rows.append(
            "| `{}` | {} | {} · {} | {} | {} |".format(
                cut.tag,
                cut.says.replace("|", "/"),
                " ＋ ".join(cut.anchors) or "见刀表",
                " ＋ ".join(f"{e.hits}" for e in cut.edits),
                line or "(缺席)",
                "；".join(failed) or "**0（空刀）**",
            )
        )

    # 还原之后重打一次，证明台子回到了入场那一格。
    # ⚠ **比的是格数，不是整行**：判定行里带着「跑了几秒」，两趟字面必然不同 ——
    #   按整行比会把每一趟都判成「还原没干净」（09-10 自查逮到的第二个台子缺陷）。
    print("\n· 收尾重打")
    for suite in ("daemon", "caps"):
        line, rc, failed = run(suite)
        ok = counts_of(line) == counts_of(entry[suite][0]) and not failed
        print(f"  {suite}：{line}   {'✅ 与入场同格' if ok else '❌ 与入场不同格 —— 还原没干净'}")

    print("\n" + "\n".join(rows))
    return 0


if __name__ == "__main__":
    os.environ.setdefault("CARGO_TERM_COLOR", "never")
    raise SystemExit(main())
