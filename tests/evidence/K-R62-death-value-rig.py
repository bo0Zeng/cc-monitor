#!/usr/bin/env python3
"""`K-R62` 死值验台 —— **可复跑的量具**，不是一份报告。

用法（在本工作树里）：
    python3 evidence/K-R62-death-value-rig.py A     # 单刀
    python3 evidence/K-R62-death-value-rig.py Z     # 「把实现整个退掉」那一刀（行为版）
    python3 evidence/K-R62-death-value-rig.py all   # 逐刀跑一遍

它做四件事，一件都不省（纪律 ⑬：`K-R55` 的刀 G 就是没切成而看起来像「没红」）：
  ① 逐字锚点，**断言命中恰好 1 次** —— 不是 1 就当场 `AssertionError`，不许当成「没红」；
  ② 落刀之后印 `git diff --stat` 当**「变异已落地」**的凭据；
  ③ 在**沙箱**里跑那一格（`K31`：宿主上不跑测试），把判定行与红了的逐条印出来；
  ④ `finally` 里 `git checkout --` 还原，末尾印 `git status --porcelain`。

⚠ **沙箱路径是会话相关的**（`SBX` 那一行指着当拍会话的 scratchpad）。
   换个会话复跑时把它换成同挂载的 `docker run`，或直接用
   `PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r62` 跑整趟。

⚠ **刀 Z 的形状是有讲究的，别改成 `git checkout <基线> -- <生产文件>`**：
   本仓 Rust 的判据与实现**同住一份文件**（`#[cfg(test)] mod tests`）⇒ 整份退回会把
   判据一起退掉，现打读数是 `1366 passed; 0 failed`（**一条都不红，因为它们不在了**）。
   那是个退化答案，量不出「哪几条断言其实是空的」。⇒ 刀 Z 逐处把**行为**掏空、签名留着。
"""
import subprocess, sys, os, re, pathlib

WT = pathlib.Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r62")
SBX = "/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/sbx"

RUST = "cd src-tauri && cargo test --lib 2>&1"
VITEST = "npx vitest run 2>&1"

CUTS = {}


def cut(name, path, anchor, repl, gate):
    CUTS[name] = (path, anchor, repl, gate)


# ── 刀 A（KR62D1）：本机那条路改用一份**自己的**副本 ─────────────────────────
cut(
    "A", "src-tauri/src/profile_installer.rs",
    "        ProfileFlavor::PosixRc => {\n"
    "            crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)\n"
    "        }\n    }\n}\n",
    "        ProfileFlavor::PosixRc => {\n"
    '            const LOCAL_SNIPPET: &str = "cc()  { ccm \\"$@\\"; }\\ncct() { ccm --tmux \\"$@\\"; }\\n";\n'
    "            crate::sftp::merge_profile_block(existing, LOCAL_SNIPPET, what)\n"
    "        }\n    }\n}\n",
    RUST,
)

# ── 刀 B（KR62D1）：再写一套本机版的 merge（第四套） ─────────────────────────
cut(
    "B", "src-tauri/src/profile_installer.rs",
    "        ProfileFlavor::PosixRc => {\n"
    "            crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)\n"
    "        }\n",
    "        ProfileFlavor::PosixRc => {\n"
    "            let _ = what;\n"
    "            let mut out = existing.to_string();\n"
    "            if !out.is_empty() && !out.ends_with('\\n') {\n"
    "                out.push('\\n');\n"
    "            }\n"
    '            out.push_str("# === cc-monitor local posix BEGIN ===\\n");\n'
    "            out.push_str(crate::sftp::CCM_WRAPPER_SNIPPET.trim());\n"
    '            out.push_str("\\n# === cc-monitor local posix END ===\\n");\n'
    "            Ok(out)\n"
    "        }\n",
    RUST,
)

# ── 刀 C（KR62D1）：档没升 ──────────────────────────────────────────────────
cut(
    "C", "src-tauri/src/tool_registry.rs",
    '        id: "posix-rc-aliases",\n'
    '        display_name: "POSIX rc 里的 ccm 别名块（cc / cct / zcc …）",\n',
    '        id: "posix-rc-aliases-DEAD",\n'
    '        display_name: "POSIX rc 里的 ccm 别名块（cc / cct / zcc …）",\n',
    RUST,
)

# ── 刀 D（KR62D2）：查法换回「只认围栏」 ────────────────────────────────────
cut(
    "D", "src-tauri/src/profile_installer.rs",
    "pub fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {\n"
    "    let mut out = Vec::new();\n"
    "    let mut inside = false;\n",
    "pub fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {\n"
    "    // 【刀 D】只认围栏：没有 BEGIN 就当什么都没有（K-R57 现打的用户 rc 正是这一形）。\n"
    "    if !find_block_version(content).0 {\n"
    "        return Vec::new();\n"
    "    }\n"
    "    let mut out = Vec::new();\n"
    "    let mut inside = false;\n",
    RUST,
)

# ── 刀 E（KR62D2 / K31）：产品自己动手删了那几行 ────────────────────────────
cut(
    "E", "src-tauri/src/profile_installer.rs",
    "    let flavor = flavor_of(path);\n"
    "    let content = std::fs::read_to_string(path).unwrap_or_default();\n",
    "    let flavor = flavor_of(path);\n"
    "    let content = std::fs::read_to_string(path).unwrap_or_default();\n"
    "    // 【刀 E】替用户把那几行删了（K31 明禁）。\n"
    "    if flavor == ProfileFlavor::PosixRc {\n"
    "        let hits = scan_legacy_rc_lines(&content);\n"
    "        if !hits.is_empty() {\n"
    "            let keep: Vec<&str> = content\n"
    "                .lines()\n"
    "                .enumerate()\n"
    "                .filter(|(i, _)| !hits.iter().any(|h| h.line_no == i + 1))\n"
    "                .map(|(_, l)| l)\n"
    "                .collect();\n"
    '            let _ = std::fs::write(path, keep.join("\\n"));\n'
    "        }\n"
    "    }\n",
    RUST,
)

# ── 刀 F（KR62D3）：本件新加的那条路不进那张账 ──────────────────────────────
cut(
    "F", "src-tauri/src/fenced_block.rs",
    '        id: "local-posix-block",\n',
    '        id: "local-posix-block-NOT-IN-LEDGER",\n',
    RUST,
)

# ── 刀 G（KR62D3）：围栏抄一份字面量，不指那个常量 ──────────────────────────
cut(
    "G", "src-tauri/src/fenced_block.rs",
    "        // 与远端那一套**同一个常量**：本机与远端装进 rc 的是同一个东西（`K15` / `K36`）。\n"
    "        begin_marker: crate::sftp::CCM_PROFILE_BEGIN,\n",
    '        begin_marker: "# === cc-monitor local posix BEGIN ===",\n',
    RUST,
)

# ── 刀 H（前端）：默认那一档也去读用户的文件 ────────────────────────────────
cut(
    "H", "src/launcher-diagnostics.ts",
    "    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;\n",
    "    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;\n"
    "    // 【刀 H】挂上去就去扫一遍（默认那一档也发 IPC）。\n"
    "    void commands.cc_integration_scan_path({\n"
    '      path: rcSel.value || r.rcCandidates[0]?.path || "",\n'
    '      commandName: "cc",\n'
    "    });\n",
    VITEST,
)

# ── 刀 I（前端）：装失败被吞掉 ──────────────────────────────────────────────
cut(
    "I", "src/launcher-diagnostics.ts",
    "      rcStatus.textContent = `${verb}失败：${String(e)}`;\n",
    "      rcStatus.textContent = `${verb}完成`;\n",
    VITEST,
)

# ── 刀 J（前端）：那段「这几行是旧的」被改写而不是原样上屏 ──────────────────
cut(
    "J", "src/launcher-diagnostics.ts",
    "    rcLegacy.textContent = scan.manual_cleanup_hint;\n",
    "    rcLegacy.textContent = scan.manual_cleanup_hint.split(`\\n`)[0];\n",
    VITEST,
)


def run(cmd):
    """在沙箱里跑一条命令并**去色**。

    ⚠ 去色不是排版：`vitest` 在非 TTY 下照样上色，行首是 `ESC[31m` 而不是空白，
    于是「按 `FAIL ` 打头」那种抓法会一条都抓不到 —— 而「一条都没抓到」与
    「一条都没红」在终端上长得一模一样（`N-G1` 就是栽在这一形上）。
    """
    out = subprocess.run([SBX, cmd], capture_output=True, text=True).stdout
    return re.sub(r"\x1b\[[0-9;:?]*[a-zA-Z]", "", out)


def cut_main(name):
    path, anchor, repl, gate = CUTS[name]
    f = WT / path
    src = f.read_text(encoding="utf-8")
    n = src.count(anchor)
    print(f"【刀 {name}】锚点命中 {n} 次 · 文件 {path}")
    assert n == 1, f"锚点必须恰好命中 1 次，实得 {n} —— 这一刀没切成，不许当成「没红」"
    f.write_text(src.replace(anchor, repl), encoding="utf-8")
    try:
        after = f.read_text(encoding="utf-8")
        assert after != src and after.count(repl) == 1
        d = subprocess.run(
            ["git", "-C", str(WT), "diff", "--stat", "--", path],
            capture_output=True, text=True).stdout.strip()
        print(f"【变异已落地】{d}")
        out = run(gate)
        fails = [l for l in out.splitlines()
                 if ("FAILED" in l and l.startswith("test "))
                 or l.strip().startswith("×")
                 or l.strip().startswith("FAIL ")
                 or l.startswith("error[")
                 or l.startswith("error:")]
        res = [l for l in out.splitlines()
               if l.startswith("test result") or "Tests " in l or "Test Files" in l]
        print("【判定行】")
        for l in res:
            print("  " + l.strip())
        print(f"【红了 {len(fails)} 条】")
        for l in fails[:40]:
            print("  " + l.strip())
    finally:
        subprocess.run(["git", "-C", str(WT), "checkout", "--", path], check=True)
        print("【已还原】" + subprocess.run(
            ["git", "-C", str(WT), "status", "--porcelain"],
            capture_output=True, text=True).stdout.strip() or "【已还原】工作树干净")




# ═══ 刀 Z ═══



PI = "src-tauri/src/profile_installer.rs"
TR = "src-tauri/src/tool_registry.rs"
FB = "src-tauri/src/fenced_block.rs"
CS = "src-tauri/src/config_surface.rs"
LD = "src/launcher-diagnostics.ts"

EDITS = [
    # ① 装：POSIX 那一臂什么都不装
    (PI,
     "        ProfileFlavor::PosixRc => {\n"
     "            crate::sftp::merge_profile_block(existing, crate::sftp::CCM_WRAPPER_SNIPPET, what)\n"
     "        }\n    }\n}\n",
     "        ProfileFlavor::PosixRc => Ok(existing.to_string()),\n    }\n}\n"),
    # ② 卸：POSIX 那一臂什么都不卸
    (PI,
     "        ProfileFlavor::PosixRc => crate::sftp::strip_profile_block(existing, what),\n",
     "        ProfileFlavor::PosixRc => Ok(existing.to_string()),\n"),
    # ③ 查：裸行扫描回到「什么都查不出」
    (PI,
     "pub fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {\n"
     "    let mut out = Vec::new();\n",
     "pub fn scan_legacy_rc_lines(content: &str) -> Vec<LegacyRcLine> {\n"
     "    if true {\n        let _ = content;\n        return Vec::new();\n    }\n"
     "    #[allow(unreachable_code)]\n    let mut out = Vec::new();\n"),
    # ④ 提示：一段都不生成
    (PI,
     "    if hits.is_empty() {\n        return String::new();\n    }\n",
     "    if true {\n        let _ = (what, hits);\n        return String::new();\n    }\n"),
    # ⑤ 「装没装」在 POSIX 上恒 false（回到只认 PowerShell 那对围栏的世界）
    (PI,
     "        ProfileFlavor::PosixRc => (\n"
     "            content\n"
     "                .lines()\n"
     "                .any(|l| l.trim_start().starts_with(crate::sftp::CCM_PROFILE_BEGIN)),\n"
     "            None,\n"
     "        ),\n",
     "        ProfileFlavor::PosixRc => (false, None),\n"),
    # ⑥ 同名函数：POSIX 那一形回到扫不出来
    (PI,
     "        if flavor == ProfileFlavor::PosixRc {\n"
     "            if let Some(name) = function_name_of(l) {\n",
     "        if flavor == ProfileFlavor::PosixRc {\n"
     "            if let Some(name) = function_name_of(\"\") {\n"),
    # ⑦ 档：从 TOOLS 摘掉（回到「app 假设它在」那一档）
    (TR,
     '        id: "posix-rc-aliases",\n',
     '        id: "posix-rc-aliases-REVERTED",\n'),
    # ⑧ 账：本件那一行不进账
    (FB,
     '        id: "local-posix-block",\n',
     '        id: "local-posix-block-REVERTED",\n'),
    # ⑨ 前端：那一块永远不出现
    (LD,
     "    rcBlock.hidden = !path;\n",
     "    rcBlock.hidden = true;\n"),
]


def z_apply():
    for path, anchor, repl in EDITS:
        f = WT / path
        s = f.read_text(encoding="utf-8")
        n = s.count(anchor)
        print(f"  · {path} 锚点命中 {n} 次")
        assert n == 1, f"锚点必须恰好 1 次，实得 {n}（{path}）—— 这一刀没切成"
        f.write_text(s.replace(anchor, repl), encoding="utf-8")
    d = subprocess.run(["git", "-C", str(WT), "diff", "--stat"],
                       capture_output=True, text=True).stdout.strip()
    print("【变异已落地】\n" + d)




def z_main():
    z_apply()
    try:
        rust = run("cd src-tauri && cargo test --lib 2>&1")
        print("【Rust 判定行】")
        for l in rust.splitlines():
            if l.startswith("test result") or l.startswith("error"):
                print("  " + l.strip())
        print("【Rust 红了】")
        for l in rust.splitlines():
            if l.startswith("test ") and "FAILED" in l:
                print("  " + l.strip())
        ts = run("npx vitest run src/launcher-diagnostics.vitest.ts 2>&1")
        print("【TS 判定行】")
        for l in ts.splitlines():
            if "Tests " in l or "Test Files" in l:
                print("  " + l.strip())
        print("【TS 红了】")
        for l in ts.splitlines():
            if l.strip().startswith("FAIL "):
                print("  " + l.strip())
    finally:
        for path, _, _ in EDITS:
            subprocess.run(["git", "-C", str(WT), "checkout", "--", path], check=True)
        st = subprocess.run(["git", "-C", str(WT), "status", "--porcelain"],
                            capture_output=True, text=True).stdout.strip()
        print("【已还原】" + (st or "工作树干净"))




def z2_main():
    """刀 Z2：**前端生产文件整份退回基线**（`src/launcher-diagnostics.ts`）。

    与刀 Z 的前端那一处（只把 `rcBlock` 恒 hidden）**不是同一刀**，而且这一点是量出来的：
    只把它藏起来，7 条新断言里只红 1 条（按钮还在、处理器还在、IPC 照发）；
    整份退回才 7/7 全红。⇒ **「藏起来」不等于「退掉」**，两刀各买一半。

    ⚠ 用 `git restore --source=<基线> --worktree`，**不是** `git checkout <基线> -- <文件>`：
    后者会把那份基线内容**写进索引**，还原时 `git checkout -- <文件>` 拿的是索引里那份
    ⇒ 看起来「还原了」，实际把基线版留在了暂存区（本拍现打踩过一次）。
    """
    base = "e1390ab"
    path = "src/launcher-diagnostics.ts"
    subprocess.run(["git", "-C", str(WT), "restore", f"--source={base}",
                    "--worktree", "--", path], check=True)
    try:
        d = subprocess.run(["git", "-C", str(WT), "diff", "--stat", "--", path],
                           capture_output=True, text=True).stdout.strip()
        print(f"【变异已落地：前端生产文件整份退回 {base}】\n" + d)
        ts = run("npx vitest run src/launcher-diagnostics.vitest.ts 2>&1")
        print("【判定行】")
        for l in ts.splitlines():
            if "Tests " in l or "Test Files" in l:
                print("  " + l.strip())
        print("【红了】")
        for l in ts.splitlines():
            if l.strip().startswith("FAIL "):
                print("  " + l.strip())
    finally:
        subprocess.run(["git", "-C", str(WT), "restore", "--source=HEAD",
                        "--worktree", "--", path], check=True)
        st = subprocess.run(["git", "-C", str(WT), "status", "--porcelain"],
                            capture_output=True, text=True).stdout.strip()
        print("【已还原】" + (st or "工作树干净"))


def main():
    which = sys.argv[1]
    if which == "Z":
        z_main()
    elif which == "Z2":
        z2_main()
    elif which == "all":
        for k in CUTS:
            cut_main(k)
        z_main()
        z2_main()
    else:
        cut_main(which)


main()
