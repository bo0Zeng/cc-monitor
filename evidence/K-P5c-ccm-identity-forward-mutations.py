#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-P5c` 死值验量具 —— 一刀一刀地问「这两条判据到底有没有牙」。

# 被测对象指向哪棵树（`brief` 第 12 条要的那一栏）

    WT = /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-p5c   （分支 `track/k-p5c`）

本量具**在原地改那两份文件、跑一次判据、再改回去**，每一刀都在 `finally` 里恢复并
**逐字核 md5**（改不回去就当场 `RuntimeError`，不许留一个被变异过的盘）。
不用「拷一份工作树」是因为 `CARGO_TARGET_DIR` 是按树名分的，换个路径等于全量重编。

# 它跑在哪儿

一律在沙箱里（`ccmon-devbox:latest`），**宿主上零测试** —— 挂载面与
`.claude/devbox/gate` 那条 `docker run` 逐项相同，只把最后那条命令从 `scripts/gate.sh`
换成一次 `cargo test --lib <过滤串>`。

# 它量的是什么、不量什么

- 量：每一刀之后**那两条判据各自 ok / FAILED / 编不过**，以及第一条炸掉的断言的报文。
- 不量：门禁其余六格（那是 `gate` 的活）。
- 一刀同时会动到 `K-H2b` 那条**先例**判据时（凡改 `shared/ccm` 的都可能），
  两条都列出来 —— 分母是「本次跑到的测试」，不是「全仓」。

用法：
    python3 evidence/K-P5c-ccm-identity-forward-mutations.py            # 全跑变异表
    python3 evidence/K-P5c-ccm-identity-forward-mutations.py MC1a MC2b  # 只跑点名的
    python3 evidence/K-P5c-ccm-identity-forward-mutations.py --census   # 入场/交回格数各现打一次
"""

import hashlib
import re
import subprocess
import sys
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-p5c")
PROJ = Path("/home/zbl/文档/claudecode-frontend")
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
TAG = "k-p5c"
FILTER = "the_ccm_container_path_forwards"

CCM = WT / "shared/ccm"
JUDGE = WT / "src-tauri/src/backend/control/payload.rs"

MINE = "the_ccm_container_path_forwards_the_launch_identity_across_the_tmux_boundary"
PRIOR = "the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary"

# `shared/ccm` 里本件那条转发的三行（整块）
BLOCK_MINE = (
    '  if [ -n "${CCM_LAUNCH_ID:-}" ]; then\n'
    '    payload="export CCM_LAUNCH_ID=$(sq "$CCM_LAUNCH_ID"); $payload"\n'
    "  fi\n"
)
LINE_R08 = '    payload="export CLAUDE_CONFIG_DIR=$(sq "$CLAUDE_CONFIG_DIR"); $payload"\n'
LINE_H2B = '    payload="export ANTHROPIC_BASE_URL=$(sq "$ANTHROPIC_BASE_URL"); $payload"\n'

# 判据里 ② 那一格（抽取器自检 + 非空对照）的整块
BLOCK_SELFCHECK_HEAD = "        for (who, needle) in [\n"
BLOCK_SELFCHECK_TAIL = (
    "            assert!(\n"
    "                window.contains(needle),\n"
    '                "窗口里看不见既有转发 `{who}` —— 窗口取错了，下面整条是空真。实得：{window}"\n'
    "            );\n"
    "        }\n"
)
LINE_START_CONST = '        const START: &str = "\\n  payload=\\"\\"\\n";\n'


def slice_selfcheck(src: str) -> str:
    """把判据里 ② 那一整块（`for (who, needle) …` 到它的 `}`）切出来。**恰好一处**。"""
    i = src.index(BLOCK_SELFCHECK_HEAD)
    j = src.index(BLOCK_SELFCHECK_TAIL, i) + len(BLOCK_SELFCHECK_TAIL)
    assert src.count(BLOCK_SELFCHECK_HEAD) == 1, "② 那块的头锚点不是恰好一处"
    return src[i:j]


class Cut:
    """一刀 = 若干 (文件, 原串, 新串, 该命中几处)。"""

    def __init__(self, cid, dod, what, edits, expect):
        self.cid, self.dod, self.what, self.edits, self.expect = cid, dod, what, edits, expect


def build_cuts():
    judge_src = JUDGE.read_text(encoding="utf-8")
    selfcheck = slice_selfcheck(judge_src)
    # 一个「歪但仍包住那三条转发」的起点锚点：往前挪到 `inner=` 那一行
    skew_wide = '        const START: &str = "\\n  inner=(\\"$CCM_SELF\\")\\n";\n'
    # 一个「全树 18 处」的锚点 —— 专打 ① 那格（唯一性）
    skew_many = '        const START: &str = "\\n  fi\\n";\n'

    return [
        Cut("MC1a", "KP5CD1", "把本件那条转发整块从 `shared/ccm` 删掉",
            [(CCM, BLOCK_MINE, "", 1)], "红"),
        Cut("MC1b", "KP5CD1", "把守卫条件写反（`-n` → `-z`）",
            [(CCM, '  if [ -n "${CCM_LAUNCH_ID:-}" ]; then\n',
              '  if [ -z "${CCM_LAUNCH_ID:-}" ]; then\n', 1)], "红"),
        Cut("MC1c", "KP5CD1", "只在 `ccm` 一侧把变量名改掉（与 `history.rs` 那个常量漂开）",
            [(CCM, BLOCK_MINE, BLOCK_MINE.replace("CCM_LAUNCH_ID", "CCM_LAUNCH_ID2"), 1)], "红"),
        Cut("MC1d", "KP5CD1", "把转发拼到载荷**外侧**（丢掉 `; ` 那个分隔）",
            [(CCM, 'payload="export CCM_LAUNCH_ID=$(sq "$CCM_LAUNCH_ID"); $payload"',
              'payload="export CCM_LAUNCH_ID=$(sq "$CCM_LAUNCH_ID") $payload"', 1)], "红"),
        Cut("MC2a", "KP5CD2", "把窗口起点锚点改歪（挪到 `inner=` 那行，窗口变大但仍包住三条转发）",
            [(JUDGE, LINE_START_CONST, skew_wide, 1)], "红"),
        Cut("MC2b", "KP5CD2", "① 那格：把起点锚点换成全树 18 处的串",
            [(JUDGE, LINE_START_CONST, skew_many, 1)], "红"),
        Cut("MC2c", "KP5CD2", "② 的被测物：把 `R08` 那条既有转发从 `ccm` 删掉",
            [(CCM, LINE_R08, "", 1)], "红"),
        Cut("MC2d", "KP5CD2", "② 的被测物：把 `K-H2b` 那条既有转发从 `ccm` 删掉",
            [(CCM, LINE_H2B, "", 1)], "红"),
        # ↓ 空真论证：**拆掉 ② 本身**，再叠上面那几刀 —— 看它还红不红。
        Cut("MC2e", "KP5CD2", "拆掉 ② 那一整格（只拆，不叠别的）",
            [(JUDGE, selfcheck, "", 1)], "观测"),
        Cut("MC2f", "KP5CD2", "拆掉 ② **并且**把起点锚点改歪（空真论证：② 是不是那一刀唯一的牙）",
            [(JUDGE, selfcheck, "", 1), (JUDGE, LINE_START_CONST, skew_wide, 1)], "观测"),
        Cut("MC2g", "KP5CD2", "拆掉 ② **并且**删掉 `R08` 那条转发（空真论证）",
            [(JUDGE, selfcheck, "", 1), (CCM, LINE_R08, "", 1)], "观测"),
        # ↓ 找「② 有没有一格**独占**的牙」：把 `R08` 改写成**行为等价、写法不同**的一行。
        #   ㈡ 看行为 ⇒ 该绿；② 是文本针 ⇒ 该红。红了就说明 ② 不是纯仪式。
        Cut("MC2h", "KP5CD2", "把 `R08` 改写成行为等价但写法不同的一行（`$X` → `${X}`）",
            [(CCM, LINE_R08, LINE_R08.replace('"$CLAUDE_CONFIG_DIR"', '"${CLAUDE_CONFIG_DIR}"'), 1)],
            "红"),
        Cut("MC2i", "KP5CD2", "同上 **并且**拆掉 ②（证 ② 是那一刀唯一的牙）",
            [(JUDGE, selfcheck, "", 1),
             (CCM, LINE_R08, LINE_R08.replace('"$CLAUDE_CONFIG_DIR"', '"${CLAUDE_CONFIG_DIR}"'), 1)],
            "观测"),
    ]


def md5(p: Path) -> str:
    return hashlib.md5(p.read_bytes()).hexdigest()


def in_box(script: str) -> str:
    """在沙箱里跑一条命令（挂载面与 `.claude/devbox/gate` 那条 `docker run` 逐项相同）。

    🔴 那个 `mkdir -p $HOME/.claude/projects` **不许省**：容器里 `HOME` 几乎是空的，
    而 `history::tests::the_delete_entry_point_actually_goes_through_the_fence` 要
    `canonicalize ~/.claude/projects`。省掉它，本量具会稳定多报一条**与本件无关的红**
    —— 09-02 第一版就是这样报出「入场 1253 passed; 1 failed」的，那 1 红是**量具造的**。
    `.claude/devbox/gate` 的最后一行逐字做了同一件事，本量具照抄它。
    """
    script = 'mkdir -p "$HOME/.claude/projects" && ' + script
    cmd = [
        "docker", "run", "--rm", "--network", "host",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}",
        "-e", "HOME=/home/zbl",
        "-w", str(WT),
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c", script,
    ]
    return subprocess.run(cmd, capture_output=True, text=True).stdout


def census(base_rev: str = "HEAD"):
    """`monitor` lib 的**格数**：入场（`base_rev` 那两份文件）↔ 交回（盘上这份）各现打一次。

    ⚠ 这是**两次真跑**，不是「盘上 N 减 1」——「新增几格」那个数被推错过太多次。
    量的是 `cargo test --lib` 的 `test result:` 那一行（`SELFTEST_BAR_RE` 同口径：
    核判定行，不是看它等不等于 0）。
    """
    pat = re.compile(r"test result: (\w+)\. (\d+) passed; (\d+) failed")

    def read_one(label):
        out = in_box("cd src-tauri && cargo test --lib 2>&1 | tail -5")
        m = pat.search(out)
        if not m:
            raise RuntimeError(f"{label}：抓不到判定行 —— 按 CRASH 记，不许报「新红 0」。\n{out[-2000:]}")
        print(f"  {label}: {m.group(0)}")
        return int(m.group(2)), int(m.group(3))

    after = read_one("交回（盘上这份）")
    originals = {p: p.read_text(encoding="utf-8") for p in (CCM, JUDGE)}
    keep = {p: md5(p) for p in (CCM, JUDGE)}
    try:
        for p in (CCM, JUDGE):
            rel = p.relative_to(WT)
            blob = subprocess.run(["git", "-C", str(WT), "show", f"{base_rev}:{rel}"],
                                  capture_output=True, text=True, check=True).stdout
            p.write_text(blob, encoding="utf-8")
        before = read_one(f"入场（{base_rev} 那两份）")
    finally:
        for p, s in originals.items():
            p.write_text(s, encoding="utf-8")
        for p in originals:
            if md5(p) != keep[p]:
                raise RuntimeError(f"🔴 census 恢复失败：{p} 的 md5 对不上！")
    print(f"\n  入场 {before[0]} passed / {before[1]} failed"
          f"  →  交回 {after[0]} passed / {after[1]} failed"
          f"  ⇒ 新增 {after[0] - before[0]} 格")
    print("  分母 = `monitor` 这一个包的 `--lib` 测试（不是门禁那个「8 个包合计」）。")


def run_judges() -> tuple[dict, str]:
    """在沙箱里跑一次那两条判据。返回 ({测试名: ok/FAILED}, 原始输出)。"""
    out = in_box(f"cd src-tauri && cargo test --lib {FILTER} 2>&1")
    verdict = {}
    for name in (MINE, PRIOR):
        m = re.search(rf"^test .*::{re.escape(name)} \.\.\. (ok|FAILED)$", out, re.M)
        verdict[name] = m.group(1) if m else "编不过/没跑到"
    return verdict, out


def panic_for(out: str, name: str) -> str:
    """**按测试名归属** panic 报文 —— 两条判据同时红时，别把先例那条的报文记到本件头上。

    `cargo test` 的失败详情印在 `---- <全名> stdout ----` 那一段里；不按名切段就会
    抓到输出里第一条 panic（哪条先跑就是哪条），那是一次静默的错归属。
    """
    m = re.search(rf"---- \S*{re.escape(name)} stdout ----\n(.*?)(?=\n---- |\nfailures:)",
                  out, re.S)
    if not m:
        e = re.search(r"^error(\[E\d+\])?: ([^\n]*)", out, re.M)
        return ("编译错：" + e.group(2)) if e else "（本条没有失败详情段）"
    body = re.search(r"panicked at [^\n]*\n((?:.*\n){0,3})", m.group(1))
    txt = (body.group(1) if body else m.group(1)).strip().splitlines()
    return " ⏎ ".join(t.strip() for t in txt[:2])[:220]


def main():
    if "--census" in sys.argv:
        print("═══ 格数普查（入场 ↔ 交回，两次真跑）═══")
        census()
        return
    only = set(sys.argv[1:])
    cuts = [c for c in build_cuts() if not only or c.cid in only]
    base_md5 = {p: md5(p) for p in (CCM, JUDGE)}

    print("═══ 改前基线（干净树）═══")
    base, out0 = run_judges()
    print(f"  {MINE}: {base[MINE]}")
    print(f"  {PRIOR}: {base[PRIOR]}")
    print(f"  md5 ccm={base_md5[CCM]}  payload.rs={base_md5[JUDGE]}")

    rows = []
    for c in cuts:
        originals = {p: p.read_text(encoding="utf-8") for p in {e[0] for e in c.edits}}
        try:
            for path, old, new, want in c.edits:
                src = path.read_text(encoding="utf-8")
                hit = src.count(old)
                if hit != want:
                    raise RuntimeError(f"{c.cid}: 锚点在 {path.name} 命中 {hit} 处，期望 {want}")
                path.write_text(src.replace(old, new, 1), encoding="utf-8")
            # 「变异已落地」不是口号：逐处核**原串真的少了一处**（brief 第 7 条）。
            for path, old, new, want in c.edits:
                left = path.read_text(encoding="utf-8").count(old)
                if left != want - 1:
                    raise RuntimeError(f"{c.cid}: 变异没落地，{path.name} 里原串仍有 {left} 处")
            print(f"\n─── {c.cid}（{c.dod}）变异已落地：{c.what}")
            v, out = run_judges()
            why = {n: (panic_for(out, n) if v[n] != "ok" else "") for n in (MINE, PRIOR)}
            rows.append((c, v, why))
            print(f"    本件判据 = {v[MINE]}    先例判据 = {v[PRIOR]}")
            for n in (MINE, PRIOR):
                if why[n]:
                    tag = "本件" if n == MINE else "先例"
                    print(f"    {tag}炸在：{why[n]}")
        finally:
            for p, s in originals.items():
                p.write_text(s, encoding="utf-8")
            for p in originals:
                if md5(p) != base_md5[p]:
                    raise RuntimeError(f"🔴 {c.cid} 恢复失败：{p} 的 md5 对不上，盘上留着变异！")

    print("\n═══ 变异表 ═══")
    print("| 刀 | DoD | 锚点命中 | 变异 | 本件判据 | 本件炸在哪一格 | 先例判据 | 期望 |")
    print("|---|---|---|---|---|---|---|---|")
    for c, v, why in rows:
        anchors = " + ".join(f"{e[3]} 处" for e in c.edits)
        print(f"| {c.cid} | {c.dod} | {anchors} | {c.what} | "
              f"**{v[MINE]}** | {why[MINE][:90] or '—'} | {v[PRIOR]} | {c.expect} |")
    print(f"\n分母 = 本量具跑到的测试共 2 条（`{MINE}` · `{PRIOR}`），不是全仓。")
    print(f"收工核 md5：ccm={md5(CCM)}  payload.rs={md5(JUDGE)}")


if __name__ == "__main__":
    main()
