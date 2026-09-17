#!/usr/bin/env python3
"""K-R59 `7u`：**把实现整个掏空**，看还有多少条本轮新断言仍绿（09-11，实现方）。

🔴 **不用 `git checkout <基线> -- <生产文件>`**（纪律 ⑱）：本件的判据与实现**同住一份**
（`readiness.ts` 里 `notApplicable` 与那条告知都在被测文件里），整份退回读数会是
「一条都不红」，读起来完全反过来。⇒ **逐处掏空行为、签名留着**，一次全上。

掏空的是**生产行为**，不动测试与判据文件 —— 那才是「实现退掉了、判据还在不在说话」。

住址唯一：`evidence/K-R59-7u-hollow-out.py`；被测对象 = 本文件所在工作树。
用法：`python3 evidence/K-R59-7u-hollow-out.py`
"""
import importlib.util
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("mut", HERE / "K-R59-mutation.py")
mut = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mut)
WT = mut.WT

# (处, 文件, 锚点逐字, 掏成什么)
HOLLOW = [
    ("① `readiness.notApplicable`：本机 daemon 那条豁免塞回（`KR59D1⑤` 的正题）",
     "src/settings/readiness.ts",
     '  if (facet === "connection") return true;',
     '  if (facet === "daemon" || facet === "connection") return true;'),
    ("② `readiness.computeGaps`：那条指名告知掏空（`KR59D3` 的正题）",
     "src/settings/readiness.ts",
     '      if (facet === "daemon" && input.legacyNoBackend?.(origin)) {',
     '      if (false && facet === "daemon" && input.legacyNoBackend?.(origin)) {'),
    ("③ `remote-config.legacyNoBackendHosts`：认旧配置那一步掏空（签名留着，返回同型空值）",
     "src/remote-config.ts",
     "  return raw\n    .filter((h) => h[LEGACY_NO_BACKEND_KEY] === true)",
     "  return ([] as Record<string, unknown>[])\n    .filter((h) => h[LEGACY_NO_BACKEND_KEY] === true)"),
    ("④ `remote-section.noteLocalBackend`：本机 daemon 的唯一写点掏空（签名留着）",
     "src/settings/remote-section.ts",
     "  private async noteLocalBackend(): Promise<void> {\n    try {",
     "  private async noteLocalBackend(): Promise<void> {\n    if (1 > 0) return;\n    try {"),
    ("⑤ `remote-section.buildLocalRow`：那个写死的 `na` 覆盖值塞回（同一档的第 ⑥ 处载体）",
     "src/settings/remote-section.ts",
     "    renderStatusCells(strip, readStatus(LOCAL_MACHINE_KEY));",
     '    renderStatusCells(strip, readStatus(LOCAL_MACHINE_KEY), {\n'
     '      daemon: { kind: "na", detail: "不需要", at: 0 },\n    });'),
    ("⑥ `remote-section.renderGaps`：那条告知的**名字**不再进 DOM",
     "src/settings/remote-section.ts",
     "      if (g.code) li.dataset.code = g.code;",
     "      if (false && g.code) li.dataset.code = g.code;"),
]

SUITE = ("npx vitest run src/settings/readiness.vitest.ts src/settings/remote-section.vitest.ts "
         "src/settings/accounts-section.vitest.ts src/remote-config.vitest.ts "
         "src/session-accounts-poll.vitest.ts src/accounts.vitest.ts src/account-chip.vitest.ts 2>&1")

if __name__ == "__main__":
    touched = sorted({rel for _, rel, _, _ in HOLLOW})
    for why, rel, anchor, repl in HOLLOW:
        p = WT / rel
        s = p.read_text(encoding="utf-8")
        n = s.count(anchor)
        assert n == 1, f"锚点在 {rel} 命中 {n} 次（要 1 次）：{why}"
        print(f"锚点命中 1 次 · {why}")
        p.write_text(s.replace(anchor, repl), encoding="utf-8")
    print(f"**变异已落地**：{len(HOLLOW)} 处生产行为一次全掏空（签名一处没动）")
    try:
        out = subprocess.run(mut.box_argv(SUITE), capture_output=True, text=True).stdout
    finally:
        subprocess.run(["git", "checkout", "--", *touched], cwd=WT, check=True)
        print("已还原")
    for line in out.splitlines():
        if " FAIL " in line or "Tests " in line or "Test Files " in line:
            print(line)
