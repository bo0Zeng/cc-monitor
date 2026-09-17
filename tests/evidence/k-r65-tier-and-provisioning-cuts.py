#!/usr/bin/env python3
"""K-R65 变异台 —— 三条 dod 的死值验，一份可复跑的量具。

# 住址与被测对象（纪律 12：量具住址要能唯一定位到那一份）

  · 量具本体：本文件（`evidence/k-r65-tier-and-provisioning-cuts.py`，随 `track/k-r65` 入库）
  · 被测对象：**本文件所在的那棵工作树**（`WT` 由本文件的位置现算，不写死路径 ——
    写死了，谁把它拷到另一棵树上跑出来的就是另一棵树的数，而输出长得一模一样）
  · 跑法：`python3 evidence/k-r65-tier-and-provisioning-cuts.py`（在任意目录都行）
  · 台子：`ccmon-devbox:latest` 沙箱（`DECISIONS.md#R21`：会执行被测代码的一律进沙箱）
    - Rust 那半：`cargo test -p monitor --lib`
    - 前端那半：`npx vitest run src/settings/config-surface-section.vitest.ts
                             src/settings/readiness.vitest.ts`
      ⚠ 只跑这两份 —— 本件动的前端面就这两份，**别把这两份的分母读成全量 npm**
      （全量归门禁那一格）。

# 形状照 `evidence/k-r63-claim-vs-reality-cuts.py`（本仓已有先例，不另发明一套）

  每一刀都先断言锚点**恰好命中 N 次**再落刀，落刀后打印「变异已落地」，
  跑完**原样还原**并逐字节核一遍还原对不对（纪律 ⑬ / 第 7 条）。
  ★ 有两刀的锚点在全文出现 **2 次**（同形代码一处在生产、一处在判据里）⇒
    它们带 `window`，锚点只在那个窗口里数。**第一趟就是在这里切错的**：
    不带窗口时 `assert 命中==1` 当场炸，那正是这条纪律要买的东西。

# ⚠ 它买不到什么（如实写明，别把射程读大一格）

  · Rust 那半只跑 `-p monitor` 一个包 —— 别的包的红它看不见（门禁那一格才是全量）；
  · 「新红」这一栏是**这一刀下红了哪几条**，不是「这条判据只可能被这一刀打红」；
  · 最后那一组（`7u`）是**把本件的实现整处掏空**（签名留着、恒答一张脸），
    它回答的是「实现退掉之后还有多少条新断言仍绿」，**不是**一条死值验。
"""

import hashlib
import subprocess
import sys
from pathlib import Path

WT = Path(__file__).resolve().parent.parent
PROJ = WT.parent.parent.parent  # .claude/worktrees/<tag> ⇒ 项目根
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
TARGET = PROJ / ".claude/pm-targets" / WT.name


def devbox(inner: str) -> str:
    """在沙箱里跑一条命令，回它的合并输出。挂载与 `.claude/devbox/gate` 逐条相同。"""
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGET}",
        "-e", "HOME=/home/zbl",
        "-w", str(WT),
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        f'mkdir -p "$HOME/.claude/projects" && {inner}',
    ]
    return subprocess.run(cmd, capture_output=True, text=True).stdout


def strip_ansi(s: str) -> str:
    out, i = [], 0
    while i < len(s):
        if s[i] == "\x1b" and i + 1 < len(s) and s[i + 1] == "[":
            j = i + 2
            while j < len(s) and not s[j].isalpha():
                j += 1
            i = j + 1
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


def run_cargo() -> list[str]:
    raw = devbox("cd src-tauri && cargo test -p monitor --lib 2>&1 | tail -40")
    lines = []
    infail = False
    for ln in raw.splitlines():
        if ln.strip() == "failures:":
            infail = True
            continue
        if ln.startswith("test result"):
            infail = False
            lines.append(ln.strip())
        elif infail and ln.startswith("    ") and "::" in ln:
            lines.append("红: " + ln.strip())
    return lines or ["（没读到 test result —— 按 CRASH 记，别当读数）"]


def run_npm() -> list[str]:
    raw = strip_ansi(
        devbox(
            "npx vitest run src/settings/config-surface-section.vitest.ts "
            "src/settings/readiness.vitest.ts --reporter=verbose 2>&1"
        )
    )
    lines = []
    for ln in raw.splitlines():
        t = ln.strip()
        if t.startswith("×"):
            lines.append("红: " + t.rsplit(" ", 1)[0])
        elif t.startswith("Tests "):
            lines.append(t)
    return lines or ["（没读到 Tests 那一行 —— 按 CRASH 记，别当读数）"]


class Cut:
    """一刀：`(题目, 相对路径, 锚点逐字, 换成什么, 应命中几次, 跑哪半, 锚点窗口)`。

    `window` 是 `(起, 止)` 两个逐字串；给了它，锚点只在那一段里数与替换 ——
    同形代码同时住在生产与判据里时**必须给**，否则数出来是 2。
    """

    def __init__(self, title, rel, anchor, repl, hits=1, half="cargo", window=None):
        self.title, self.rel, self.anchor, self.repl = title, rel, anchor, repl
        self.hits, self.half, self.window = hits, half, window


def apply(cuts: list[Cut]) -> dict:
    """落刀。回 `{路径: (原文, 原文 md5)}` 供还原与逐字节核对。

    🔴 **一份文件只存一次原文，而且要在这一组的任何一刀落下去之前存** ——
    第一版是「每刀各存一份」：同一组里第二刀读到的已经是被第一刀改过的内容，
    于是它把**变异体**当成了「原文」，还原之后树上留着残渣，
    而那句 `已还原: True` 照样印出来 —— **一句自信的假读数**，正是本仓在治的那族病。
    〔09-11 现打：F4 与 7u 两组各留了残渣，是 `git status` 逮到的，不是这句话逮到的。〕
    """
    saved: dict = {}
    for c in cuts:
        p = WT / c.rel
        if p not in saved:
            o = p.read_text(encoding="utf-8")
            saved[p] = (o, hashlib.md5(o.encode()).hexdigest())
        s = p.read_text(encoding="utf-8")
        lo, hi = 0, len(s)
        if c.window:
            lo = s.index(c.window[0])
            hi = s.index(c.window[1], lo)
        n = s.count(c.anchor, lo, hi)
        print(f"   文件 {c.rel} · 锚点命中 {n} 次（应为 {c.hits}）")
        assert n == c.hits, f"锚点命中 {n} 次而不是 {c.hits} —— 先查锚点，别改断言"
        p.write_text(s[:lo] + s[lo:hi].replace(c.anchor, c.repl) + s[hi:], encoding="utf-8")
    print("   变异已落地： True")
    return saved


def restore(saved: dict, baseline_dirty: str):
    """还原，并**用两把互相独立的尺子**核它：逐字节 md5 ＋ `git status --porcelain`。

    ⚠ 只用第一把是不够的 —— 存错了原文时它会拿变异体核变异体，恒真。
    第二把不经过本脚本存的任何东西，问的是 git：**树回到开跑那一刻了吗**。
    """
    ok = True
    for p, (orig, md5) in saved.items():
        p.write_text(orig, encoding="utf-8")
        ok = ok and hashlib.md5(p.read_text(encoding="utf-8").encode()).hexdigest() == md5
    now = subprocess.run(
        ["git", "-C", str(WT), "status", "--porcelain"], capture_output=True, text=True
    ).stdout
    clean = now.strip() == baseline_dirty.strip()
    print(f"   已还原（逐字节回原文 {ok} · git 树回到开跑那一刻 {clean}）")
    assert ok and clean, f"还原对不上 —— 停下来手工查，别接着跑。git 现状：\n{now}"


TR = "src-tauri/src/tool_registry.rs"
CS = "src-tauri/src/config_surface.rs"
FE = "src/settings/config-surface-section.ts"
RD = "src/settings/readiness.ts"
AQ = "remote-daemon-proto/src/sidecars/codepicture/acquire.rs"

# ── 一刀一组：`(题目, [Cut, …])` ──────────────────────────────────────────
SINGLES = [
    ("M1 KR65D1：把「查了、确认没有」抹成「查不动」（最小面）", [Cut(
        "M1", CS,
        "                Some(false) => (None, SurfaceState::Absent),",
        '                Some(false) => (\n'
        '                    None,\n'
        '                    SurfaceState::Undetermined {\n'
        '                        why: "M1".to_string(),\n'
        '                    },\n'
        '                ),',
    )]),
    ("M2 KR65D1：`observe_unmanaged` 整支掏空（＝退回 K-R60 那一版「我们压根没去查」）", [Cut(
        "M2", CS,
        "    match probe {\n        // `PATH` 上的裸命令",
        '    let _ = (named, probe, env);\n'
        '    return (\n'
        '        None,\n'
        '        SurfaceState::Undetermined { why: "M2".to_string() },\n'
        '    );\n'
        '    #[allow(unreachable_code)]\n'
        '    match probe {\n        // `PATH` 上的裸命令',
    )]),
    ("M3 KR65D1 失效方向：8 处探测全掐掉（＝把「不查」换个名字回来）", [Cut(
        "M3", TR,
        "        probe: EnvProbe::OnPath,",
        '        probe: EnvProbe::CannotProbe { why: "M3 —— 探测掐掉，只留一个档名" },',
        hits=8,
    )]),
    ("M4 KR65D2：把 `cc-acct-iso-local` 标成「不该我们装」⇒ 必须红", [Cut(
        "M4", TR,
        "        who: Provisioning::AppShips,",
        "        who: Provisioning::UserProvides,",
        window=('        id: "cc-acct-iso-local",', '        host: HostScope::Client,'),
    )]),
    ("M5 KR65D2：两格合回去（没装口 ⇒ 直接当成「不该我们装」）", [Cut(
        "M5", TR,
        "            (Provisioning::AppShips, false) => EnvTier::AppShipsNoInstallerYet,",
        "            (Provisioning::AppShips, false) => EnvTier::AppOnlyChecks,",
    )]),
    ("M6 KR65D3 死值验一：把 `cc-bus` 从「app 自带」里摘掉，别处不动", [Cut(
        "M6", TR,
        "            let who = Provisioning::of_tool(t);",
        '            let who = if t.id == "cc-bus" {\n'
        "                Provisioning::NotAnInstall\n"
        "            } else {\n"
        "                Provisioning::of_tool(t)\n"
        "            };",
        # ⚠ 这个锚点全文 2 处（另一处在 `nobody_declares_an_installer_…` 判据里）⇒ 必须给窗口
        window=("pub fn environment() -> Vec<EnvEntry> {", "/// ★ **反向登记"),
    )]),
    ("M7a KR65D3 死值验二：动 daemon 那一层的签名（编译不受影响）", [Cut(
        "M7a", AQ,
        "pub fn obtain<O: Origin>(",
        "pub fn obtain<O: Origin + Sized>(",
    )]),
    ("M7b KR65D3：闭集里 `code-picture-sidecar` 那一条整体换掉 `who`", [Cut(
        "M7b", TR,
        "        who: Provisioning::AppShips,",
        "        who: Provisioning::NotAnInstall,",
        window=('        id: "code-picture-sidecar",', '        host: HostScope::Either,'),
    )]),
    ("M7c 【射程自查，不是死值验】签名逐字不动、`obtain` 的函数体掏空", [Cut(
        "M7c", AQ,
        "    // ⚠ 解构而不是 `pin.` 取字段",
        "    return Err(Face::Offline);\n    #[allow(unreachable_code)]\n"
        "    // ⚠ 解构而不是 `pin.` 取字段",
    )]),
    ("M8 让 `Provisioning::UserProvides` 没有任何使用者", [Cut(
        "M8", TR,
        "        who: Provisioning::UserProvides,",
        "        who: Provisioning::NotAnInstall,",
        hits=9,
    )]),
    ("F1 KR65D1：`promptToInstall` 掏空（恒答 null ⇒ 缺席时不出声）", [Cut(
        "F1", FE,
        '  if (row.tier !== "UserInstallsWePrompt") return null;',
        "  void row;\n  return null;",
        half="npm",
    )]),
    ("F2 KR65D1：把「缺」与「查不动」合成一格", [Cut(
        "F2", FE,
        '    case "undetermined":\n      return "unknown";',
        '    case "undetermined":\n      return "missing";',
        half="npm",
    )]),
    ("F3 KR65D2：把「欠装口」那一格的计数掏空（恒答 null）", [Cut(
        "F3", FE,
        '  const owed = rows.filter((r) => r.tier === "AppShipsNoInstallerYet");',
        "  const owed: SurfaceRow[] = [];\n  void rows;",
        half="npm",
    )]),
    ("F4 「缺」/「未测过」不再只有一个住址（describeGap 自抄一份 ＋ GAP_HEAD 改字）", [
        Cut("F4a", RD, "  const head = GAP_HEAD[g.kind];",
            '  const head = g.kind === "missing" ? "缺" : "未测过";', half="npm"),
        Cut("F4b", RD, '  missing: "缺",', '  missing: "缺件",', half="npm"),
    ]),
    ("F5 KR65D1：把「只有那一档才劝人去装」那条闸拆掉", [Cut(
        "F5", FE,
        '  if (row.tier !== "UserInstallsWePrompt") return null;',
        "  // F5：闸拆掉，四档都劝人去装",
        half="npm",
    )]),
]

# ── `7u`：把本件的实现**整处掏空**（签名留着、恒答一张脸），两半一起量 ──────
SEVEN_U = [
    Cut("of_tool", TR, "        match t.destination {\n            ToolDestination::NotInstalledByUs { .. } => Provisioning::NotAnInstall,\n            _ => Provisioning::AppShips,\n        }",
        "        let _ = t;\n        Provisioning::NotAnInstall"),
    Cut("EnvTier::of", TR, "        match (who, has_installer_today) {\n            (Provisioning::AppShips, true) => EnvTier::AppInstalls,",
        "        let _ = (who, has_installer_today);\n        return EnvTier::AppOnlyChecks;\n        #[allow(unreachable_code)]\n        match (who, has_installer_today) {\n            (Provisioning::AppShips, true) => EnvTier::AppInstalls,"),
    Cut("observe_unmanaged", CS, "    match probe {\n        // `PATH` 上的裸命令",
        '    let _ = (named, probe, env);\n    return (None, SurfaceState::Undetermined { why: "7u".to_string() });\n    #[allow(unreachable_code)]\n    match probe {\n        // `PATH` 上的裸命令'),
    Cut("gapKindOfState", FE, "  switch (st?.kind) {\n    case \"present\":\n      return null;",
        '  void st;\n  return "unknown";\n  switch (st?.kind) {\n    case "present":\n      return null;', half="npm"),
    Cut("promptToInstall", FE, '  if (row.tier !== "UserInstallsWePrompt") return null;',
        "  void row;\n  return null;", half="npm"),
    Cut("summarizeOwedInstallers", FE, '  const owed = rows.filter((r) => r.tier === "AppShipsNoInstallerYet");',
        "  const owed: SurfaceRow[] = [];\n  void rows;", half="npm"),
    Cut("describeUndo", FE, "  switch (row.tier) {\n    case \"AppInstalls\":",
        '  return "cc-monitor 不装这一项（尚未支持部署，或本来就不该由它装），也就无所谓撤销";\n  switch (row.tier) {\n    case "AppInstalls":', half="npm"),
]


def main() -> int:
    out = []

    def say(s=""):
        print(s)
        out.append(s)

    say(f"被测对象：{WT}")
    say(f"分支尖：{subprocess.run(['git', '-C', str(WT), 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()}")
    dirty = subprocess.run(["git", "-C", str(WT), "status", "--porcelain"], capture_output=True, text=True).stdout
    say(f"开跑前 git status --porcelain：{'（空）' if not dirty.strip() else dirty.strip()}")
    say()
    say("== 基线（不落任何刀）==")
    for ln in run_cargo():
        say("  | " + ln)
    for ln in run_npm():
        say("  | " + ln)

    for title, cuts in SINGLES:
        say()
        say(f"== [{title}]")
        saved = apply(cuts)
        for ln in (run_cargo() if cuts[0].half == "cargo" else run_npm()):
            say("  | " + ln)
        restore(saved, dirty)
        out.append("   已还原（逐字节回原文 True · git 树回到开跑那一刻 True）")

    say()
    say("== [7u：把本件实现整处掏空（签名留着、恒答一张脸）—— 不是死值验，是「还有多少条新断言仍绿」]")
    saved = apply(SEVEN_U)
    for ln in run_cargo():
        say("  | " + ln)
    for ln in run_npm():
        say("  | " + ln)
    restore(saved, dirty)
    out.append("   已还原（逐字节回原文 True · git 树回到开跑那一刻 True）")

    dst = WT / "evidence/k-r65-tier-and-provisioning-cuts.readings.txt"
    dst.write_text("\n".join(out) + "\n", encoding="utf-8")
    print(f"\n落盘：{dst}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
