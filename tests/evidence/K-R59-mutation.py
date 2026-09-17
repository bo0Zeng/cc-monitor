#!/usr/bin/env python3
"""K-R59 变异台（实现方，09-11）。

住址唯一：`evidence/K-R59-mutation.py`，被测对象**只指本工作树** `.claude/worktrees/k-r59`
（下面 `WT` 是从本文件位置推出来的，不是写死的路径 —— 复制到别的树上跑，量的就是那棵树）。

用法：`python3 evidence/K-R59-mutation.py [刀号 …]`（不给刀号 = 全跑）。
每一刀：断言锚点**恰好命中 1 次** → 落刀 → 打印「变异已落地」→ 在**沙箱**里跑被测套件
→ 记读数 → `git checkout --` 还原。

⚠ 沙箱：`box` 与 `.claude/devbox/gate` 同一套挂载（`--network none`、不挂 `~/.cc-monitor`、
不挂宿主 tmux）。**只编译不跑的才许在宿主**（`DECISIONS.md#R21`）。
"""
import json
import pathlib
import subprocess
import sys

WT = pathlib.Path(__file__).resolve().parent.parent
PROJ = WT.parent.parent.parent
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"

# 🔴 **驱动一律是 Python，不许留一个 `.sh`。**〔`K-R61` 09-11 在同一处踩过，提交 `b9a5800`
# 逐字「变异台驱动改用 Python（shell_lint_registry 现打逮到）」；本件 09-11 现打复现了一次：
# `evidence/K-R59-box.sh` 一落盘，`shell_lint_registry::every_shell_script_is_either_linted_or_
# registered_as_exempt` 当场点名它「既不在 CI 的 shellcheck 表达式里、也没登记豁免」。〕


def box_argv(cmd: str) -> list:
    """沙箱跑手 —— 与 `.claude/devbox/gate` **同一套挂载**，只换末尾那条命令。

    挂：项目目录 · 那条硬写的 skill 路径（只读）· cargo registry 具名卷。
    不挂：宿主 tmux · `~/.cc-monitor` · `$HOME` 其余部分 · 网络（`--network none`）。
    """
    return [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/k-r59",
        "-e", "HOME=/home/zbl",
        "-e", "PB_WS=backend-consolidation",
        "-w", str(WT), "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        f'mkdir -p "$HOME/.claude/projects" && {cmd}',
    ]

CUTS = [
    # (刀号, 一句话, 文件, 锚点逐字, 换成什么, 跑什么, 怎么读)
    ("M1", "KR59D1① 把字段塞回落盘清单（名字那一侧·最小面 1 行）",
     "src/remote-config.ts", '  "jump",\n', '  "jump",\n  "daemonless",\n', "cargo:f07_main_path_tests"),
    ("M2", "KR59D1④ 把界面那一格塞回（名字那一侧·最小面 1 行）",
     "src/settings/machine-card.ts", "  private jumpInput!: HTMLInputElement;\n",
     "  private jumpInput!: HTMLInputElement;\n  private daemonlessInput!: HTMLInputElement;\n",
     "cargo:f07_main_path_tests"),
    ("M3", "🔴 KR59D1⑤ 把本机那条豁免塞回（**不含 daemonless 字样**·最小面 1 行）",
     "src/settings/readiness.ts", '  if (facet === "connection") return true;',
     '  if (facet === "daemon" || facet === "connection") return true;', "vitest:readiness"),
    ("M3b", "🔴 KR59D1⑤ 把界面那处写死的 `na` 塞回（同一档的第 ⑥ 处·最小面 1 行）",
     "src/settings/remote-section.ts", "    renderStatusCells(strip, readStatus(LOCAL_MACHINE_KEY));",
     '    renderStatusCells(strip, readStatus(LOCAL_MACHINE_KEY), {\n      daemon: { kind: "na", detail: "不需要", at: 0 },\n    });',
     "vitest:readiness"),
    ("M4", "KR59D1 删过界：摘掉 `connect_and_exec_cmd` 的一个现有消费者（改走函数项，编得过）",
     "src-tauri/src/mcp.rs", "        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, CMD).await?;",
     "        let f = crate::ssh_source::connect_and_exec_cmd;\n        let stream = f(cfg, CMD).await?;",
     "cargo:exec_site_registry"),
    ("M6", "🔴 KR59D2 死值验：把那条换人手续**整条删掉**，别的都不动",
     "src-tauri/src/backend/control/launch_wire.rs",
     "    fn the_ts_fallback_renderer_now_stands_on_its_own_consumers() {",
     "    fn a_name_that_is_not_the_tombstone() {", "cargo:f07_main_path_tests"),
    ("M7", "🔴 KR59D3 死值验：把那条**指名的告知**去掉（computeGaps 那一支·最小面 1 块）",
     "src/settings/readiness.ts",
     '      if (facet === "daemon" && input.legacyNoBackend?.(origin)) {',
     '      if (false && facet === "daemon" && input.legacyNoBackend?.(origin)) {',
     "vitest:readiness"),
    ("M8", "KR59D3 另一半：把「认出旧配置」那一步掏空（签名留着·返回同型空值）",
     "src/remote-config.ts", "  return raw\n    .filter((h) => h[LEGACY_NO_BACKEND_KEY] === true)",
     "  return ([] as Record<string, unknown>[])\n    .filter((h) => h[LEGACY_NO_BACKEND_KEY] === true)",
     "vitest:readiness"),
    ("M1b", "KR59D1① 同一刀量在 TS 那一侧：字段塞回落盘清单 ⇒ 保存又会把旧键写出去",
     "src/remote-config.ts", '  "jump",\n', '  "jump",\n  "daemonless",\n', "vitest:migration"),
    ("M12", "K-R59 翻面那条的牙：让扇出**排掉一台**（原判据断的正是「一台都不排」）",
     "src/session-accounts-poll.ts", "  const per = await mapWithLimit(hosts, limit, (h) => oneHost(h, f));",
     "  const per = await mapWithLimit(hosts.filter((_, i) => i !== 1), limit, (h) => oneHost(h, f));",
     "vitest:poll"),
    ("M13", "K-R59 翻面那条的牙：把 `deriveUi` 那一支 `hidden` 塞回（`accounts.rs` 那条串的下游）",
     "src/accounts.ts", '    if (e.includes("过旧") || e.includes("不支持账号")) {',
     '    if (e.includes("daemonless")) return { kind: "hidden", reason: e } as AccountsUi;\n'
     '    if (e.includes("过旧") || e.includes("不支持账号")) {', "vitest:accounts"),
    ("M11", "🔴 KR59D3 产品面：把那条告知的**名字**从 DOM 上摘掉（最小面 1 行）",
     "src/settings/remote-section.ts", "      if (g.code) li.dataset.code = g.code;",
     "      if (false && g.code) li.dataset.code = g.code;", "vitest:readiness"),
    ("M9", "7u 把实现掏空①：`noteLocalBackend` 只留签名（本机 daemon 的唯一写点）",
     "src/settings/remote-section.ts", "  private async noteLocalBackend(): Promise<void> {\n    try {",
     "  private async noteLocalBackend(): Promise<void> {\n    if (1 > 0) return;\n    try {",
     "vitest:readiness"),
    ("M10", "7u 把实现掏空②：`TS_FALLBACK_KEEPERS` 那一半的比对掏空（表留着，不再比）",
     "src-tauri/src/backend/control/launch_wire.rs",
     "        for (symbol, file, want, what, unlock) in TS_FALLBACK_KEEPERS {",
     "        for (symbol, file, want, what, unlock) in TS_FALLBACK_KEEPERS.iter().take(0) {",
     "cargo:f07_main_path_tests"),
]

SUITES = {
    "cargo:f07_main_path_tests":
        "cd src-tauri && cargo test -p monitor --lib f07_main_path_tests 2>&1 | tail -25",
    "cargo:exec_site_registry":
        "cd src-tauri && cargo test -p monitor --lib exec_site_registry 2>&1 | tail -25",
    "vitest:migration":
        "npx vitest run src/settings/remote-section.vitest.ts src/remote-config.vitest.ts 2>&1 | tail -40",
    "vitest:poll":
        "npx vitest run src/session-accounts-poll.vitest.ts 2>&1 | tail -30",
    "vitest:accounts":
        "npx vitest run src/accounts.vitest.ts src/account-chip.vitest.ts 2>&1 | tail -30",
    "vitest:readiness":
        "npx vitest run src/settings/readiness.vitest.ts src/settings/remote-section.vitest.ts "
        "src/settings/accounts-section.vitest.ts src/remote-config.vitest.ts 2>&1 | tail -40",
}


def sandbox(cmd: str) -> str:
    return subprocess.run(box_argv(cmd), capture_output=True, text=True).stdout


def main() -> None:
    argv = sys.argv[1:]
    # `--box '<命令>'`：只借沙箱跑一条命令（死值验之外的现打都走它，不再另立一个 `.sh`）。
    if argv[:1] == ["--box"]:
        print(sandbox(argv[1]))
        return
    want = set(argv)
    out = []
    for cid, why, rel, anchor, repl, suite in CUTS:
        if want and cid not in want:
            continue
        p = WT / rel
        src = p.read_text(encoding="utf-8")
        n = src.count(anchor)
        assert n == 1, f"{cid}：锚点在 {rel} 命中 {n} 次（要 1 次）——这一刀不许落"
        print(f"[{cid}] 锚点命中 1 次（{rel}）")
        p.write_text(src.replace(anchor, repl), encoding="utf-8")
        print(f"[{cid}] **变异已落地**：{why}")
        try:
            log = sandbox(SUITES[suite])
        finally:
            subprocess.run(["git", "checkout", "--", rel], cwd=WT, check=True)
        out.append({"刀": cid, "说的是": why, "切在": rel, "锚点命中": 1,
                    "跑的": suite, "输出尾": log.strip().splitlines()[-14:]})
        print(f"[{cid}] 已还原\n" + "\n".join(log.strip().splitlines()[-14:]) + "\n")
    (WT / "evidence" / "K-R59-mutation.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
