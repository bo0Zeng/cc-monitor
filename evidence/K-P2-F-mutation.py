#!/usr/bin/env python3
"""K-P2 `F` 拍的变异台（死值验）—— 每一刀：先断言锚点命中数，再落地，再跑判据，再逐字节还原。

用法（**在宿主上跑，测试一律进沙箱容器**）：
    python3 evidence/K-P2-F-mutation.py <被测工作树的绝对路径> [只跑这几个 id …]

住址与射程（`brief` 第 12 条：量具住址要能唯一定位到那一份 + 被测对象指向哪棵树）：
  · 量具住址 = `<被测树>/evidence/K-P2-F-mutation.py`（**跟着被测树走**，不住临时目录
    ⇒ 不会被别的 agent 同名覆盖成「指向另一棵树」的版本）；
  · 被测对象 = **argv[1] 那棵树**，不是 `__file__` 的那棵。两者可以不同，
    每一行输出都把它印出来。

为什么每一刀都要「锚点命中数」这一栏（`brief` 第 7 / 第 128 行）：
  不记锚点，下一轮谁也复不出这一刀；而复不出的数只能悬着。

为什么还原要比 md5 而不是「再改回去」：
  「改回去了」和「改回去但差一个字节」在终端上一模一样。

⚠ 测试**只在沙箱里跑**（`ccmon-devbox:latest`）——〔用 08-29〕「以后所有开发测试都不允许
  直接在本机跑」。本脚本自己不跑测试，它 `docker run`。
⚠ 本脚本**不是**门禁。门禁的唯一合法跑法是 `.claude/devbox/gate`；这里跑的是**单条判据**，
  为的是让一刀一刀的读数便宜到能真打十几刀。输出里的数**不许当门禁读数引用**。
"""

import hashlib
import pathlib
import subprocess
import sys

IMAGE = "ccmon-devbox:latest"
PROJ = "/home/zbl/文档/claudecode-frontend"
CARGO_REG = "ccmon-cargo-registry"


def sandbox(tree: pathlib.Path, workdir: str, script: str, target_tag: str | None):
    """在沙箱里跑一条命令。`workdir` 是相对被测树的子目录。"""
    args = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-e", "HOME=/home/zbl",
    ]
    if target_tag:
        args += [
            "-v", f"{CARGO_REG}:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{target_tag}",
        ]
    args += ["-w", f"{tree}/{workdir}".rstrip("/"), IMAGE, "bash", "-o", "pipefail", "-c", script]
    p = subprocess.run(args, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


# ── 判定行的口径（**一处**，不在每一刀里各写一遍）──────────────────────────────
def cargo_verdict(out: str) -> str:
    """`cargo test` 的判定行 —— 取 `test result:` 那一行。"""
    for ln in out.splitlines():
        if ln.startswith("test result:"):
            return ln.strip()
    return "<抓不到判定行 —— 按 CRASH 记>"


def e2e_verdict(out: str) -> str:
    """e2e 套件的判定行 —— 取 `合计 PASS=` 那一行。"""
    for ln in out.splitlines():
        if "合计 PASS=" in ln:
            return ln.strip()
    return "<抓不到判定行 —— 按 CRASH 记>"


CARGO = ("src-tauri", "cargo test --lib {filter} 2>&1", cargo_verdict, "k-p2f")
E2E_CLI = (".", "mkdir -p $HOME/.claude/projects && bash e2e/ccm-cli.test.sh 2>&1", e2e_verdict, None)


# ── 刀表 ──────────────────────────────────────────────────────────────────────
# 每一刀：(id, 挨刀的文件, 锚点, 换成什么, 锚点该命中几次, 跑哪条判据, 说这一刀打的是哪一格)
#
# ⚠ 「换上去的东西类型契约还成立吗」（`brief` 第 7 条）：下面每一刀换上去的都是**形状对、
#   恒答其中一张脸**的东西 —— shell 仍是合法 shell、Rust 仍是合法 Rust。
#   台子炸了那不是读数是 CRASH，判定行会说。
CUTS = [
    (
        "F1", "shared/ccm",
        'if [ "$do_print" = 1 ]; then\n    seq="{ tmux new-session',
        'if [ "$do_print" != 1 ]; then\n    seq="{ tmux new-session',
        1, ("cargo", "the_local_launch_recipe_is_reachable_only_from_print"),
        "把那段本机编排从 `--print` 那一支翻到 exec 那一支 = **把退路加回来**",
    ),
    (
        "F2", "shared/ccm",
        "CCM_RC_NO_BACKEND=4",
        "CCM_RC_NO_BACKEND=2",
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "退出码与 `die` 撞（调用方从此分不开「你敲错了」与「没有后端」）",
    ),
    (
        # ⚠ 〔`F` 拍后半〕`backend_unreachable "$why"` 今天**有两处**（账号 · 建会话）
        #   ⇒ 锚点要带上下文才唯一。这一刀切的是**建会话**那一处
        #   （它上面那句注释是 `launch_via_daemon` 独有的）。
        "F3", "shared/ccm",
        '  #      `ccm_cli_contract::BACKEND_BACKED_PATHS`。〕\n  backend_unreachable "$why"',
        '  #      `ccm_cli_contract::BACKEND_BACKED_PATHS`。〕\n  return 1',
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "建会话那条腿**不再走唯一失败面**（③b/③c：调用点数与登记的无退路条数对不上）",
    ),
    (
        "F3b", "shared/ccm",
        '  # ★★ 〔`F` 拍〕**这里原来是「降级出声 ＋ 读 manifest」，退路删了 ⇒ 唯一失败面。**',
        '  return 0\n  # ★★ 〔`F` 拍〕**这里原来是「降级出声 ＋ 读 manifest」，退路删了 ⇒ 唯一失败面。**',
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "**账号**那条腿不再走唯一失败面（同上，另一条腿 —— 证 F3 不是只逮一处）",
    ),
    (
        "F3c", "shared/ccm",
        '      backend_unreachable "在 tmux 里起有身份的 agent',
        '      die "在 tmux 里起有身份的 agent',
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "身份前置检查那一处**退回自己一套说法**（③c：调用点总数 3→2）"
        "—— 它不在 `BACKEND_BACKED_PATHS` 里，只有 ③c 逮得到",
    ),
    (
        "F4", "shared/ccm",
        '"$_ccm_db" --launch',
        '"$_ccm_db" --launchZ',
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "成对判（`KP2C ③`）：把新路摘掉 ⇒ 「失败面唯一」不许再算满足",
    ),
    (
        "F5", "shared/ccm",
        "    seq=\":\"\n  fi",
        "    _ccm_launched=0\n    seq=\":\"\n  fi",
        1, ("cargo", "every_backend_backed_path_in_ccm_keeps_an_observable_fallback"),
        "`NoFallback.gone` 那一格：旧退路的锚点又回到生产段里",
    ),
    (
        "F6", "src-tauri/src/ccm_cli_contract.rs",
        'gone: "_ccm_launched=",\n            },',
        'gone: "_ccm_launched=",\n            },\n        ),\n        (\n            "--resolve",\n            FallbackShape::Speaks {\n                anchor: "eval \\"$DAEMON_BIN_RECIPE\\"",\n                says: "账号解析已降级",\n            },',
        1, ("cargo", "every_backend_backed_path_in_ccm_keeps_an_observable_fallback"),
        "第二条棘轮 ⑤：多登记一条**有退路**的路 ⇒ 有退路的条数 2→3（⑤ 该红）",
    ),
    (
        "F6b", "src-tauri/src/ccm_cli_contract.rs",
        'gone: "_ccm_launched=",',
        'gone: "这个串在生产段里当然找不到",',
        1, ("cargo", "every_backend_backed_path_in_ccm_keeps_an_observable_fallback"),
        "`gone` **锚点烂掉** ⇒ 「不该在」那半条本会空真。本轮补的靶子自检该逮住它",
    ),
    (
        "F13", "shared/ccm",
        '  _ccm_acct_tab="$(daemon_out_to_table "$out")"',
        '  _ccm_acct_src=file\n  _ccm_acct_tab="$(daemon_out_to_table "$out")"',
        1, ("cargo", "every_backend_backed_path_in_ccm_keeps_an_observable_fallback"),
        "**账号**那条的 `NoFallback.gone`（`_ccm_acct_src=file`）又回到生产段里 —— "
        "证 F5 那一刀不是只对建会话那一条成立",
    ),
    (
        "F14", "shared/ccm",
        '  if [ "$do_print" = 1 ]; then\n    seq="{ tmux new-session',
        '  if [ "$do_print" = 1 ] || [ "${CCM_FORCE_LOCAL:-}" = 1 ]; then\n    seq="{ tmux new-session',
        1, ("cargo", "the_local_launch_recipe_is_reachable_only_from_print"),
        "🔴 **用一个环境变量把退路塞回来**（`§6-3` 逐字排除过这种买法：「一个环境变量就能"
        "走回本地 ⇒ 旧住址还在。**不许这么买**」）—— 它含着 `[ \"$do_print\" = 1 ]`、也不含 "
        "`!=` ⇒ **上一版的子串判会放它过去**；本轮改成逐字钉整条守卫才逮得到〔自查第二处〕",
    ),
    (
        "F7", "shared/ccm",
        'printf "$NAME_TAKEN_FMT" "$tmux_name" >&2; exit 3',
        'printf "$NAME_TAKEN_FMT" "$tmux_name" >&2\n      exit 3',
        1, ("cargo", "an_explicit_tmux_name_collision_fails_loudly"),
        "撞名的一个报出口不再响亮（`exit 3` 拆到下一行 ⇒ 逐行核那条读成「没有非零码」）",
    ),
    (
        "F8", "shared/ccm",
        "NAME_TAKEN_FMT='ccm: tmux 会话名",
        "NAME_TAKEN_FMT='ccm: tmux 会话名 %s 被别人占了\\n'\nNAME_TAKEN_FMT='ccm: tmux 会话名",
        1, ("cargo", "an_explicit_tmux_name_collision_fails_loudly"),
        "撞名文案长出**第二份定义**（本条上一版的病）",
    ),
    (
        "F9", "shared/ccm",
        "printf 'ccm: 后端不可达",
        "printf 'ccm: 后端不可达（第二份措辞）\\n' >&2\n  printf 'ccm: 后端不可达",
        1, ("cargo", "the_backend_unreachable_failure_face_has_exactly_one_home"),
        "唯一文案长出**第二份**（失败面最容易长成两份，那是本条的正题）",
    ),
    (
        "F10", "shared/ccm",
        'if ! launch_via_daemon "$tmux_name" "$payload" "$cwd" "$ccm_sid" "$agent" "$tmux_size"; then',
        "if false; then",
        1, ("cargo", "the_local_launch_recipe_is_reachable_only_from_print"),
        "**非空对照**：把 exec 那一支掏空 ⇒ 「那一支里不许有 new-session」变空真，"
        "本条最后那条断言该逮住它（`§21 裁二` 那把 `if false; then`）",
    ),
    (
        "F12", "shared/ccm",
        '    seq=":"',
        '    [ 1 = 2 ] && tmux new-session -d -s never\n    seq=":"',
        1, ("cargo", "the_local_launch_recipe_is_reachable_only_from_print"),
        "把 `new-session` 塞进 exec 那一支里、**且在内层 `fi` 之后** —— "
        "上一版取「第一个 `fi`」的段界读法在这里会停早，看不见它（本轮自查逮到的那一格）",
    ),
    (
        "F11", "shared/ccm",
        "  # ⚠ **删掉的那个状态变量叫 `_ccm_launched=`**",
        "  # ⚠ **删掉的那个状态变量**",
        1, ("cargo", "every_backend_backed_path_in_ccm_keeps_an_observable_fallback"),
        "把注释里那个名字删掉 ⇒ `gone` 的靶子自检该红（证 F6b 那条自检不是靠别的东西过的）",
    ),
]


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    tree = pathlib.Path(sys.argv[1]).resolve()
    only = set(sys.argv[2:])
    print(f"# 被测树：{tree}")
    print(f"# 量具：  {pathlib.Path(__file__).resolve()}")
    sha = subprocess.run(["git", "-C", str(tree), "rev-parse", "HEAD"],
                         capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "-C", str(tree), "status", "--porcelain"],
                           capture_output=True, text=True).stdout.strip()
    print(f"# 量于提交：{sha}（`git status` {len(dirty.splitlines())} 条）")
    print()
    print("| 刀 | 挨刀处 | 锚点命中 | 判定行 | 打的是哪一格 |")
    print("|---|---|---|---|---|")
    rows = []
    for cid, rel, anchor, repl, want, (kind, filt), why in CUTS:
        if only and cid not in only:
            continue
        p = tree / rel
        orig = p.read_bytes()
        md5_before = hashlib.md5(orig).hexdigest()
        text = orig.decode()
        hits = text.count(anchor)
        if hits != want:
            rows.append((cid, rel, f"{hits}（要 {want}）", "**锚点不命中 ⇒ 这一刀没打**", why))
            continue
        p.write_bytes(text.replace(anchor, repl, 1).encode())
        assert p.read_bytes() != orig, "变异没落地"
        print(f"# [{cid}] 变异已落地（锚点命中 {hits}/{want}）", file=sys.stderr)
        if kind == "cargo":
            wd, tmpl, verdict, tag = CARGO
            rc, out = sandbox(tree, wd, tmpl.format(filter=filt), tag)
        else:
            wd, tmpl, verdict, tag = E2E_CLI
            rc, out = sandbox(tree, wd, tmpl, tag)
        line = verdict(out)
        p.write_bytes(orig)
        md5_after = hashlib.md5(p.read_bytes()).hexdigest()
        assert md5_after == md5_before, f"{cid} 还原不逐字节！{md5_before} != {md5_after}"
        rows.append((cid, rel, f"{hits}/{want}", f"rc={rc} · {line}", why))
    for r in rows:
        print("| " + " | ".join(r) + " |")
    print()
    # ⚠ 条数**现算**（`brief` 13b：报一个基数也是复述 ⇒ 别写死）。
    print(f"⚠ 还原对拍：每一刀跑完立刻比 md5，不相等就 `assert` 当场炸 —— "
          f"上面能打印出这一行就说明这 **{len(rows)}** 刀全部逐字节还原。")


if __name__ == "__main__":
    main()
