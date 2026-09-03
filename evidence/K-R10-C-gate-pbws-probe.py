#!/usr/bin/env python3
"""K-R10 · C 拍量具 —— 收 §3 那三格（a / b / c）。

住址（唯一，只属于 K-R10 的 C 拍）：
    cc-monitor/evidence/K-R10-C-gate-pbws-probe.py
⚠ 它原先落在 `.claude/planned-build/backend-consolidation/evidence/`，被那个仓的
  `.gitignore`（`*` + 只白名单 `*.md`）**静默挡住** —— 盘上有、版本控制里没有，
  而 `git status` 对它一个字不说。PM 09-01 挪来这里（`K-G5` 的量具也在这个目录）。
  🔴 根因是 PM 给的写区里**没有 `evidence/`**，实现方只能落进计划仓。裁定见
  `.claude/planned-build/backend-consolidation/features/K-R10-…md` 的 `§0b` 六。
被测对象指向哪棵树（写死，别让它跟着 cwd 漂）：
    /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r10   （分支 track/k-r10）
被测的那份文件：
    <上面那棵树>/scripts/gate.sh          —— **原样跑，一个字节都不改**

# 怎么跑

    python3 <本文件>            # 跑五格，逐格打 PASS/FAIL，末尾一行合计
    python3 <本文件> --raw a    # 把某一格的容器内完整输出原样吐出来

退出码：全格 PASS = 0；有 FAIL = 1。

# 它在哪儿跑 —— 🔴 宿主上零测试

本文件自己**不跑任何被测代码**：它只组装 `docker run`，被测的 `gate.sh` 在
`ccmon-devbox:latest` 容器里跑。走的是**直接 `docker run`**，不是 `.claude/devbox/gate`
那个包装（包装里 `bash scripts/gate.sh` 写死、且不透传 `PB_WS`，覆盖不了本量具要拧的两个旋钮）。
`K-R10 §0a 二` 已裁这条路合规，并要求「绕开包装时把挂载逐条写出来」⇒ 见下面 MOUNTS，
与 `.claude/devbox/gate:71-80` 逐条对齐，差异也在那里逐条写明。

# ⚠⚠ 桩的射程 —— 这是本量具最该被人怀疑的一格，先自己写出来

gate.sh 一共五格：cargo · generated · daemon · npm · pb check。
本量具**只测第 5 格**（`pb check`），前四格用 PATH 上的桩（假 `cargo` / 假 `npm`）打成绿：
  · 桩住在**容器内**的 `$HOME/.k-r10-stub-bin`（HOME 没挂宿主 ⇒ 随 `--rm` 一起消失，宿主零残留）；
  · `generated` 那格没有桩，跑的是真 `git diff`（工作树干净 ⇒ 真绿）。
⇒ **本量具打出来的 `GATE: OK` 不是一次真门禁读数**，它只说明「第 5 格没红」。
真门禁读数一律以 `.claude/devbox/gate` 那一趟为准，本文件替代不了它。
桩的好处是：`gate.sh` 本体**没有被改过**，第 5 格执行的是盘上那份的逐字原文。

# 三格判据（照 `K-R10 §0a 五`，PM 一字不改的那三条）

(a) 误指到一个「绿的别人」必须红。
    `PB_WS=devbench`（盘上现成不绿：`FAIL=1 BROKEN=1`），被测树是 k-r10 ⇒ 门禁必须红，
    且红因里必须点名 `pb check`。
    改之前它一定 PASS 不了：gate.sh 硬写 `control-parity`（绿）⇒ 无论 `PB_WS` 给谁都绿。
    ★ 不需要故意造 FAIL —— 盘上现成就有不绿的工作区。

(b) 那行绿必须自证它查了谁。两格，互为对照：
    b1  `PB_WS=issue-triage`   ⇒ 绿，且 `ok ... pb check` 那行**逐字含** `issue-triage`，
        **且不含** `control-parity`。
    b2  `PB_WS=control-parity` ⇒ 绿，且那行**逐字含** `control-parity`，**且不含** `issue-triage`。
    ⚠⚠ **b1 第一版用的是 `backend-consolidation`，09-01 当场翻车，换掉的理由写在这儿**：
      本工作区是**活的** —— 实现方把交回写进件文件那一刻，`pb check` 就报
      `FAIL=1 [J3 陈账] INDEX.md 比源文件旧`，b1 当场从绿变红，**而红因与被测的 gate.sh 无关**。
      ⇒ 判据不许挂在一个**会被自己这次交回改变颜色**的东西上。
      两格现在都用**已收官**的工作区（`control-parity` / `issue-triage`），
      它们不会因为本拍写了什么而变色。
    ⚠ b1/b2 成对是承重的：单独一格用「行里有这个名字」判，一个写死的名字就能骗过去；
      两格换着给名字、并各自断言**不含对方**，写死的名字必然在其中一格露馅。
    ⚠ 反过来的坑（brief §12「别让路径/目录名混进断言」）在这里**不适用而必须说明**：
      这两格断的就是「那行有没有报出自己查了谁」—— 名字进这一行**正是本件要买的东西**，
      不是恒真的路径噪音。恒真的风险由「不含对方」那半格挡住。

(c) 不给工作区名 ⇒ 红，不许回落默认。两格：
    c1  `PB_WS` **不存在**（`env -u`）⇒ 必须红，红因点名 `pb check`。
    c2  `PB_WS=""`（存在但空）⇒ 必须红。空串与不存在是两条不同的路，各收一格。

# 🔴 验收纪律（`K-R10 §0a 五` 末，逐字）

这五格**必须先在改 gate.sh 之前跑一遍确认它们是红的**。哪一格开局就绿，那一格就是仪式。
本量具因此把「改前基线」当成一等公民：`--baseline` 只是给读数打个标签，判据完全一样。
"""

import argparse
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
WT = PROJ + "/.claude/worktrees/k-r10"
TAG = "k-r10-c1"
IMAGE = "ccmon-devbox:latest"

# 挂载逐条写出来（`K-R10 §0a 二` 的口径），与 `.claude/devbox/gate:71-80` 对齐：
#   -v $PROJ:$PROJ                     ← 同（工作树 · 计划仓 · pm-targets 都在里面）
#   -v $SKILL:$SKILL:ro                ← 同（gate.sh:161 硬写要它）
#   -e HOME=/home/zbl                  ← 同
#   -w $WT                             ← 同
#   --network host                     ← 包装默认 none；本量具不编译、不取 crates，两者都跑得通，
#                                        取 host 是为了与红线「一律 DEVBOX_NET=host」同口径。
# 差异（本量具**有意**不带的两条，各写理由）：
#   -v ccmon-cargo-registry:/opt/rust/cargo/registry   不带 —— cargo 被桩掉了，用不着 registry。
#   -e CARGO_TARGET_DIR=…/pm-targets/$TAG              不带 —— 同上，没有一行 Rust 会被编译。
#                                                      ⇒ 本量具**一个字节都不写** pm-targets。
MOUNTS = [
    "-v", f"{PROJ}:{PROJ}",
    "-v", f"{SKILL}:{SKILL}:ro",
    "-e", "HOME=/home/zbl",
    "-w", WT,
]

# 桩：住容器内 $HOME 下（HOME 没挂宿主）⇒ --rm 之后宿主零残留。
STUB = r"""
set -uo pipefail
B="$HOME/.k-r10-stub-bin"
mkdir -p "$B" "$HOME/.claude/projects"
cat > "$B/cargo" <<'STUBEOF'
#!/usr/bin/env bash
# 桩 cargo：只吐 gate.sh 的 run_gate_sum / run_gate 读得懂的 8 行，不编译任何东西。
for i in 1 2 3 4 5 6 7 8; do echo "test result: ok. 1 passed; 0 failed"; done
exit 0
STUBEOF
cat > "$B/npm" <<'STUBEOF'
#!/usr/bin/env bash
echo "Tests  1 passed (1)"
exit 0
STUBEOF
chmod +x "$B/cargo" "$B/npm"
export PATH="$B:$PATH"
echo "### PB_WS=[${PB_WS-<unset>}]"
bash scripts/gate.sh
echo "### GATE_RC=$?"
"""


def run_cell(env):
    """跑一格。env 是 dict；值为 None 表示这个变量**不存在**（env -u 那一路）。"""
    cmd = ["docker", "run", "--rm", "--network", "host"] + MOUNTS
    for k, v in env.items():
        if v is not None:
            cmd += ["-e", f"{k}={v}"]
    cmd += [IMAGE, "bash", "-c", STUB]
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.stdout + p.stderr


def pb_line(out):
    """gate.sh 打的那行 `  ok   pb check ...`；没有就返回 None。"""
    for ln in out.splitlines():
        s = ln.strip()
        if s.startswith("ok") and "pb check" in s:
            return ln
    return None


def gate_verdict(out):
    """('OK'|'FAIL'|None, 那一整行)。"""
    for ln in out.splitlines():
        if ln.startswith("GATE: OK"):
            return "OK", ln
        if ln.startswith("GATE: FAIL"):
            return "FAIL", ln
    return None, ""


def must_red_on_pb(out):
    v, ln = gate_verdict(out)
    if v is None:
        return False, "读不到 GATE 裁决行（CRASH，不许当成任何一色）"
    if v != "FAIL":
        return False, f"门禁是绿的，而本格要求它红 —— {ln}"
    if "pb check" not in ln:
        return False, f"红了，但红因里没有 pb check（红错了地方，不算）—— {ln}"
    return True, ln


def must_green_named(out, want, forbid):
    v, gl = gate_verdict(out)
    if v is None:
        return False, "读不到 GATE 裁决行（CRASH）"
    if v != "OK":
        return False, f"本格要求绿，实测红 —— {gl}"
    ln = pb_line(out)
    if ln is None:
        return False, "找不到 `ok ... pb check` 那一行"
    if want not in ln:
        return False, f"那行绿没报出它查了谁（缺 `{want}`）——{ln.strip()}"
    if forbid in ln:
        return False, f"那行里出现了别人的名字 `{forbid}` —— {ln.strip()}"
    return True, ln.strip()


CELLS = {
    "a": ("误指到一个「绿的别人」必须红（PB_WS=devbench，被测树 k-r10）",
          {"PB_WS": "devbench"},
          must_red_on_pb),
    "b1": ("那行绿必须逐字含 issue-triage、且不含 control-parity",
           {"PB_WS": "issue-triage"},
           lambda o: must_green_named(o, "issue-triage", "control-parity")),
    "b2": ("那行绿必须逐字含 control-parity、且不含 issue-triage",
           {"PB_WS": "control-parity"},
           lambda o: must_green_named(o, "control-parity", "issue-triage")),
    "c1": ("PB_WS 不存在 ⇒ 红，不许回落默认",
           {},
           must_red_on_pb),
    "c2": ("PB_WS 是空串 ⇒ 红，不许回落默认",
           {"PB_WS": ""},
           must_red_on_pb),
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--raw", metavar="CELL", help="把某一格的容器内完整输出原样吐出来")
    ap.add_argument("--label", default="", help="给这一趟读数打个标签（如 baseline / after）")
    a = ap.parse_args()

    if a.raw:
        print(run_cell(CELLS[a.raw][1]))
        return 0

    print(f"K-R10 · C 拍五格   被测树={WT}   标签={a.label or '<无>'}")
    print(f"被测文件=scripts/gate.sh（原样跑）  镜像={IMAGE}\n")
    npass = 0
    for cid, (desc, env, judge) in CELLS.items():
        out = run_cell(env)
        ok, why = judge(out)
        npass += ok
        print(f"[{'PASS' if ok else 'FAIL'}] {cid}  {desc}")
        print(f"        {why.strip()}\n")
    print(f"===== K-R10 C 五格：PASS={npass}/{len(CELLS)} =====")
    return 0 if npass == len(CELLS) else 1


if __name__ == "__main__":
    sys.exit(main())
