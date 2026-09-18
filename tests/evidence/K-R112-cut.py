#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R112` 的刀具。两族刀，**跑法不同，别混**：

- **门禁刀**（`apply <刀名>` → 跑一趟沙箱门禁 → `restore`）：被测对象是 **Rust 判据**，
  只有 `cargo` 那一格判得了它。一趟一刀。
- **尺子刀**（`ruler <刀名>`，自带跑）：被测对象是 `evidence/K-R111-ruler.py` 那张三档表，
  **在副本上**做，一秒出读数，不碰生产树。

住址：本文件住 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 12：量具住址要唯一定位到那一份被测对象，不许拿 `cwd` 猜）。
⚠ 与 `K-R109-cut.py` / `K-R106-cut.py` 是不同的量具（被测对象是不同的树）。
跑之前先看它印的「被测对象」那一行。

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**。
- 🔴 同一份文件的多处替换在内存里做完再写一次盘，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）⇒ `restore` 是重写原文。
- 备份落 `evidence/.K-R112-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。
- 🔴 尺子刀**先把副本里的 `.git` 去掉**（`brief` 12c：工作树的 `.git` 是一行指回原仓的指针）——
  本脚本只拷 `src-tauri/src` 与 `remote-daemon-proto/src` 两棵子树，压根不拷 `.git`。
"""
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BACKUP = ROOT / "evidence" / ".K-R112-cut-backup.json"
RULER = ROOT / "evidence" / "K-R111-ruler.py"

CCBUS = "src-tauri/src/cc_bus.rs"
TMUX = "src-tauri/src/tmux.rs"

# ── 门禁刀谱：{刀名: {文件: [(锚点, 替换, 该命中几次), …]}} ────────────────────

# `KR112D1` ①：把 `bus-kill` 那一跳换回拼 shell（最小面 —— 只动 `cc_bus_kill` 的体）。
D1_1 = {
    CCBUS: [
        (
            """pub async fn cc_bus_kill(origin: String, id: String) -> Result<String, String> {
    kill_via_daemon(&origin, &id).await
}""",
            """pub async fn cc_bus_kill(origin: String, id: String) -> Result<String, String> {
    if let Some(why) = refuse_local_write(&origin, "收掉 agent") {
        return Err(why);
    }
    let cmd = format!("cc-kill {id} 2>&1");
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        30,
        "收掉 agent",
        OnOverflow::Truncate,
    )
    .await?;
    Ok(out.trim().to_string())
}""",
            1,
        )
    ]
}

# `KR112D1` ②：删回落之后**再把回落塞回去**（查在线那条）。
D1_2 = {
    CCBUS: [
        (
            """pub async fn check_cc_bus_agent_online(origin: String, id: String) -> Result<bool, String> {
    online_via_daemon(&origin, &id).await
}""",
            """pub async fn check_cc_bus_agent_online(origin: String, id: String) -> Result<bool, String> {
    if let Ok(live) = online_via_daemon(&origin, &id).await {
        return Ok(live);
    }
    let cmd = format!("tmux has-session -t '={id}:' 2>/dev/null && echo ONLINE || echo OFFLINE");
    let raw = local_shell_read(&cmd, 4096, 15, "查在线", OnOverflow::Reject).await?;
    Ok(raw.contains("ONLINE"))
}""",
            1,
        )
    ]
}

# `KR112D2` ①：抓屏换回一次性 SSH。
D2_1 = {
    TMUX: [
        (
            """pub async fn capture_remote_pane(origin: String, target: String) -> Result<String, String> {
    use crate::backend::control::daemon_route::Routed;""",
            """pub async fn capture_remote_pane(origin: String, target: String) -> Result<String, String> {
    gate1_reject_empty(&target)?;
    let t = exact_target(&target)?;
    let cmd = format!("if command -v tmux >/dev/null 2>&1; then tmux capture-pane -p -t {t} 2>/dev/null; fi");
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("读 pane 快照失败: {e}"))?;
    return Ok(String::from_utf8_lossy(&buf).to_string());
    #[allow(unreachable_code)]
    use crate::backend::control::daemon_route::Routed;""",
            1,
        )
    ]
}

# `KR112D2` ②：本机那一支退回「回一句还看不了」。
D2_2 = {
    TMUX: [
        (
            """    gate1_reject_empty(&target)?;
    match capture_via_daemon(&origin, &target).await {""",
            """    gate1_reject_empty(&target)?;
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return Err(format!(
            "本机还看不了 `{target}` 的画面预览：本机的 tmux 快照只带会话名，不带屏幕内容。"
        ));
    }
    match capture_via_daemon(&origin, &target).await {""",
            1,
        )
    ]
}

#: 阴性对照要停掉的判据 —— **本件新加或重写的那几条**，逐条点名。
#: 停法用 `#[ignore]` 而不是整段删：删会连带动到相邻块（切多了就不是「只拿掉判据」），
#: 而 `#[ignore]` 让它们**不参与判定**且仍然编译 —— 这正是阴性对照要的那一格。
D1_JUDGES = [
    "the_kill_path_asks_the_backend_instead_of_composing_a_shell_line",
    "the_kill_reply_keeps_the_three_states_apart",
    "an_unknown_liveness_is_never_rendered_as_dark",
    "the_broadcast_has_no_ssh_fallback_left",
    "the_kill_entry_still_refuses_bad_ids_before_it_asks_anyone",
    "letting_kill_broadcast_and_online_through_did_not_let_spawn_through",
    "the_probe_by_name_template_is_gone_from_production",
    "the_online_lamp_asks_the_identity_space_and_has_nothing_else_to_ask",
    "whoever_still_asks_for_a_remote_config_branches_on_local_first",
    "the_write_face_branches_on_local_before_it_asks_for_a_remote_config",
]
D2_JUDGES = [
    "the_capture_path_asks_the_backend_instead_of_composing_a_shell_line",
    "the_local_capture_is_no_longer_a_dead_end",
    "the_five_capture_refusals_stay_apart",
    "the_tmux_shell_line_detector_really_sees_each_shape",
    "every_target_placeholder_comes_from_exact_target",
    "tmux_targets_use_exact_match",
    "gate1_rejects_only_empty_target",
]


def _ignore_cuts(path: str, names: list[str]) -> dict:
    return {
        path: [
            (f"    #[test]\n    fn {n}(", f'    #[test]\n    #[ignore = "阴性对照"]\n    fn {n}(', 1)
            for n in names
        ]
    }


def _merge(*ds: dict) -> dict:
    out: dict[str, list] = {}
    for d in ds:
        for k, v in d.items():
            out.setdefault(k, []).extend(v)
    return out


# ★ 本件自加（DoD 没点名）：**把两个「讲成人话」的纯函数整个退掉** ——
#   `describe_kill_reply` / `describe_capture_refusal` 各压成一句通用话。
#   ⚠ **形状对、恒答其中一张脸**（`brief` 第 7 条）：签名与返回类型一个字没动，
#   不是 `return None` 那种把台子炸掉的换法。它验的是那两条「分得开」的判据不是仪式。
D0_FLAT = {
    CCBUS: [
        (
            """    let get = |k: &str| reply.and_then(|r| r.get(k)).and_then(|v| v.as_bool());
    match (get("killed"), get("stale_only")) {""",
            """    let _ = reply;
    return format!("收掉 {id} 完成");
    #[allow(unreachable_code)]
    let get = |k: &str| reply.and_then(|r| r.get(k)).and_then(|v| v.as_bool());
    #[allow(unreachable_code)]
    match (get("killed"), get("stale_only")) {""",
            1,
        )
    ],
    TMUX: [
        (
            """fn describe_capture_refusal(target: &str, code: &str, message: &str) -> String {
    match code {""",
            """fn describe_capture_refusal(target: &str, code: &str, message: &str) -> String {
    let _ = (code, message);
    return format!("抓不了 `{target}` 的画面");
    #[allow(unreachable_code)]
    match code {""",
            1,
        )
    ],
}

# `KR112D1` ②的第二半（本件自加）：广播那条回落也塞回去。
# ⚠ DoD ② 只说「删回落之后再把回落塞回去」，没说是哪一条 —— 本件删了**两条**回落
#   （查在线 · 广播），所以两条各切一刀，别只切一条就说「回落被钉住了」。
D1_5 = {
    CCBUS: [
        (
            """    match broadcast_via_daemon(&origin, &text).await {
        Ok(msg) => Ok(msg),
        Err(BroadcastRoute::NoChannel(why)) => Err(format!(""",
            """    match broadcast_via_daemon(&origin, &text).await {
        Ok(msg) => return Ok(msg),
        Err(BroadcastRoute::NoChannel(why)) => {
            tracing::info!("[{origin}] 广播回落到 SSH 路径：{why}");
        }
        Err(BroadcastRoute::Failed(why)) => return Err(why),
    }
    let cmd = format!("cc-broadcast {} 2>&1", crate::ssh_source::shell_quote(&text));
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        30,
        "广播",
        OnOverflow::Truncate,
    )
    .await?;
    return Ok(out.trim().to_string());
    #[allow(unreachable_code)]
    match broadcast_via_daemon(&origin, &text).await {
        Ok(msg) => Ok(msg),
        Err(BroadcastRoute::NoChannel(why)) => Err(format!(""",
            1,
        )
    ]
}

CUTS = {
    "d1-5": ("`KR112D1` ②的第二半：广播那条 SSH 回落也塞回去", D1_5),
    "d0-flat": ("本件自加：两个「讲成人话」的纯函数各压成一句通用话（形状对、恒答一张脸）", D0_FLAT),
    "d1-1": ("`KR112D1` ①：`bus-kill` 那一跳换回拼 shell", D1_1),
    "d1-2": ("`KR112D1` ②：删掉的回落再塞回去（查在线）", D1_2),
    "d1-4": (
        "`KR112D1` ④ 阴性对照：本件新加/重写的判据整段停掉 ＋ 刀①",
        _merge(_ignore_cuts(CCBUS, D1_JUDGES), D1_1),
    ),
    "d2-1": ("`KR112D2` ①：抓屏换回一次性 SSH", D2_1),
    "d2-2": ("`KR112D2` ②：本机那一支退回「回一句还看不了」", D2_2),
    "d2-3": (
        "`KR112D2` ③ 阴性对照：本件新加/重写的判据整段停掉 ＋ 刀①",
        _merge(_ignore_cuts(TMUX, D2_JUDGES), D2_1),
    ),
}

# ── 尺子刀谱：在副本上改，跑 `K-R111-ruler.py` ──────────────────────────────
RULER_CUTS = {
    # `KR112D1` ③：摘掉 daemon 那一侧的 `bus-kill`。**锚点照抄 `K-R111` 刀④**：
    # 两张具名表**块内**各一处（⚠ 全文数是 1 与 4 —— 不收窄到块内就会切错地方）。
    "d1-3": "摘掉 daemon 两张具名表**块内**各一行 `bus-kill`",
    # `KR112D3` ①：基线不拧（把本件拧下来的四格退回去）。
    "d3-1": "基线不拧：四格退回 `还有接线活`",
    # `KR112D3` ②：基线拧过头（把还没退役的 `read_cc_bus_state` 也写成 `已完`）。
    "d3-2": "基线拧过头：`read_cc_bus_state` 也写成 `已完`",
}


def _apply_text(cuts: dict, dry: bool = False) -> dict:
    """先全量断言锚点，再写盘。返回 {文件: 原文}。"""
    backup: dict[str, str] = {}
    staged: dict[str, str] = {}
    for rel, edits in cuts.items():
        p = ROOT / rel
        src = p.read_text(encoding="utf-8")
        backup[rel] = src
        cur = src
        for old, new, want in edits:
            got = cur.count(old)
            if got != want:
                print(f"❌ {rel}: 锚点命中 {got} 次（要 {want}）\n   锚点前 60 字：{old[:60]!r}")
                sys.exit(3)
            cur = cur.replace(old, new, want)
        staged[rel] = cur
    if dry:
        return backup
    for rel, cur in staged.items():
        (ROOT / rel).write_text(cur, encoding="utf-8")
    # 回读逐处核对
    for rel, edits in cuts.items():
        back = (ROOT / rel).read_text(encoding="utf-8")
        for old, new, _ in edits:
            if new and new not in back:
                print(f"❌ {rel}: 写完回读找不到替换文本 —— 落盘没成功")
                sys.exit(3)
    return backup


def cmd_apply(name: str) -> int:
    if BACKUP.exists():
        print(f"❌ 已有备份 {BACKUP.name} —— 上一刀没还原。先 `restore`。")
        return 3
    if name not in CUTS:
        print(f"❌ 没有这把刀：{name}（有 {sorted(CUTS)}）")
        return 3
    why, cuts = CUTS[name]
    backup = _apply_text(cuts)
    BACKUP.write_text(json.dumps({"cut": name, "files": backup}, ensure_ascii=False), encoding="utf-8")
    n = sum(len(v) for v in cuts.values())
    print(f"变异已落地：{name} —— {why}（{len(cuts)} 份文件 / {n} 处锚点，逐处命中数已断言）")
    print(f"被测对象：{ROOT}")
    print("下一步：跑一趟沙箱门禁，读完再 `restore`。")
    return 0


def cmd_restore() -> int:
    if not BACKUP.exists():
        print("· 没有备份，无需还原")
        return 0
    d = json.loads(BACKUP.read_text(encoding="utf-8"))
    for rel, src in d["files"].items():
        (ROOT / rel).write_text(src, encoding="utf-8")  # 重写原文 ⇒ mtime 必变
    BACKUP.unlink()
    print(f"已还原（刀 {d['cut']}，{len(d['files'])} 份文件；重写原文，mtime 已变）")
    return 0


def _copy_trees(dst: Path) -> tuple[Path, Path]:
    src = dst / "src-tauri" / "src"
    dmn = dst / "remote-daemon-proto" / "src"
    src.parent.mkdir(parents=True)
    dmn.parent.mkdir(parents=True)
    shutil.copytree(ROOT / "src-tauri" / "src", src)
    shutil.copytree(ROOT / "remote-daemon-proto" / "src", dmn)
    return src, dmn


def _block_replace(p: Path, const: str, line: str, want: int) -> None:
    """在 `const` 那张表的**块内**删掉 `line`。⚠ 收窄到块内 —— 全文数会切错地方。"""
    t = p.read_text(encoding="utf-8")
    i = t.index(const)
    j = t.index("\n];", i)
    block = t[i:j]
    got = block.count(line)
    if got != want:
        print(f"❌ {p.name} 的 `{const}` 块内 `{line.strip()}` 命中 {got} 次（要 {want}）")
        sys.exit(3)
    p.write_text(t[:i] + block.replace(line, "", want) + t[j:], encoding="utf-8")


def cmd_ruler(name: str) -> int:
    if name not in RULER_CUTS:
        print(f"❌ 没有这把尺子刀：{name}（有 {sorted(RULER_CUTS)}）")
        return 3
    with tempfile.TemporaryDirectory(prefix="kr112-") as td:
        dst = Path(td)
        src, dmn = _copy_trees(dst)
        ruler = dst / "K-R112-ruler-copy.py"
        ruler.write_text(RULER.read_text(encoding="utf-8"), encoding="utf-8")
        if name == "d1-3":
            _block_replace(dmn / "main.rs", "const SUBCOMMANDS", '    "--bus-kill",\n', 1)
            _block_replace(dmn / "inbound.rs", "pub const COMMANDS", '    "bus-kill",\n', 1)
            print("变异已落地：daemon 两张具名表**块内**各摘掉一行 `bus-kill`（各断言命中 1 次）")
        elif name == "d3-1":
            t = ruler.read_text(encoding="utf-8")
            for k in (
                "capture_remote_pane",
                "check_cc_bus_agent_online",
                "cc_bus_broadcast",
                "cc_bus_kill",
            ):
                old = f'    "{k}": "已完",'
                if t.count(old) != 1:
                    print(f"❌ 基线里 `{k}` 那一行命中 {t.count(old)} 次（要 1）")
                    return 3
                t = t.replace(old, f'    "{k}": "还有接线活",')
            ruler.write_text(t, encoding="utf-8")
            print("变异已落地：基线四格退回 `还有接线活`（各断言命中 1 次）")
        elif name == "d3-2":
            t = ruler.read_text(encoding="utf-8")
            old = '    "read_cc_bus_state": "不是接线",'
            if t.count(old) != 1:
                print(f"❌ 基线里 `read_cc_bus_state` 那一行命中 {t.count(old)} 次（要 1）")
                return 3
            ruler.write_text(t.replace(old, '    "read_cc_bus_state": "已完",'), encoding="utf-8")
            print("变异已落地：基线拧过头 —— `read_cc_bus_state` 写成 `已完`（断言命中 1 次）")
        print(f"被测对象：{src}  ＋  {dmn}")
        r = subprocess.run(
            [sys.executable, str(ruler), "--src-root", str(src), "--daemon-root", str(dmn)],
            capture_output=True,
            text=True,
        )
        tail = [l for l in r.stdout.splitlines() if l.startswith(("🔴", "   ·", "✅"))]
        print("\n".join(tail) if tail else r.stdout[-2000:])
        if r.stderr.strip():
            print("--- stderr ---\n" + r.stderr[-1500:])
        print(f"退出码 {r.returncode}")
    return 0


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        print("门禁刀：", " ".join(sorted(CUTS)))
        print("尺子刀：", " ".join(sorted(RULER_CUTS)))
        return 0
    verb = sys.argv[1]
    if verb == "list":
        for k, (why, _) in sorted(CUTS.items()):
            print(f"  [门禁] {k:6} {why}")
        for k, why in sorted(RULER_CUTS.items()):
            print(f"  [尺子] {k:6} {why}")
        return 0
    if verb == "apply":
        return cmd_apply(sys.argv[2])
    if verb == "restore":
        return cmd_restore()
    if verb == "ruler":
        return cmd_ruler(sys.argv[2])
    print(f"❌ 不认识的动作：{verb}")
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
