#!/usr/bin/env python3
"""`K-R104` 死值验的刀具。**一趟一刀**，fail-closed。

住址：`evidence/K-R104-cut.py`（本轮实现方自建）。
被测对象：**它自己所在的那棵工作树**（`__file__` 的上一级）——
不接路径参数，免得「同一个住址下先后住过两份被测对象不同的量具」那一形
（`brief` 12 记的 `5k`）。

用法：
  python3 evidence/K-R104-cut.py apply  <刀号>
  python3 evidence/K-R104-cut.py restore <刀号>
  python3 evidence/K-R104-cut.py list

🔴 **fail-closed**：任何一处锚点命中数不等于 1 ⇒ **一个字节都不写**并以非零退出。
   （`K-R87` 的 `cut.sh` 静默跳过，`7u` 首趟因此叠在上一刀上。）
🔴 还原不用 `cp -a`、不用 `git checkout <文件>`：**逐处反向替换**，
   之后自己再跑一次 `git diff --quiet` 自证。
"""
import io, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 刀号 → (说明, [(相对路径, 原文, 变异文)…])
CUTS = {
    # ── KR104D1 ────────────────────────────────────────────────────────────
    "M1": (
        "KR104D1①：只加 REGISTRY 不加镜子（从 COMMANDS 里拿掉 capture-pane）",
        [("remote-daemon-proto/src/inbound.rs",
          '    "capture-pane",\n    "kill",\n    "launch",\n    "oneshot-session",',
          '    "kill",\n    "launch",\n    "oneshot-session",')],
    ),
    "M3": (
        "KR104D1③：表里有、分派到不了（把 lookup 收窄成看不见 capture-pane）",
        [("remote-daemon-proto/src/inbound.rs",
          'REGISTRY.iter().find(|s| s.name == name)',
          'REGISTRY.iter().find(|s| s.name == name && s.name != "capture-pane")')],
    ),
    # ── KR104D2 ────────────────────────────────────────────────────────────
    "M5": (
        "KR104D2①：编排退回 CLI 面（生产段又渲染一条抓屏 shell 串）",
        [("src-tauri/src/account_usage.rs",
          '    let ch = match dial() {',
          '    let _relapse = format!("tmux capture-pane -p -t {}", "=x:");\n    let ch = match dial() {')],
    ),
    "M6": (
        "KR104D2③：每一段都重新拨一次号（= 走 CLI 面那个形状）",
        [("src-tauri/src/account_usage.rs",
          '    let out = drive_probe(ch.as_ref(), &session, payload, t).await;',
          '    let again = match dial() {\n        Ok(c) => c,\n        Err(e) => return probe_failed(e),\n    };\n    let out = drive_probe(again.as_ref(), &session, payload, t).await;')],
    ),
    # ── KR104D3 ────────────────────────────────────────────────────────────
    "M8": (
        "KR104D3①：帧面命令集动了而 BUILD_ID 没动",
        [("remote-daemon-proto/src/main.rs",
          'const BUILD_ID: &str = "p2i-frame-tmux-primitives";',
          'const BUILD_ID: &str = "p2h-oneshot-session";')],
    ),
    "M9": (
        "KR104D3③：把「老后端不认得」压成普通失败（分流器的两档合成一档）",
        [("src-tauri/src/account_usage.rs",
          '        Routed::NoChannel(why) => ProbeStepError::NothingWasSent(why),',
          '        Routed::NoChannel(why) => ProbeStepError::Refused(why),')],
    ),
    "M8b": (
        "KR104D3① 重切：表动了、BUILD_ID 没动、历史表也没追行（M8 切错了刀口）",
        [("remote-daemon-proto/src/main.rs",
          'const BUILD_ID: &str = "p2i-frame-tmux-primitives";',
          'const BUILD_ID: &str = "p2h-oneshot-session";'),
         ("remote-daemon-proto/src/build_id_guard.rs",
          '        (\n            "p2i-frame-tmux-primitives",',
          '        #[cfg(any())]\n        (\n            "p2i-frame-tmux-primitives",')],
    ),
    # ── KR104D4 ────────────────────────────────────────────────────────────
    "M10": (
        "KR104D4①：只读白名单多一条（仍只许 2 条）",
        [("remote-daemon-proto/src/readonly_guard.rs",
          '    ];\n\n    /// 这个路径在白名单上吗。',
          '        ("control/capture_pane.rs", "K-R104 死值验：白名单多一条"),\n    ];\n\n    /// 这个路径在白名单上吗。')],
    ),
    "M11": (
        "KR104D4②：帧面那条抓屏改成会改 tmux 状态",
        [("remote-daemon-proto/src/control/capture_pane.rs",
          'pub(crate) const CAPTURE_SUBCOMMAND: &str = "capture-pane";',
          'pub(crate) const CAPTURE_SUBCOMMAND: &str = "new-session";')],
    ),
    "M12": (
        "KR104D4③：daemon 里长出「隔 N 毫秒再抓一次」",
        [("remote-daemon-proto/src/control/capture_pane.rs",
          '    let name = checked_name(name)?;',
          '    std::thread::sleep(std::time::Duration::from_millis(500));\n    let name = checked_name(name)?;')],
    ),
    # ── 7u：把实现整个退掉 ─────────────────────────────────────────────────
    "U1": (
        "7u-a：退掉帧面那一半（两条 CommandSpec ＋ 镜子两行）",
        [("remote-daemon-proto/src/inbound.rs",
          '    "capture-pane",\n    "kill",\n    "launch",\n    "oneshot-session",',
          '    "kill",\n    "launch",'),
         ("remote-daemon-proto/src/inbound.rs",
          '    CommandSpec {\n        name: "capture-pane",',
          '    #[cfg(any())]\n    CommandSpec {\n        name: "capture-pane",'),
         ("remote-daemon-proto/src/inbound.rs",
          '    CommandSpec {\n        name: "oneshot-session",',
          '    #[cfg(any())]\n    CommandSpec {\n        name: "oneshot-session",')],
    ),
    "U2": (
        "7u-b：退掉 monitor 编排那一半（整条不发一个字节，直接回失败）",
        [("src-tauri/src/account_usage.rs",
          '    let ch = match dial() {',
          '    if true {\n        return probe_failed("7u：实现退掉了".to_string());\n    }\n    let ch = match dial() {')],
    ),
}


def read(rel):
    return io.open(os.path.join(ROOT, rel), encoding="utf-8").read()


def write(rel, s):
    io.open(os.path.join(ROOT, rel), "w", encoding="utf-8").write(s)


def run(cut, forward):
    if cut not in CUTS:
        print(f"❌ 没有这一刀：{cut}（有的：{', '.join(sorted(CUTS))}）")
        return 3
    why, edits = CUTS[cut]
    # ── 先全量核锚点，一处不对就整趟不写 ──────────────────────────────────
    #
    # 🔴 **`buf` 是在 09-13 现打的一个 bug 上加的**：上一版每一处编辑都各自
    #    `read(rel)` 重算、然后逐份 `write` —— 同一份文件上有多处编辑时**后写覆盖前写**，
    #    于是 `U1` 三处只落地了第 3 处，而刀具照样印「变异已落地」。
    #    那正是本轮单子逐字禁的那一形（`K-R87` 的 `cut.sh` 静默跳过）。
    #    ⇒ 同一份文件的多处编辑必须**在同一份内存文本上累加**，锚点也对着**累加后**的文本数。
    buf = {}
    for rel, old, new in edits:
        a, b = (old, new) if forward else (new, old)
        src = buf.get(rel) or read(rel)
        n = src.count(a)
        print(f"  锚点 {rel}：命中 {n} 次")
        if n != 1:
            print(f"❌ 锚点命中 {n} 次（要恰好 1）—— **一个字节都没写**（fail-closed）")
            return 2
        buf[rel] = src.replace(a, b)
    for rel, s in buf.items():
        write(rel, s)
    print(f"✅ {'变异已落地' if forward else '已还原'}：{cut} —— {why}")
    if not forward:
        rc = subprocess.run(["git", "-C", ROOT, "diff", "--quiet"]).returncode
        print("  git diff --quiet ⇒", "干净" if rc == 0 else f"🔴 还有残留（rc={rc}）")
        return 0 if rc == 0 else 4
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] == "list":
        for k in sorted(CUTS):
            print(f"{k}\t{CUTS[k][0]}")
        sys.exit(0)
    sys.exit(run(sys.argv[2], sys.argv[1] == "apply"))
