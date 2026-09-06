#!/usr/bin/env python3
"""W-F1b 摸底量具 ④：**SSH 那一摊的承载面普查**（`WF1bD1` 的 `§0a②③`、`WF1bD2` 的「盘上有」维）。

本量具只答「**盘上有什么**」。「**实测跑得起来**」那一维在
`W-F1b-headless-russh-carrier-probe.py`（`WF1bD2` 逼出来的两维，一个量具只管一维）。

## 它答的三格

  ① `§0a②` 那把 grep 的**份数**，并把每一份**逐份分类** —— PM `§0a⑤①` 明说
     「哪几份真在发 SSH、哪几份只是提了一句」他**没有逐份看**。
  ② `§0a③` 的 `[[bin]]` 分母。⚠ **`[[bin]]` 块数不等于 bin 目标数** ——
     cargo 还会**自动发现** `src/main.rs` 与 `src/bin/*.rs`。两个数都报，别混。
  ③ `WF1bD2` 的四条分母：`[[bin]]` · `tests/` · `#[cfg(test)]` · `examples/`。

## 🔴 两把尺子（死值验 `W1bM1`）

第 ① 格的份数用**两把独立的尺子**各打一次：
  · 尺子甲：`grep -rln <needle> src-tauri/src remote-daemon-proto/src`（PM 用的那把，逐字）
  · 尺子乙：Python 自己走 `git ls-files` 拿到人群，再逐文件用 `re` 匹配同一组 needle
两把不同值 ⇒ 读数不成立，输出里逐字写出来，不许挑一个报。

⚠ 两把尺子的**人群本来就可能不同**：甲走**工作树上的文件**（含未跟踪的），
乙走**索引里跟踪的文件**。差集单列，不混进「一致 / 不一致」的判定里
〔`brief` 第 12 条：量「两边一样」时分母只许装两边都有的东西〕。

## 分类口径（逐条写死，别靠印象）

对每一份命中文件，按**非注释行**上的证据分四档（可叠加，输出取最强的一档）：
  · `A 发起SSH连接` —— 非注释行上有 `client::connect` / `client::connect_stream`
  · `B 用已有russh会话` —— 非注释行上有 `russh` / `russh_sftp` 的类型或调用，但不发起连接
  · `C 起外部ssh进程` —— 有 `Command::new("ssh")`
  · `D 只提了一句` —— 上面三档都没有（命中全落在注释 / 文档 / 字符串名字上）
「注释行」= 行首去空白后以 `//` 或 `*` 或 `/*` 打头。⚠ 这**不剥行尾注释**，
故 `B` 档可能把「行尾注释里提了一句 russh」的行算成代码 —— 这条**假阳方向**写在这里，
而它压不垮结论：`A` 档只认 `client::connect`，那个串不出现在任何行尾注释里（输出可核）。

## 被测对象指向哪棵树

`WT` 常量 = `/home/zbl/文档/claudecode-frontend/.claude/worktrees/w-f1b`。
换树重跑改这一个常量。

用法：  python3 evidence/W-F1b-ssh-carrier-census.py
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/w-f1b")
SCAN_DIRS = ["src-tauri/src", "remote-daemon-proto/src"]

# PM `§0a②` 那把 grep 的 needle，逐字（BRE 的 `\|` 在这里翻成 Python 的 `|`）
NEEDLES = [r"ssh2", r"russh", r"openssh", r'Command::new\("ssh"\)']
NEEDLE_RE = re.compile("|".join(NEEDLES))

CONNECT_RE = re.compile(r"client::connect(_stream)?\b")
RUSSH_RE = re.compile(r"\brussh(_sftp)?::|\buse russh\b")
EXTSSH_RE = re.compile(r'Command::new\("ssh"\)')
COMMENT_RE = re.compile(r"^\s*(//|\*|/\*)")


def sh(cmd: str) -> tuple[int, str]:
    p = subprocess.run(["bash", "-o", "pipefail", "-c", cmd],
                       cwd=WT, capture_output=True, text=True)
    return p.returncode, p.stdout


def ruler_a() -> list[str]:
    """尺子甲：PM 用的那把 `grep -rln`，逐字。"""
    needle = r"ssh2\|russh\|openssh\|Command::new(\"ssh\")"
    rc, out = sh(f"grep -rln 'ssh2\\|russh\\|openssh\\|Command::new(\"ssh\")' "
                 f"{' '.join(SCAN_DIRS)}")
    _ = needle
    return sorted(out.split())


def ruler_b() -> tuple[list[str], list[str]]:
    """尺子乙：Python 走 `git ls-files`（索引里跟踪的），逐文件 `re` 匹配同一组 needle。"""
    rc, out = sh("git ls-files -z -- " + " ".join(SCAN_DIRS))
    tracked = [p for p in out.split("\0") if p]
    hits = []
    for rel in tracked:
        try:
            text = (WT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if NEEDLE_RE.search(text):
            hits.append(rel)
    return sorted(hits), tracked


def classify(rel: str) -> dict:
    text = (WT / rel).read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()
    code = [ln for ln in lines if not COMMENT_RE.match(ln)]
    code_txt = "\n".join(code)
    n_connect = len(CONNECT_RE.findall(code_txt))
    n_russh = len(RUSSH_RE.findall(code_txt))
    n_ext = len(EXTSSH_RE.findall(text))
    n_hits_all = len(NEEDLE_RE.findall(text))
    if n_connect:
        cls = "A 发起SSH连接"
    elif n_russh:
        cls = "B 用已有russh会话"
    elif n_ext:
        cls = "C 起外部ssh进程"
    else:
        cls = "D 只提了一句"
    return {"file": rel, "class": cls, "hits_all": n_hits_all,
            "code_connect": n_connect, "code_russh": n_russh, "ext_ssh_cmd": n_ext}


def cargo_targets() -> dict:
    rc, out = sh("git ls-files -z | tr '\\0' '\\n' | grep 'Cargo.toml$'")
    manifests = sorted(out.split())
    rows = []
    for m in manifests:
        txt = (WT / m).read_text(encoding="utf-8", errors="replace")
        pkg = re.search(r'^\[package\]\s*$\n(?:.*\n)*?^\s*name\s*=\s*"([^"]+)"',
                        txt, re.M)
        root = (WT / m).parent
        # 🔴 显式 `[[bin]]` 里已经点名的 `path`，**不再算一次自动发现** ——
        #    初版没扣，把 `remote-daemon-proto` 那一个 bin 数成了两个
        #    （`[[bin]] path = "src/main.rs"` 与自动发现的 `src/main.rs` 是**同一个目标**）。
        #    09-05 自查逮到，逐字记在这里。〔`brief` 第 12 条：分母怎么数的要写明〕
        declared_paths = set(re.findall(r'^\s*path\s*=\s*"([^"]+)"', txt, re.M))
        auto = []
        if (root / "src" / "main.rs").is_file() and "src/main.rs" not in declared_paths:
            auto.append("src/main.rs")
        bindir = root / "src" / "bin"
        if bindir.is_dir():
            auto += [f"src/bin/{p.name}" for p in sorted(bindir.glob("*.rs"))
                     if f"src/bin/{p.name}" not in declared_paths]
        rows.append({
            "manifest": m,
            "package": pkg.group(1) if pkg else None,
            "explicit_bin_blocks": len(re.findall(r"^\[\[bin\]\]", txt, re.M)),
            "declared_target_paths": sorted(declared_paths),
            "explicit_example_blocks": len(re.findall(r"^\[\[example\]\]", txt, re.M)),
            "explicit_test_blocks": len(re.findall(r"^\[\[test\]\]", txt, re.M)),
            "auto_bins": auto,
            "has_tests_dir": (root / "tests").is_dir(),
            "has_examples_dir": (root / "examples").is_dir(),
            "has_benches_dir": (root / "benches").is_dir(),
        })
    return {"manifests": len(manifests), "rows": rows}


def cfg_test_census() -> dict:
    rc, out = sh("git ls-files -z -- " + " ".join(SCAN_DIRS))
    files = [p for p in out.split("\0") if p]
    total = 0
    per_file = {}
    for rel in files:
        n = (WT / rel).read_text(encoding="utf-8", errors="replace").count("#[cfg(test)]")
        if n:
            per_file[rel] = n
            total += n
    return {"files_scanned": len(files), "files_with_cfg_test": len(per_file),
            "cfg_test_attrs_total": total,
            "ssh_source_rs": per_file.get("src-tauri/src/ssh_source.rs", 0)}


def main() -> int:
    a = ruler_a()
    b, tracked = ruler_b()
    only_a = [x for x in a if x not in b]
    only_b = [x for x in b if x not in a]
    both = [x for x in a if x in b]

    rep = {
        "worktree": str(WT),
        "head": sh("git rev-parse HEAD")[1].strip(),
        "ruler_a_grep_rln": {"count": len(a), "files": a},
        "ruler_b_python_over_git_ls_files": {"count": len(b), "files": b,
                                             "population": len(tracked)},
        "two_rulers_agree_on_both_sides_population": len(only_a) == 0 and len(only_b) == 0,
        "only_in_a": only_a,
        "only_in_b": only_b,
        "classification": [classify(f) for f in both],
        "cargo_targets": cargo_targets(),
        "cfg_test": cfg_test_census(),
    }

    print("=" * 92)
    print(f"W-F1b · SSH 承载面普查    树={WT}    HEAD={rep['head']}")
    print("=" * 92)
    print(f"【① §0a② 那把 grep 的份数】")
    print(f"  尺子甲 grep -rln（工作树上的文件）      ⇒ {len(a)} 份")
    print(f"  尺子乙 python+git ls-files（跟踪的文件）⇒ {len(b)} 份"
          f"（人群 {len(tracked)} 份 .rs/其它跟踪文件）")
    print(f"  两把尺子逐份一致：{rep['two_rulers_agree_on_both_sides_population']}"
          f"   只在甲={only_a or '无'}   只在乙={only_b or '无'}")
    print("-" * 92)
    print(f"【② 逐份分类（PM §0a⑤① 明说没逐份看的那一格）】")
    print(f"  {'文件':<46}{'档':<18}{'总命中':>7}{'代码connect':>12}{'代码russh':>10}{'外部ssh':>8}")
    for c in rep["classification"]:
        print(f"  {c['file']:<46}{c['class']:<18}{c['hits_all']:>7}"
              f"{c['code_connect']:>12}{c['code_russh']:>10}{c['ext_ssh_cmd']:>8}")
    tally: dict = {}
    for c in rep["classification"]:
        tally[c["class"]] = tally.get(c["class"], 0) + 1
    print(f"  合计：{tally}")
    print("-" * 92)
    print("【③ cargo 目标分母（§0a③ / WF1bD2）】")
    ct = rep["cargo_targets"]
    print(f"  Cargo.toml 清单 {ct['manifests']} 份")
    print(f"  {'manifest':<44}{'包名':<20}{'[[bin]]':>8}{'自动发现的bin':>16}"
          f"{'tests/':>8}{'examples/':>11}")
    for r in ct["rows"]:
        print(f"  {r['manifest']:<44}{str(r['package']):<20}{r['explicit_bin_blocks']:>8}"
              f"{str(r['auto_bins'] or '—'):>16}{str(r['has_tests_dir']):>8}"
              f"{str(r['has_examples_dir']):>11}")
    tot_expl = sum(r["explicit_bin_blocks"] for r in ct["rows"])
    tot_auto = sum(len(r["auto_bins"]) for r in ct["rows"])
    print(f"  ⇒ [[bin]] 显式块合计 = {tot_expl} · 自动发现的 bin 合计 = {tot_auto}"
          f"（已扣掉被显式 path 点过名的，不重复计）"
          f" · **真正的 bin 目标数 = {tot_expl + tot_auto}**")
    print(f"  ⇒ tests/ 目录 = {sum(1 for r in ct['rows'] if r['has_tests_dir'])} 个 ·"
          f" examples/ 目录 = {sum(1 for r in ct['rows'] if r['has_examples_dir'])} 个 ·"
          f" [[example]] 块 = {sum(r['explicit_example_blocks'] for r in ct['rows'])} 个")
    print("-" * 92)
    ct2 = rep["cfg_test"]
    print(f"【④ #[cfg(test)] 分母】扫了 {ct2['files_scanned']} 份跟踪文件 ⇒ "
          f"{ct2['files_with_cfg_test']} 份里有，共 {ct2['cfg_test_attrs_total']} 处；"
          f"其中 ssh_source.rs 一份就有 {ct2['ssh_source_rs']} 处")
    print("=" * 92)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
