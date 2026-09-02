#!/usr/bin/env python3
"""K-G3 `C1` 的死值验量具（09-01）—— 门⑥（`ccm` e2e）那一格的**可复跑凭据**。

本件治的是 `丙1-f1`：`scripts/gate.sh` 头注自称「出货前的**唯一闸门**」，而
`grep -c ccm scripts/gate.sh` = **0**（PM 08-24 独立复核，本拍在 `b28464e` 上复打，仍是 0）。
本拍给它加了门⑥（两套 `ccm` e2e，走 `e2e/assert-pass-floor.sh`）。
这份量具就是「那一格真有牙」的证据台。

── 被测对象指向哪棵树（`brief` `5k`）─────────────────────────────────────────
只测**它自己所在的那棵工作树**：树根由 `git rev-parse --show-toplevel` 在**本文件所在目录**
现算，不接受任何外部路径参数 ⇒ 被复制到别处也不会还指着原来那棵树。
每次运行先印「树根 · HEAD · 被切文件 md5 · 未提交改动处数」—— 读数与这四样是一套，别拆开引用。
落地那一刻：工作树 `.claude/worktrees/k-g3`、分支 `track/k-g3`、基线尖 `b28464e`，
本树**未铺** `src-tauri/embedded-daemons/`（cargo 1308 · daemon 490 · npm 1480 是这种配置下的数）。

── 为什么切 `shared/ccm`（本件 `§2` 写着「不碰 shared/ccm 本身」）───────────
`§2` 禁的是**留下改动**；`KG31` 的 DoD 逐字要求「造一处 `shared/ccm` 的真缺陷，证明新加
那道门当场红」。所以这里的每一刀都是**切完就 `git checkout` 还原并核 md5**，
盘上不留一个字节。`restore` 之后看不到 md5 回基线，就是这一刀没收干净。

── 两刀都不是自造的坏味道，是**盘上原文点名过的那个回归** ─────────────────
· `C1` 把 `set-titles-string` 换回 `#T` —— `e2e/ccm-rbind-title.sh` 头注逐字记的
  2026-07-31 真机事故（claude 把 pane 标题写成状态文本 ⇒ marker 被冲掉 ⇒ 约 1/5 命中）。
· `C2` 把通道 A 的 `@ccm_sid_expect` 换成通道 B 的 `@ccm_sid` —— `shared/ccm:968` 那段
  `F04` 注释逐字禁止的那件事（「意图」被写成「事实」⇒ 破坏性动作会杀错会话）。

── 用法 ────────────────────────────────────────────────────────────────────
  kg3-c1-cuts.py readings    本拍的真实读数（写死在这里，供下一轮对账）
  kg3-c1-cuts.py anchors     逐刀核锚点在当前文件里恰好命中几次（只读；`brief` 第 7 条）
  kg3-c1-cuts.py cut C1      把一刀真的落进工作树（要求 `shared/ccm` 干净）
  kg3-c1-cuts.py gate        在沙箱里跑整道门禁，印每一格 + 最后那行裁决 + 退出码
  kg3-c1-cuts.py oldgate     把 `scripts/gate.sh` 临时换成 `b28464e` 那一版再跑门禁
                             （`brief` `7u`：把实现退掉，看这一刀还红不红）
  kg3-c1-cuts.py suites      只跑门⑥那两套（不跑 cargo/npm），拿逐套读数与秒数
  kg3-c1-cuts.py restore     还原被切的文件并核 md5

一刀的标准跑法：`anchors` -> `cut C1` -> `gate` -> `oldgate` -> `restore`。
⚠ `cut` / `oldgate` 会**改工作树里的文件**；两者都在收尾核 md5，核不回去就是没收干净。
⚠ 宿主上零测试：`gate` / `oldgate` / `suites` 一律 `docker run` 进 `ccmon-devbox:latest`，
   挂载与 `.claude/devbox/gate` 逐条同形（那个脚本 55+ 棵树共用、且不在任何 git 仓里，不许改）。
"""

import hashlib
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
IMAGE = "ccmon-devbox:latest"
TAG = "k-g3-c1"
CARGO_CACHE = "ccmon-cargo-registry"
BASELINE_TIP = "b28464e"

TARGET_REL = "shared/ccm"
GATE_REL = "scripts/gate.sh"

HERE = pathlib.Path(__file__).resolve().parent


def git(*args, root=None):
    return subprocess.run(
        ["git", "-C", str(root or HERE), *args], capture_output=True, text=True
    ).stdout.strip()


ROOT = pathlib.Path(git("rev-parse", "--show-toplevel"))
TARGET = ROOT / TARGET_REL
GATE = ROOT / GATE_REL


def md5(p):
    return hashlib.md5(pathlib.Path(p).read_bytes()).hexdigest()


def banner():
    dirty = [ln for ln in git("status", "--porcelain", root=ROOT).splitlines() if ln]
    print(f"· 树根   {ROOT}")
    print(f"· HEAD   {git('rev-parse', 'HEAD', root=ROOT)}  分支 {git('rev-parse', '--abbrev-ref', 'HEAD', root=ROOT)}")
    print(f"· 被切件 {TARGET_REL}  md5 {md5(TARGET)}")
    print(f"· 门禁件 {GATE_REL}  md5 {md5(GATE)}")
    print(f"· git    {len(dirty)} 处未提交改动")


# 每一刀：id -> (说明, 锚点原文, 换上去的东西, 锚点该命中几次, 该打红哪一格)
CUTS = {
    "C1": (
        "窗口标题退回 `#T`（= 2026-07-31 真机事故的原形）⇒ marker 被 pane 标题冲掉",
        "tmux set-option set-titles-string '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}'",
        "tmux set-option set-titles-string '#T'",
        1,
        "ccm e2e/ccm-rbind-title",
    ),
    "C2": (
        "通道 A 写成通道 B（`@ccm_sid_expect` -> `@ccm_sid`）⇒ 「意图」被当成「事实」",
        '@ccm_sid_expect $(sq "$ccm_sid")',
        '@ccm_sid $(sq "$ccm_sid")',
        1,
        "ccm e2e/ccm-print-parity",
    ),
    # ── 下面两刀是**最小面**的那两刀（`brief` 第 9 条）───────────────────────
    #
    # C1 / C2 都是**粗刀**：实测它们连 `cargo` 门一起打红（C1 经
    # `polling_registry::the_identity_poller_is_gone_for_good`、C2 经
    # `ccm_cli_contract::CHANNEL_A_LITERALS`）⇒ 它们证明不了门⑥买到了什么。
    # C3 / C4 刻意**等长替换**（同字节数），理由现打：`polling_registry.rs:735` 那条
    # 反空真自检是 `prod.len()*4 > raw.len()`，本树余量只有 **97 字节**
    # ⇒ 任何改动 `shared/ccm` 长度的刀都可能顺手把 `cargo` 打红，那不是门⑥的牙。
    "C3": (
        "窗口标题 marker 拼错一个字母（`ccm-rbind-` -> `ccm-rbnid-`，等长）⇒ monitor 扫不到、绑不上",
        "ccm-rbind-#{@ccm_sid}",
        "ccm-rbnid-#{@ccm_sid}",
        1,
        "ccm e2e/ccm-rbind-title（且只有它）",
    ),
    "C4": (
        "`new-session` 的 `-c` 写成 `-C`（等长）⇒ 会话的工作目录整个不传了",
        '-c $(sq "$cwd")',
        '-C $(sq "$cwd")',
        1,
        "ccm e2e/ccm-print-parity（且只有它）",
    ),
}


def docker_argv(workdir, script):
    """与 `.claude/devbox/gate` 的 `docker run` 逐条同形（网络默认 host，好让第一趟能取 crates）。"""
    return [
        "docker", "run", "--rm",
        "--network", "host",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}",
        "-e", "HOME=/home/zbl",
        "-e", "PB_WS=backend-consolidation",
        "-w", str(workdir),
        IMAGE,
        "bash", "-o", "pipefail", "-c", script,
    ]


def run_gate(label):
    t0 = time.time()
    p = subprocess.run(
        docker_argv(ROOT, 'mkdir -p "$HOME/.claude/projects" && bash scripts/gate.sh'),
        # ⚠ `errors="replace"` 不是装饰：门⑥红的那一趟会把套件的原始输出透出来，
        #   里面有 tmux `capture-pane` 抓来的**非 UTF-8 字节**（第一趟实测 `0xef` 断在中途）。
        #   不写它，量具会在**恰恰是要读的那一趟**上崩掉 —— 而崩掉与「门没跑」在终端上一模一样。
        capture_output=True, text=True, errors="replace",
    )
    dt = time.time() - t0
    print(f"===== {label} =====")
    for ln in p.stdout.splitlines():
        if ln.startswith(("  ok", "  分母", "GATE:", "  FAIL")) or "别提交" in ln:
            print(ln)
    print(f"--- 退出码 {p.returncode}  墙钟 {dt:.1f}s")
    return p.returncode, p.stdout


def cmd_anchors():
    banner()
    src = TARGET.read_text(encoding="utf-8")
    print("--- 锚点（只读）---")
    for cid, (why, old, _new, want, grid) in CUTS.items():
        hit = src.count(old)
        flag = "OK " if hit == want else "!! "
        print(f"{flag}{cid}  命中 {hit} 次（应当 {want}）  该打红：{grid}")
        print(f"      {why}")


def cmd_cut(ids):
    banner()
    dirty = [ln for ln in git("status", "--porcelain", root=ROOT).splitlines() if TARGET_REL in ln]
    if dirty:
        sys.exit(f"拒切：{TARGET_REL} 不干净 —— 先 restore。{dirty}")
    src = TARGET.read_text(encoding="utf-8")
    for cid in ids:
        why, old, new, want, grid = CUTS[cid]
        hit = src.count(old)
        if hit != want:
            sys.exit(f"拒切 {cid}：锚点命中 {hit} 次，应当 {want}")
        src = src.replace(old, new)
        print(f"变异已落地 {cid}（锚点命中 {hit} 次）：{why}  ⇒ 该打红 {grid}")
    TARGET.write_text(src, encoding="utf-8")
    print(f"· 切后 md5 {md5(TARGET)}")


def cmd_restore():
    git("checkout", "--", TARGET_REL, root=ROOT)
    banner()
    dirty = [ln for ln in git("status", "--porcelain", root=ROOT).splitlines() if TARGET_REL in ln]
    print("还原干净" if not dirty else f"!! 还没干净：{dirty}")


def cmd_oldgate():
    """`brief` 7u：把本拍对 `gate.sh` 的实现整个退掉，看这一刀还红不红。"""
    banner()
    keep = pathlib.Path(tempfile.mkdtemp(prefix="kg3c1-")) / "gate.sh.mine"
    shutil.copy2(GATE, keep)
    mine = md5(GATE)
    old = subprocess.run(
        ["git", "-C", str(ROOT), "show", f"{BASELINE_TIP}:{GATE_REL}"],
        capture_output=True, text=True,
    ).stdout
    GATE.write_text(old, encoding="utf-8")
    print(f"· 已把 {GATE_REL} 换成 {BASELINE_TIP} 那一版  md5 {md5(GATE)}")
    try:
        rc, _ = run_gate(f"7u 对照：{BASELINE_TIP} 的 gate.sh（本拍实现整个退掉）")
    finally:
        shutil.copy2(keep, GATE)
        back = md5(GATE)
        print(f"· 已还原本拍那一版  md5 {back}  {'一致' if back == mine else '!! 对不上'}")
    return rc


def cmd_suites():
    banner()
    script = (
        'mkdir -p "$HOME/.claude/projects"\n'
        'for pair in "ccm-print-parity 12" "ccm-rbind-title 8"; do\n'
        '  set -- $pair\n'
        '  st=$(date +%s.%N)\n'
        '  out=$(bash e2e/assert-pass-floor.sh "$1" "$2" 2>&1); rc=$?\n'
        '  en=$(date +%s.%N)\n'
        '  n=$(printf "%s" "$out" | grep -oE "合计 PASS=[0-9]+" | grep -oE "[0-9]+" | tail -1)\n'
        '  echo "$1  地板=$2  实得PASS=${n:-<抓不到>}  rc=$rc  秒=$(echo "$en - $st" | bc)"\n'
        'done\n'
    )
    p = subprocess.run(docker_argv(ROOT, script), capture_output=True, text=True, errors="replace")
    print(p.stdout.strip() or p.stderr.strip())
    print(f"--- 退出码 {p.returncode}")


READINGS = """\
K-G3 `C1` 本拍真实读数（09-01，全部量于工作树 .claude/worktrees/k-g3，基线尖 b28464e，
沙箱 ccmon-devbox:latest，CARGO_TARGET_DIR=.claude/pm-targets/k-g3-c1，PB_WS=backend-consolidation）

【入场读数】
  grep -c ccm scripts/gate.sh          = 0    （治前；PM 08-24 那条读数今天仍成立）
  grep -c -- --target scripts/gate.sh  = 0
  grep -c fmt scripts/gate.sh          = 0
  门禁基线                              GATE: OK  墙钟 148s（冷 target）/ 43s（热 target）
  cargo 1308（8 包）· daemon 490 · npm 1480 · pb check FAIL=0 BROKEN=0

【门⑥ 逐套代价（沙箱内单独计时，分母 = 上面那个墙钟）】
  ccm-print-parity   PASS=12  地板 12  rc=0  1.28s
  ccm-rbind-title    PASS=8   地板 8   rc=0  0.28s
  ⇒ 合计 +1.56s / 148s 冷 ≈ +1.1%；/ 43s 热 ≈ +3.6%

【没挂的那四套（在只多装了 jq 的探针镜像上现打，官方镜像里它们跑不起来）】
  ccm-cli            PASS=126 FAIL=0  rc=0   7.16s   官方镜像缺 jq ⇒ 今天挂不上
  ccm-contract-parity PASS=68 FAIL=0  rc=0   5.60s   同上
  ccm-acceptance     PASS=28  FAIL=1  rc=1  37.68s   加了 jq 也仍红 1 条（沙箱 HOME 几乎是空的）
  ccm-pretrust       PASS=14  FAIL=1  rc=1  35.56s   同上
  官方镜像里直接跑：ccm-acceptance / ccm-contract-parity 打「需要 jq」rc=1（fail-closed，不假绿）；
  ccm-pretrust 在无 jq 下 PASS=7 FAIL=8 rc=1。

【变异表（每刀都：锚点命中 1 次；门禁全量跑于沙箱；两列退出码 = 本拍 gate.sh / b28464e gate.sh）】
  刀  等长?  本拍门禁                                        b28464e 门禁     判读
  C1  否    rc=1  cargo 101 + ccm-rbind-title PASS=4/8       rc=1（cargo 101） 粗刀，证明不了门⑥
  C2  否    rc=1  cargo 101 + ccm-print-parity PASS=11/12    未单跑           粗刀，同上
  C3  是    rc=1  只有 ccm-rbind-title PASS=4/8              rc=0 GATE: OK    ★最小面 + 7u 真空
  C4  是    rc=1  只有 ccm-print-parity PASS=10/12           rc=0 GATE: OK    ★最小面 + 7u 真空
  基线（无刀）    rc=0 GATE: OK  cargo 1308 · daemon 490 · npm 1480 · 两套 e2e 12/8

  ⚠ C1 打红 cargo 的**根因不是它坏了 ccm 的行为**，是 `polling_registry.rs:735` 那条
    反空真自检 `prod.len()*4 > raw.len()`：本树余量只有 **97 字节**（现打
    raw=91299 / prod=22849 / prod*4=91396），C1 让 raw 少 35 字节而 prod 也少 35
    ⇒ 4 倍那侧掉 140，当场翻负。**往 shared/ccm 加 97 字节纯注释就能把 cargo 打红，
    而它的报文说的是「剥法坏了，本条在空转」—— 指错方向。** 这条不在本件写区里，交回 PM。
"""


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    cmd = sys.argv[1]
    if cmd == "readings":
        print(READINGS)
    elif cmd == "anchors":
        cmd_anchors()
    elif cmd == "cut":
        cmd_cut(sys.argv[2:] or sys.exit("要给刀号，如 cut C1"))
    elif cmd == "gate":
        banner()
        sys.exit(run_gate("门禁（当前工作树状态）")[0])
    elif cmd == "oldgate":
        sys.exit(cmd_oldgate())
    elif cmd == "suites":
        cmd_suites()
    elif cmd == "restore":
        cmd_restore()
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
