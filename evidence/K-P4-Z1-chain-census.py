#!/usr/bin/env python3
"""K-P4 摸底拍（`KP4Z1`）的量具 —— **「调出会话的终端」今天真的经过哪几个函数**。

住址：`<工作树>/evidence/K-P4-Z1-chain-census.py`（默认工作树 = `.claude/worktrees/k-p4b`）

它答的是 `KP4Z1` 那两维，**两维分开报，不合并**：

  · **盘上有** —— 每一跳的定义在哪、被谁调、住哪个 `cfg` 臂。纯文本判据。
  · **实测被走到** —— 这一跳**有没有一次真的执行过的读数**。本尺子只认三种执行面：
      ① 本机门禁（`.claude/devbox/gate`，**Linux**）会跑的 rust / vitest 测试；
      ② 仓里的 `e2e/*`（⚠ 它们不在 `npm run gate` 里，只在 CI）；
      ③ GitHub Actions 上真跑过的那一次（`rust` job 是 **windows-latest**）——
         要给 `--ci-log <文件>`，本尺子**不联网**。
    ⇒ 「盘上有」永远给得出；「实测被走到」给不出时**明写 `NO-EVIDENCE`**，不许拿①冒充③。

⚠ 尺子的射程（写在前面，免得被读大）：
  · 「测试调用」数的是**测试语料里出现该符号的行**，不是「这一跳真的被执行了」——
    真执行那一维只有 `--ci-log` 那一列能证，而它证的是**那一次、那个 commit**。
  · 本尺子**不判**「这条链在真 Windows 上跑得通」。那要一台真 Windows 现打，本拍没有。

跑法：
    python3 evidence/K-P4-Z1-chain-census.py
    python3 evidence/K-P4-Z1-chain-census.py --ci-log /path/to/job.log --rev <sha>

`--ci-log` 怎么取（本拍取的那一份，逐字记着好复算）：
    gh api repos/bo0Zeng/cc-monitor/actions/jobs/92463572527/logs > job.log
    # job 92463572527 = run 31052781231（head `1eeb4bfb`，2026-08-05T22:26:56Z）的
    # "Rust lint + test"（runs-on: windows-latest），该 job 结论 ✓ success
"""

import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-p4b"

# —— 语料地板（反空真自检）。切不到这么多文件就说明尺子的作用域坏了，报 CRASH 而不是「0 命中」。
PROD_FLOOR = 100
TEST_FLOOR = 40
CI_OK_FLOOR = 100  # CI 日志里 `... ok` 的行数下界

# 阳性对照：这一格**必须** > 0，否则测试语料是空的 / 切错了。
POSITIVE_CONTROL = "explainBringFrontFailure"

# ——「调出会话的终端」这一条链，逐跳。
#    每项：(跳号, 层, 显示名, 定义所在文件, 定义正则, 数命中用的 needle)
#    ⚠ needle 是**逐字子串**，数的是「出现该串的行」；定义行与注释行由 `call_hits` 单独剔。
CHAIN = [
    ("1", "前端·入口", 'focusBtn click 回调',
     "src/tabs.ts", r'focusBtn\.addEventListener\("click"', 'focusBtn'),
    ("1'", "前端·入口", "bringActiveTerminalToFront",
     "src/tabs.ts", r"^\s*bringActiveTerminalToFront\(\)", "bringActiveTerminalToFront"),
    ("2a", "前端·本机", "bringTerminalToFront",
     "src/tabs.ts", r"^function bringTerminalToFront", "bringTerminalToFront("),
    ("2b", "前端·远端", "bringRemoteTerminalToFront",
     "src/tabs.ts", r"^function bringRemoteTerminalToFront", "bringRemoteTerminalToFront("),
    ("3a", "IPC 边界", 'invoke "bring_terminal_to_front"',
     "src/tabs.ts", r'invoke<void>\("bring_terminal_to_front"', '"bring_terminal_to_front"'),
    ("3b", "IPC 边界", 'invoke "bring_remote_terminal_to_front"',
     "src/tabs.ts", r'invoke<void>\("bring_remote_terminal_to_front"',
     '"bring_remote_terminal_to_front"'),
    ("4a", "宿主·命令", "fn bring_terminal_to_front",
     "src-tauri/src/lib.rs", r"^async fn bring_terminal_to_front", "bring_terminal_to_front"),
    ("4b", "宿主·命令", "fn bring_remote_terminal_to_front",
     "src-tauri/src/lib.rs", r"^async fn bring_remote_terminal_to_front",
     "bring_remote_terminal_to_front"),
    ("5", "宿主·查表", "SidHwndCache/RemoteHwndCache::lookup",
     "src-tauri/src/bind.rs", r"^\s*pub fn lookup\(&self, sid: &str\)", "cache.lookup("),
    ("6", "宿主·现扫", "try_bind_with_retry",
     "src-tauri/src/bind.rs", r"^\s*pub fn try_bind_with_retry", "try_bind_with_retry("),
    ("7", "宿主·现扫", "try_bind",
     "src-tauri/src/bind.rs", r"^\s*pub fn try_bind\(", "try_bind(&"),
    ("8", "宿主·扫窗", "find_window_by_marker_substr",
     "src-tauri/src/bind.rs", r"^fn find_window_by_marker_substr",
     "find_window_by_marker_substr("),
    ("9", "宿主·校验", "verify_binding",
     "src-tauri/src/bind.rs", r"^pub fn verify_binding", "verify_binding(&"),
    ("10", "宿主·校验", "process_creation_filetime",
     "src-tauri/src/bind.rs", r"^fn process_creation_filetime", "process_creation_filetime("),
    ("11", "宿主·拉窗", "bind::activate",
     "src-tauri/src/bind.rs", r"^pub fn activate", "activate(binding.hwnd)"),
    ("12", "宿主·拉窗", "SetForegroundWindow",
     "src-tauri/src/bind.rs", r"SetForegroundWindow\(h\)", "SetForegroundWindow"),
]

CFG_RE = re.compile(r"^#\[cfg\((not\()?windows\)?\)\]")


def rs_strip_cfg_test(src: str):
    """把 `#[cfg(test)]` 那一块摘出来。返回 (生产段, 测试段)，两段都保留行号占位（空行）。

    切法：见到 `#[cfg(test)]` 就从下一个 `{` 起数花括号，配平为止。
    ⚠ 它不认字符串 / 注释里的花括号 —— 本仓的守卫用的是同一族粗切法，**这一格是已知的近似**。
    """
    lines = src.splitlines()
    prod, test = [], []
    i = 0
    while i < len(lines):
        if lines[i].strip() == "#[cfg(test)]":
            depth, started = 0, False
            while i < len(lines):
                depth += lines[i].count("{") - lines[i].count("}")
                if "{" in lines[i]:
                    started = True
                test.append(lines[i])
                prod.append("")
                i += 1
                if started and depth <= 0:
                    break
            continue
        prod.append(lines[i])
        test.append("")
        i += 1
    return "\n".join(prod), "\n".join(test)


def is_test_path(rel: str) -> bool:
    return (
        ".vitest." in rel
        or ".test." in rel
        or ".spec." in rel
        or rel.startswith("e2e/")
        or "/__tests__/" in rel
    )


def collect(wt: Path):
    """切两个语料面。**分母写清楚**：只走这四棵子树，不走 node_modules / target / 生成物。"""
    roots = ["src", "src-tauri/src", "remote-daemon-proto/src", "e2e", "shared"]
    exts = {".ts", ".mts", ".tsx", ".rs", ".js", ".sh", ".py", ""}
    prod, test = {}, {}
    for r in roots:
        base = wt / r
        if not base.exists():
            continue
        for p in sorted(base.rglob("*")):
            if not p.is_file():
                continue
            if any(part in ("node_modules", "target", "dist") for part in p.parts):
                continue
            if p.suffix not in exts:
                continue
            rel = str(p.relative_to(wt))
            try:
                src = p.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if p.suffix == ".rs":
                pr, te = rs_strip_cfg_test(src)
                prod[rel] = pr
                test[rel + "#[cfg(test)]"] = te
            elif is_test_path(rel):
                test[rel] = src
            else:
                prod[rel] = src
    return prod, test


COMMENT_RE = re.compile(r"^\s*(//|///|/\*|\*|#\s)")


def count_hits(corpus, needle, skip_comments=False):
    out = []
    for rel, src in corpus.items():
        for n, line in enumerate(src.splitlines(), 1):
            if needle in line:
                if skip_comments and COMMENT_RE.match(line):
                    continue
                out.append((rel, n, line.strip()))
    return out


def find_defs(wt: Path, rel: str, pattern: str):
    """定义行 + 它头上最近的那个 `#[cfg(windows)]` / `#[cfg(not(windows))]`（若有）。"""
    p = wt / rel
    if not p.exists():
        return []
    lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
    rx = re.compile(pattern)
    out = []
    for n, line in enumerate(lines, 1):
        if rx.search(line):
            arm = "—"
            for back in range(max(0, n - 4), n - 1):
                m = CFG_RE.match(lines[back])
                if m:
                    arm = lines[back].strip()
            out.append((n, arm, line.strip()[:70]))
    return out


def ci_executed(ci_lines, symbol_owner_tests):
    """在 CI 日志里找「这些测试真的跑过」的行。返回命中的 (测试名, 时间戳)。"""
    hits = []
    for t in symbol_owner_tests:
        for line in ci_lines:
            if f"test {t} ... ok" in line:
                hits.append((t, line.split()[0]))
                break
    return hits


# 哪些测试**声称**覆盖哪一跳（人工登记；每条都要能在 CI 日志里对上号才算数）
TEST_OWNERS = {
    "find_window_by_marker_substr": ["bind::tests::remote_bind_finds_real_ccm_rbind_window"],
    "try_bind": ["bind::tests::remote_bind_finds_real_ccm_rbind_window"],
    "verify_binding": ["bind::tests::remote_bind_finds_real_ccm_rbind_window"],
    "SidHwndCache/RemoteHwndCache::lookup": [
        "bind::tests::remote_hwnd_cache_insert_lookup_forget",
        "bind::tests::remote_bind_finds_real_ccm_rbind_window",
    ],
}

# 这几个函数体要逐个对拍字节（`rev` vs HEAD）：那次真机执行之后它们动没动
#    ⚠ 这里**没有** `tauri::generate_handler!` —— 本尺子的切法是数花括号，
#      切不动那个宏体（它的 `{` 有的在字符串/属性里，实测切出 1400 行 ≠ 170 行）。
#      「那两条命令还注册着吗」这一格由上表 hop 4a/4b 的 `P src-tauri/src/lib.rs:1200/1202` 给。
FN_WATCH = [
    ("src-tauri/src/lib.rs", "async fn bring_terminal_to_front"),
    ("src-tauri/src/lib.rs", "async fn bring_remote_terminal_to_front"),
]


def body_of(src: str, header: str, floor_lines: int = 3):
    """从含 `header` 的那一行起，按花括号配平切到函数末。切不出来返回 None（**不返回空串**）。"""
    lines = src.splitlines()
    for i, line in enumerate(lines):
        if header in line:
            depth, started, out = 0, False, []
            for j in range(i, len(lines)):
                depth += lines[j].count("{") - lines[j].count("}")
                out.append(lines[j])
                if "{" in lines[j]:
                    started = True
                if started and depth <= 0:
                    break
            if len(out) < floor_lines:
                return None
            return "\n".join(out)
    return None


def fn_md5(wt: Path, rel: str, rev: str, names):
    """逐函数 md5：`rev` 那一版 vs 工作树当前版。用来判「那次真机执行的字节 == 今天的字节」吗。"""
    try:
        old = subprocess.run(
            ["git", "-C", str(wt), "show", f"{rev}:{rel}"],
            capture_output=True, text=True, check=True,
        ).stdout
    except subprocess.CalledProcessError as e:
        return None, f"取不到 {rev}:{rel} —— {e.stderr.strip()[:120]}"
    new = (wt / rel).read_text(encoding="utf-8", errors="replace")
    return (hashlib.md5(old.encode()).hexdigest(),
            hashlib.md5(new.encode()).hexdigest()), None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    ap.add_argument("--ci-log", default=None)
    ap.add_argument("--rev", default="1eeb4bfb3b1ecd97f5543c48fb2ba2fe81339680")
    args = ap.parse_args()
    wt = Path(args.wt)

    head = subprocess.run(["git", "-C", str(wt), "rev-parse", "--short", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    print(f"【量于】工作树 {wt} · HEAD {head}")

    prod, test = collect(wt)
    print(f"【分母】生产语料 {len(prod)} 个文件 · 测试语料 {len(test)} 个切片"
          f"（.rs 的 `#[cfg(test)]` 段单算一片）")
    if len(prod) < PROD_FLOOR or len(test) < TEST_FLOOR:
        print(f"CRASH: 语料切少了（地板 {PROD_FLOOR}/{TEST_FLOOR}）—— 尺子坏了，不是「没命中」")
        return 2

    pc = count_hits(test, POSITIVE_CONTROL)
    print(f"【反空真·阳性对照】测试语料里 `{POSITIVE_CONTROL}` 命中 {len(pc)} 行"
          f"（{'OK' if pc else 'CRASH'}）")
    if not pc:
        print("CRASH: 阳性对照 0 命中 ⇒ 下面所有「测试 0」都不可信")
        return 2
    for rel, n, _ in pc[:6]:
        print(f"        · {rel}:{n}")

    ci_lines = []
    if args.ci_log:
        ci_lines = Path(args.ci_log).read_text(encoding="utf-8", errors="replace").splitlines()
        oks = [l for l in ci_lines if " ... ok" in l]
        print(f"【CI 日志】{args.ci_log} · {len(ci_lines)} 行 · `... ok` {len(oks)} 条"
              f"（地板 {CI_OK_FLOOR} ⇒ {'OK' if len(oks) >= CI_OK_FLOOR else 'CRASH'}）")
        if len(oks) < CI_OK_FLOOR:
            print("CRASH: CI 日志里跑过的测试太少 ⇒ 这份日志不是那个 job 的全量输出")
            return 2
    else:
        print("【CI 日志】未给 —— 「实测被走到」第③面**这一轮没量**（不是量出了 0）")

    print()
    print("跳  层        符号                                   定义(行·cfg臂)"
          "          生产码行  测试码行  实测被走到")
    print("-" * 122)
    detail = []
    for hop, layer, sym, rel, pat, needle in CHAIN:
        defs = find_defs(wt, rel, pat)
        defstr = " ".join(f"{n}{'' if arm == '—' else '·' + arm[6:-2]}"
                          for n, arm, _ in defs) or "×"
        ph = count_hits(prod, needle, skip_comments=True)
        th = count_hits(test, needle, skip_comments=True)
        ci = "—(没量)"
        if ci_lines:
            owners = TEST_OWNERS.get(sym, [])
            hits = ci_executed(ci_lines, owners)
            ci = ("✅ " + hits[0][0].split("::")[-1][:30]) if hits else "NO-EVIDENCE"
        print(f"{hop:<3} {layer:<9} {sym[:38]:<38} {defstr[:24]:<24}"
              f" {len(ph):>6}   {len(th):>6}   {ci}")
        detail.append((hop, sym, ph, th))

    print()
    print("【逐跳的码行住址】（生产 P / 测试 T，各最多列 6 条）")
    for hop, sym, ph, th in detail:
        print(f"  {hop} {sym}")
        for rel, n, line in ph[:6]:
            print(f"      P {rel}:{n}  {line[:78]}")
        for rel, n, line in th[:6]:
            print(f"      T {rel}:{n}  {line[:78]}")

    print()
    print(f"【整文件字节对拍】那一次真机执行（{args.rev[:8]}）的字节 vs 今天")
    for rel in ("src-tauri/src/bind.rs", "src-tauri/src/lib.rs"):
        got, err = fn_md5(wt, rel, args.rev, None)
        if err:
            print(f"  {rel}: {err}")
            continue
        old, new = got
        print(f"  {rel}: {args.rev[:8]} md5 {old[:12]} · HEAD md5 {new[:12]} ⇒ "
              f"{'同一份字节' if old == new else '**变了** ⇒ 整文件不能直接搬，往下看逐函数'}")

    print()
    print("【逐函数字节对拍】整文件变了不等于这条链变了 —— 只切链上那几段")
    for rel, header in FN_WATCH:
        try:
            old_src = subprocess.run(
                ["git", "-C", str(wt), "show", f"{args.rev}:{rel}"],
                capture_output=True, text=True, check=True).stdout
        except subprocess.CalledProcessError as e:
            print(f"  {rel} :: {header}: 取不到旧版 —— {e.stderr.strip()[:80]}")
            continue
        new_src = (wt / rel).read_text(encoding="utf-8", errors="replace")
        ob, nb = body_of(old_src, header), body_of(new_src, header)
        if ob is None or nb is None:
            print(f"  {rel} :: {header}: CRASH —— 切不出函数体"
                  f"（旧 {'×' if ob is None else '√'} / 新 {'×' if nb is None else '√'}）"
                  "⇒ 这是尺子坏了，不是「没变」")
            continue
        om, nm = hashlib.md5(ob.encode()).hexdigest(), hashlib.md5(nb.encode()).hexdigest()
        print(f"  {rel} :: {header[:38]:<38} {om[:10]} → {nm[:10]}  "
              f"{'同一份字节' if om == nm else '**变了**'}（{len(ob.splitlines())}→"
              f"{len(nb.splitlines())} 行）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
