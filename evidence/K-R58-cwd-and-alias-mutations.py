#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R58` 死值验量具 —— 一刀一刀地问「本件那三条判据到底有没有牙」。

# 被测对象指向哪棵树（`brief` 第 12 条要的那一栏）

    WT = /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r58   （分支 `track/k-r58`）

量具自己的住址（同一条纪律的另一半）：`evidence/K-R58-cwd-and-alias-mutations.py`，
名字里带件号 ⇒ 别的 agent 同名覆盖不了它。

本量具**在原地改文件、跑一次判据、再改回去**，每一刀在 `finally` 里恢复并逐字核 md5
（改不回去就当场 `RuntimeError`，不许留一个被变异过的盘）。

# 🔴 每一刀都先断言锚点**恰好命中 N 次**

〔PM 09-11 自己栽过：正则没命中，「切后」与基线逐字相同，只看判定行会得出完全相反的
结论 —— **「没切」与「没红」在输出上同形**。〕⇒ 这里锚点是**逐字串**（不是正则），
命中数对不上当场 `AssertionError`，并且改完之后再核一次「内容真的变了」才去跑判据。

# 它跑在哪儿

一律在沙箱里（`ccmon-devbox:latest`），**宿主上零测试** —— 挂载面与
`.claude/devbox/gate` 那条 `docker run` 逐项相同，只把最后那条命令从 `scripts/gate.sh`
换成一次带过滤串的 `cargo test`。

# 它量的是什么、不量什么

- 量：每一刀之后**点到的那几条判据各自 ok / FAILED / 编不过**，以及第一条炸掉的报文。
- 不量：门禁其余各格（那是 `gate` 的活）；也不量 `e2e/ccm-cli.test.sh`
  （那一套的红在上报里单列，读数是「旧判据 × 新实现」= PASS=43 FAIL=3）。

用法：
    python3 evidence/K-R58-cwd-and-alias-mutations.py          # 全跑
    python3 evidence/K-R58-cwd-and-alias-mutations.py MR3a     # 只跑点名的
"""

import hashlib
import subprocess
import sys
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r58")
PROJ = Path("/home/zbl/文档/claudecode-frontend")
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
TAG = "k-r58"

SH = WT / "shared/ccm-aliases.sh"
PLAN = WT / "remote-daemon-proto/src/control/ccm/plan.rs"
DOC = WT / "doc/IPC-PROTOCOL.md"

# ── 判据（三条新的 + 两条被本件改过的既有的）─────────────────────────────
D1 = "cch_is_gone_and_the_name_is_free_for_the_user"
D1B = "a_name_that_is_already_taken_gets_a_note"  # 人群改成现算的那一条
D2 = "the_protocol_doc_sentence_about_the_alias_block_matches_the_file"
D2B = "ccm_aliases_snippet_has_required_elements"  # 既有的要素清单
D3 = "the_default_cwd_is_the_identity_in_every_layout"

MONITOR_FILTER = f"{D1} {D1B} {D2} {D2B}"
DAEMON_FILTER = D3

# ── 锚点（全部逐字，不用正则）────────────────────────────────────────────
CC_BLOCK = (
    "if ! declare -f cc >/dev/null 2>&1; then\n"
    'cc()  { ccm "$@"; }                        # 在当前目录起会话\n'
    "fi\n"
)
CCT_HEAD = "if ! declare -f cct >/dev/null 2>&1; then\n"
CCT_BLOCK = (
    CCT_HEAD
    + 'cct() { ccm --tmux "$@"; }                 # 在 tmux 里起（断线可 attach 回来）\n'
    + "fi\n"
)
# `K-R58` 删掉的那两行（原样）
CCH_BLOCK = (
    "if ! declare -f cch >/dev/null 2>&1; then\n"
    'cch() { ccm --cwd . "$@"; }                # 当前目录直起\n'
    "fi\n"
)

RESOLVE_BODY = (
    "pub(crate) fn resolve_cwd(o: &Opts, env: &Env) -> String {\n"
    "    match &o.cwd_spec {\n"
    "        CwdSpec::Explicit(d) => d.clone(),\n"
    "        CwdSpec::Auto => env.pwd.clone(),\n"
    "    }\n"
    "}\n"
)


def resolve_with(extra: str, explicit: str = "d.clone()") -> str:
    """造一份**形状对**的 `resolve_cwd`：仍然收 `(&Opts, &Env)`、仍然回 `String`。

    〔`brief` 第 7 条：按文本替换容易落在**类型**上而不是**语义**上 ⇒ 台子炸了那不是
    读数是 CRASH。正确形状是「形状对、恒答其中一张脸」。〕
    """
    return (
        "pub(crate) fn resolve_cwd(o: &Opts, env: &Env) -> String {\n"
        "    match &o.cwd_spec {\n"
        f"        CwdSpec::Explicit(d) => return {explicit},\n"
        "        CwdSpec::Auto => {}\n"
        "    }\n"
        "    if o.action != Action::New {\n"
        "        return env.pwd.clone();\n"
        "    }\n"
        f"{extra}"
        "    env.pwd.clone()\n"
        "}\n"
    )


GUESS_HOME = (
    "    if env.pwd == env.home {\n"
    '        return format!("{}/claude-conversation", env.home);\n'
    "    }\n"
)
GUESS_GIT = (
    "    let mut p = std::path::Path::new(&env.pwd);\n"
    "    loop {\n"
    '        if p.join(".git").exists() {\n'
    "            return p\n"
    "                .parent()\n"
    "                .map(|x| x.to_string_lossy().to_string())\n"
    "                .unwrap_or_else(|| env.pwd.clone());\n"
    "        }\n"
    "        match p.parent() {\n"
    "            Some(up) if up != p => p = up,\n"
    "            _ => break,\n"
    "        }\n"
    "    }\n"
)
# 「把猜挪进别处」——`KR58D3` 的失效方向逐字：挪成一个**默认开着的开关**。
GUESS_BEHIND_DEFAULT_ON_SWITCH = (
    '    if std::env::var("CCM_NO_GUESS").as_deref() != Ok("1") && env.pwd == env.home {\n'
    '        return format!("{}/claude-conversation", env.home);\n'
    "    }\n"
)

DOC_SENT = "那个其实是 `shared/ccm-aliases.sh`，**36 行、别名只有 `cc`/`cct` 这 2 个**，\n"
DOC_SENT_OLD = "那个其实是 `shared/ccm-aliases.sh`，**29 行、只有 `cc`/`cch`/`cct` 三个别名**，\n"


class Cut:
    """一刀 = 若干 (文件, 原串, 新串, 该命中几处) + 跑哪一套判据。"""

    def __init__(self, cid, dod, what, edits, suite, expect):
        self.cid, self.dod, self.what = cid, dod, what
        self.edits, self.suite, self.expect = edits, suite, expect


CUTS = [
    # ── KR58D1：`cch` 真的不在了 ────────────────────────────────────────
    Cut("MR1a", "KR58D1", "把 `cch` 那三行整块加回 `shared/ccm-aliases.sh`（= 把①整个退掉）",
        [(SH, CCT_HEAD, CCH_BLOCK + CCT_HEAD, 1)], "monitor", "红"),
    Cut("MR1b", "KR58D1", "只把 `cch()` 那一行加回去（不带 `declare -f` 守卫）—— 判据认的该是「定义了没有」，不是「有没有守卫」",
        [(SH, CCT_HEAD, 'cch() { ccm --cwd . "$@"; }\n' + CCT_HEAD, 1)], "monitor", "红"),
    Cut("MR1c", "KR58D1", "把 `cc` 那一条删掉（人群少一个，但不空）—— 问人群是不是真的现算",
        [(SH, CC_BLOCK, "", 1)], "monitor", "观测"),
    Cut("MR1d", "KR58D1", "把别名全删了（人群变**空**）—— 问它会不会静默塌成空真",
        [(SH, CC_BLOCK, "", 1), (SH, CCT_BLOCK, "", 1)], "monitor", "红"),
    # ── KR58D2：数与名单同句，两样一起对 ────────────────────────────────
    Cut("MR2a", "KR58D2", "只改文档里的**数**（36 → 37），名单不动",
        [(DOC, "**36 行、", "**37 行、", 1)], "monitor", "红"),
    Cut("MR2b", "KR58D2", "🔴 只改文档里的**名单**（把 `cch` 写回去），数不动 —— 这一刀就是「数与名单同句、只改一半」那条病的正面",
        [(DOC, "别名只有 `cc`/`cct` 这 2 个", "别名只有 `cc`/`cch`/`cct` 这 2 个", 1)],
        "monitor", "红"),
    Cut("MR2c", "KR58D2", "把文档那一整句删掉 —— 判据够不着被测对象时必须**响亮地红**，不许变空真",
        [(DOC, DOC_SENT, "", 1)], "monitor", "红"),
    # ── KR58D3：默认是恒等 ──────────────────────────────────────────────
    Cut("MR3a", "KR58D3", "把「站在 $HOME 就跳工作区」那一档加回 `resolve_cwd`",
        [(PLAN, RESOLVE_BODY, resolve_with(GUESS_HOME), 1)], "daemon", "红"),
    Cut("MR3b", "KR58D3", "把「往上找得到 `.git` 就跳仓的父目录」那一档加回",
        [(PLAN, RESOLVE_BODY, resolve_with(GUESS_GIT), 1)], "daemon", "红"),
    Cut("MR3c", "KR58D3", "两档一起加回（= 把②整个退掉，逐字是 `K-R58` 之前那份实现）",
        [(PLAN, RESOLVE_BODY, resolve_with(GUESS_HOME + GUESS_GIT), 1)], "daemon", "红"),
    Cut("MR3d", "KR58D3", "🔴 **失效方向逐字**：把「猜」挪成一个**默认开着的开关**（`CCM_NO_GUESS=1` 才关）",
        [(PLAN, RESOLVE_BODY, resolve_with(GUESS_BEHIND_DEFAULT_ON_SWITCH), 1)], "daemon", "红"),
    Cut("MR3e", "KR58D3", "反向对照：把**显式** `--cwd` 那一支也改成回 `pwd`（证这条判据不是恒真）",
        [(PLAN, RESOLVE_BODY, resolve_with("", explicit="env.pwd.clone()"), 1)], "daemon", "红"),
    # ── 7u：把实现整个退掉，还有多少条新断言仍绿 ────────────────────────
    Cut("MR7u-m", "7u", "把①与 `KR58D2` 的实现面整个退掉（`cch` 加回 + 文档那句换回旧的），只留判据",
        [(SH, CCT_HEAD, CCH_BLOCK + CCT_HEAD, 1), (DOC, DOC_SENT, DOC_SENT_OLD, 1)],
        "monitor", "红"),
    Cut("MR7u-d", "7u", "把②整个退掉（`resolve_cwd` 逐字回到 `K-R58` 之前），只留判据",
        [(PLAN, RESOLVE_BODY, resolve_with(GUESS_HOME + GUESS_GIT), 1)], "daemon", "红"),
]


def md5(p: Path) -> str:
    return hashlib.md5(p.read_bytes()).hexdigest()


def in_box(script: str) -> str:
    """在沙箱里跑一条命令（挂载面与 `.claude/devbox/gate` 那条 `docker run` 逐项相同）。

    🔴 `mkdir -p $HOME/.claude/projects` 不许省：容器里 `HOME` 几乎是空的，而
    `history::tests::the_delete_entry_point_actually_goes_through_the_fence` 要
    `canonicalize ~/.claude/projects`，省掉它会稳定多报一条**量具自己造的**红。
    """
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}",
        "-e", "HOME=/home/zbl",
        "-w", str(WT),
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        'mkdir -p "$HOME/.claude/projects"; ' + script,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout + r.stderr


def run_suite(suite: str) -> str:
    if suite == "monitor":
        return in_box(f"cd src-tauri && cargo test --lib -p monitor -- {MONITOR_FILTER} 2>&1")
    return in_box(f"cd remote-daemon-proto && cargo test {DAEMON_FILTER} 2>&1")


def verdicts(out: str, names) -> dict:
    """逐条判据的结论。**按名字认**，不按「有没有 FAILED 这个词」认。"""
    v = {}
    for n in names:
        if f"test {n} ... ok" in out or out.count(f"::{n} ... ok"):
            v[n] = "ok"
        elif f"{n} ... FAILED" in out:
            v[n] = "FAILED"
        else:
            v[n] = "没跑到"
    if "error: could not compile" in out or "error[E" in out:
        for n in names:
            if v[n] == "没跑到":
                v[n] = "编不过(CRASH)"
    return v


def apply_cut(cut: Cut):
    saved = {}
    for path, old, new, n in cut.edits:
        if path not in saved:
            saved[path] = (path.read_text(encoding="utf-8"), md5(path))
    try:
        for path, old, new, n in cut.edits:
            src = path.read_text(encoding="utf-8")
            hits = src.count(old)
            assert hits == n, (
                f"{cut.cid}：锚点命中 {hits} 次，要的是 {n} 次 —— "
                f"**没切**与**没红**在输出上同形，这一刀作废。文件 {path.name}"
            )
            out = src.replace(old, new)
            assert out != src, f"{cut.cid}：替换之后内容逐字相同 —— 这一刀什么都没干"
            path.write_text(out, encoding="utf-8")
        print(f"  变异已落地（锚点各命中 {[e[3] for e in cut.edits]} 次）")
        names = [D1, D1B, D2, D2B] if cut.suite == "monitor" else [D3]
        out = run_suite(cut.suite)
        v = verdicts(out, names)
        # 头一条炸的：`panicked at <住址>:` 的**下一行**才是报文 ⇒ 两行一起取，
        # 只取住址会把「它到底在骂什么」丢掉，而那正是判断「红得对不对」的唯一依据。
        first_panic = ""
        lines = out.splitlines()
        for i, line in enumerate(lines):
            if "panicked at" in line:
                msg = " ".join(x.strip() for x in lines[i + 1: i + 4] if x.strip())
                first_panic = f"{line.strip()[line.strip().find('panicked at'):]} ｜ {msg}"
                break
        return v, first_panic
    finally:
        for path, (text, h) in saved.items():
            path.write_text(text, encoding="utf-8")
            if md5(path) != h:
                raise RuntimeError(f"{cut.cid}：{path} 恢复后 md5 对不上 —— 盘上留了一把刀，先修这个")


def main():
    want = set(sys.argv[1:])
    cuts = [c for c in CUTS if not want or c.cid in want]
    print(f"被测对象：{WT}（分支 track/k-r58）")
    print(f"量于：{subprocess.run(['date', '-Is'], capture_output=True, text=True).stdout.strip()}")
    print(f"基线 sha：{subprocess.run(['git', '-C', str(WT), 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()}")
    print()
    for c in cuts:
        print(f"[{c.cid}] dod={c.dod} 期望={c.expect}")
        print(f"  切的是：{c.what}")
        v, panic = apply_cut(c)
        print(f"  读数：{v}")
        if panic:
            print(f"  头一条炸的：{panic}")
        print()


if __name__ == "__main__":
    main()
