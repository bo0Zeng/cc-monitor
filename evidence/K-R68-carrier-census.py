#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R68 载体普查量具 —— 「后端二进制有几种载体、它们是不是同一次构建的产物」。

只读。一条命令重打全部读数。每一节自己印**分母怎么数的**与**量于哪棵树**。

⚠ 本量具**跨三个被测对象**，三者的分母不同，别混着读：
  A) 工作树 `.claude/worktrees/k-r68`（本件的树，`--tree-a`）—— 源码面的事实
  B) 主工作树 `cc-monitor`（`--tree-b`）—— 盘上**真的铺着**载体的那棵树
  C) 本机 `$HOME/.cc-monitor`（`--home`）—— 用户机器上**真的落着**的那一份

住址（唯一）：`evidence/K-R68-carrier-census.py`；输出落 `evidence/K-R68-carrier-census.out`。
量不到就印 `?` 当场喊，不静默算 0。
"""

import hashlib
import os
import re
import subprocess
import sys
from pathlib import Path

TREE_A = Path(
    os.environ.get("KR68_TREE_A", "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r68")
)
TREE_B = Path(os.environ.get("KR68_TREE_B", "/home/zbl/文档/claudecode-frontend/cc-monitor"))
HOME_CC = Path(os.environ.get("KR68_HOME_CC", str(Path.home() / ".cc-monitor")))


def sh(args, cwd):
    try:
        r = subprocess.run(args, cwd=str(cwd), capture_output=True, text=True, timeout=120)
        return r.returncode, r.stdout.strip(), r.stderr.strip()
    except Exception as e:  # 量不到就喊
        return -1, "", f"{type(e).__name__}: {e}"


def md5(p: Path):
    try:
        h = hashlib.md5()
        with p.open("rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception as e:
        return f"?({type(e).__name__})"


def readtxt(p: Path):
    try:
        return p.read_text(encoding="utf-8", errors="replace").strip()
    except Exception:
        return None


def head(title):
    print()
    print("=" * 78)
    print(title)
    print("=" * 78)


# 三种载体的**住址闭集**：(编号, 目录相对 src-tauri, 文件名模式, 消费侧住址)
CARRIERS = [
    (
        "①内嵌·供本机",
        "native-daemon",
        ["cc-monitor-native", "cc-monitor-native.build_id", "cc-monitor-native.target"],
        "src-tauri/src/backend/control/local_backend.rs 的 include_bytes!"
        "（`native_embedded_daemon`）；名字定死在 build.rs::NATIVE_DAEMON_DIR/FILE",
    ),
    (
        "②内嵌·供推远端",
        "embedded-daemons",
        [
            "cc-monitor-remote-x86_64",
            "cc-monitor-remote-x86_64.build_id",
            "cc-monitor-remote-aarch64",
            "cc-monitor-remote-aarch64.build_id",
        ],
        "src-tauri/build.rs::embed_daemons 拷进 OUT_DIR → src-tauri/src/sftp.rs "
        "的 include_bytes!(concat!(env!(\"OUT_DIR\"), …))",
    ),
    (
        "③安装包那份",
        "binaries",
        ["cc-monitor-remote-<triple>[.exe]"],
        "src-tauri/tauri.sidecar.conf.json 的 externalBin → 装完落在 monitor 旁边；"
        "消费侧 local_backend::resolve_beside_this_exe / resolve_with",
    ),
]


def sect_1_addresses():
    head("§1 三种载体的住址（源码面，量于 tree-a）")
    rc, sha, _ = sh(["git", "rev-parse", "HEAD"], TREE_A)
    print(f"tree-a = {TREE_A}  HEAD = {sha or '?'}")
    rc, st, _ = sh(["git", "status", "--porcelain"], TREE_A)
    print(f"tree-a git status --porcelain 行数 = {len(st.splitlines()) if rc == 0 else '?'}")
    for tag, d, files, consumer in CARRIERS:
        print(f"\n  {tag}  目录 src-tauri/{d}/")
        print(f"    期望文件：{', '.join(files)}")
        print(f"    消费侧：{consumer}")
    print("\n  分母 = PM 在 R24 裁定一那张表里点的 3 格；下面 §6 是我自己在盘上重扫的那一遍。")


def sect_2_tracked():
    head("§2 三种载体在 git 里吗（分母 = 上面 3 个目录，量于 tree-a）")
    rc, out, err = sh(["git", "ls-files"], TREE_A)
    if rc != 0:
        print(f"  ? git ls-files 失败：{err}")
        return
    tracked = out.splitlines()
    for tag, d, _f, _c in CARRIERS:
        hits = [p for p in tracked if p.startswith(f"src-tauri/{d}/")]
        print(f"  {tag}  src-tauri/{d}/ 被 git 跟踪的文件数 = {len(hits)}  {hits or ''}")
    gi = TREE_A / "src-tauri" / ".gitignore"
    txt = readtxt(gi) or ""
    for tag, d, _f, _c in CARRIERS:
        pat = f"/{d}/"
        print(f"  {tag}  `{pat}` 在 src-tauri/.gitignore 里出现 {txt.count(pat)} 次")


def sect_3_on_disk():
    head("§3 三种载体今天真的铺着吗（量于 tree-a 与 tree-b 各一遍）")
    for label, tree in (("tree-a k-r68", TREE_A), ("tree-b main", TREE_B)):
        rc, sha, _ = sh(["git", "rev-parse", "HEAD"], tree)
        print(f"\n  ── {label} = {tree}  HEAD={sha or '?'} ──")
        for tag, d, _f, _c in CARRIERS:
            dd = tree / "src-tauri" / d
            if not dd.is_dir():
                print(f"    {tag}  src-tauri/{d}/  **不存在**（0 个文件）")
                continue
            ents = sorted(p for p in dd.iterdir() if p.is_file())
            print(f"    {tag}  src-tauri/{d}/  {len(ents)} 个文件")
            for p in ents:
                print(f"        {md5(p)}  {p.stat().st_size:>9}  {p.name}")


def daemon_source_build_id(tree: Path):
    p = tree / "remote-daemon-proto" / "src" / "main.rs"
    txt = readtxt(p)
    if txt is None:
        return None, f"? 读不到 {p}"
    for i, line in enumerate(txt.splitlines(), 1):
        if "const BUILD_ID" in line:
            m = re.search(r'"([^"]+)"', line)
            return (m.group(1) if m else None), f"{p.relative_to(tree)}:{i}  逐字 `{line.strip()}`"
    return None, "? 源码里找不到 `const BUILD_ID`"


def sect_4_identity():
    head("§4 🔴 身份读数 —— 「它们是不是同一次构建的产物」买在这一节")
    print("  分母 = 「这台机器上今天能拿到的、后端二进制的身份串」的全部住址，逐个点名：")
    rows = []
    for label, tree in (("tree-a k-r68 源码", TREE_A), ("tree-b main 源码", TREE_B)):
        v, where = daemon_source_build_id(tree)
        rows.append((label, v or "?", where))
    for tag, d, _f, _c in CARRIERS:
        for tree_label, tree in (("tree-a", TREE_A), ("tree-b", TREE_B)):
            dd = tree / "src-tauri" / d
            if not dd.is_dir():
                rows.append((f"{tag} @{tree_label} 清单", "（目录不存在）", f"src-tauri/{d}/"))
                continue
            mans = sorted(dd.glob("*.build_id"))
            if not mans:
                rows.append(
                    (f"{tag} @{tree_label} 清单", "**无 .build_id 清单**", f"src-tauri/{d}/")
                )
            for m in mans:
                rows.append(
                    (f"{tag} @{tree_label} 清单", readtxt(m) or "?", str(m.relative_to(tree)))
                )
    bid = HOME_CC / "bin" / ".build_id"
    rows.append(
        (
            "本机已部署那一份",
            readtxt(bid) if bid.exists() else "（文件不存在）",
            str(bid),
        )
    )
    binp = HOME_CC / "bin" / "cc-monitor-remote"
    if binp.exists():
        rows.append(
            ("本机已部署那一份·字节", md5(binp) + f"  {binp.stat().st_size} B", str(binp))
        )
    w = max(len(r[0]) for r in rows)
    for a, b, c in rows:
        print(f"    {a:<{w}}  = {b}")
        print(f"    {'':<{w}}    住 {c}")
    vals = {b for a, b, c in rows if "清单" in a or "源码" in a or a == "本机已部署那一份"}
    vals = {v for v in vals if v and not v.startswith("（") and not v.startswith("**")}
    print(f"\n  ⇒ 不同取值 {len(vals)} 个：{sorted(vals)}")
    print("  ⚠ 口径：这一节量的是**身份串**，不是字节。同串不蕴含同字节（见 §5）。")


def sect_5_bumps():
    head("§5 BUILD_ID 是**手 bump 的常量**，不是派生的（量于 tree-b 的 git 历史）")
    rc, a, _ = sh(
        ["git", "log", "--format=%H", "--", "remote-daemon-proto/src"], TREE_B
    )
    n_src = len(a.splitlines()) if rc == 0 else "?"
    rc, b, _ = sh(
        [
            "git",
            "log",
            "--format=%h %ad %s",
            "--date=short",
            "-G",
            "^const BUILD_ID",
            "--",
            "remote-daemon-proto/src/main.rs",
        ],
        TREE_B,
    )
    bumps = b.splitlines() if rc == 0 else []
    print(f"  改过 `remote-daemon-proto/src/` 的提交数 = {n_src}")
    print(f"    分母怎么数的：`git log --format=%H -- remote-daemon-proto/src | wc -l`（全历史，含只改注释 / 只改测试的）")
    print(f"  diff 里动过 `^const BUILD_ID` 那一行的提交数 = {len(bumps)}")
    print(f"    分母怎么数的：`git log -G '^const BUILD_ID' -- remote-daemon-proto/src/main.rs`")
    print("  🔴 **诚实边界**：两个数**不能相减当成「漏 bump 次数」** —— 只改注释 / 只改测试的提交本来就不该 bump。")
    print("     这两个数买到的只有一件事：**BUILD_ID 不是派生的**，「同一个 BUILD_ID」不蕴含「同一份字节」。")
    print("  ⇒ 而**自陈漏过 bump** 的提交（提交信息里逐字承认的），逐条点名：")
    named = [ln for ln in bumps if ("漏" in ln and "bump" in ln) or "欠了" in ln]
    for ln in named:
        print(f"       · {ln}")
    print(f"     计数 {len(named)}（分母 = 上面那 {len(bumps)} 条 bump 提交的**提交信息**，人读判定，不是脚本语义）")


def sect_6_sweep():
    head("§6 🔴 我自己在盘上重扫一遍「随产品分发的可执行字节」（不照 PM 的名单数）")
    print("  人群定义（分母怎么数的）：**会以可执行字节的形态落到用户机器上的东西**，四条产出路各扫一遍——")
    print("    (a) Tauri 打包面：tauri.conf.json / tauri.sidecar.conf.json 的 externalBin + resources")
    print("    (b) 编译期内嵌：产品面 .rs 里的 include_bytes!（逐处看它嵌的是不是可执行体）")
    print("    (c) 运行期拉取：我们自己去网上取一个二进制下来跑")
    print("    (d) 运行期再落一份：把已经落下的二进制**再拷 / 再改名**一份")
    print()
    # (a)
    for conf in ("tauri.conf.json", "tauri.sidecar.conf.json"):
        txt = readtxt(TREE_A / "src-tauri" / conf) or ""
        eb = re.findall(r'"externalBin"\s*:\s*\[(.*?)\]', txt, re.S)
        res = re.findall(r'"resources"\s*:\s*\[(.*?)\]', txt, re.S)
        print(f"  (a) {conf}: externalBin={eb or '无'}  resources={res or '无'}")
    # (b)
    rc, out, _ = sh(
        ["grep", "-rn", "--include=*.rs", "include_bytes!", "src-tauri/src", "src-tauri/build.rs"],
        TREE_A,
    )
    lines = [l for l in out.splitlines() if "include_bytes!(" in l and not l.split(":", 2)[2].strip().startswith("//")]
    print(f"\n  (b) 产品面 include_bytes! 真调用处 = {len(lines)}（分母 = src-tauri/src + build.rs 下、剥掉整行注释后仍含 `include_bytes!(` 的行）")
    groups = {}
    for l in lines:
        f = l.split(":", 1)[0]
        groups.setdefault(f, []).append(l.split(":", 2)[2].strip())
    for f, ls in sorted(groups.items()):
        print(f"      {f}  {len(ls)} 处")
        for x in ls:
            print(f"          {x}")
    # (c)
    rc, out, _ = sh(
        ["grep", "-rln", "--include=*.rs", "codepicture", "remote-daemon-proto/src"], TREE_A
    )
    print(f"\n  (c) 运行期拉取：remote-daemon-proto/src 下提到 codepicture 的文件 {len(out.splitlines())} 份")
    for f in out.splitlines():
        print(f"      {f}")
    # (d)
    rc, out, _ = sh(
        [
            "grep",
            "-rn",
            "--include=*.rs",
            "fs::copy(",
            "src-tauri/src/backend/control/local_backend.rs",
        ],
        TREE_A,
    )
    print(f"\n  (d) local_backend.rs 里的 fs::copy( 调用处 = {len(out.splitlines())}")
    for l in out.splitlines():
        print(f"      {l}")


def sect_7_closedset():
    head("§7 闭集今天怎么说（量于 tree-a 的 src-tauri/src/tool_registry.rs）")
    txt = readtxt(TREE_A / "src-tauri" / "src" / "tool_registry.rs") or ""
    m = re.search(r"pub const TOOLS: &\[ToolSpec\] = &\[(.*?)\n\];", txt, re.S)
    body = m.group(1) if m else ""
    ids = re.findall(r'^\s{8}id: "([^"]+)"', body, re.M)
    print(f"  TOOLS 成员 {len(ids)} 条（分母 = `pub const TOOLS` 那个字面量块里缩进 8 空格的 `id:` 行）：")
    for i in ids:
        print(f"      · {i}")
    m2 = re.search(r"pub const UNMANAGED_ENV: &\[UnmanagedEnv\] = &\[(.*?)\n\];", txt, re.S)
    b2 = m2.group(1) if m2 else ""
    ids2 = re.findall(r'^\s{8}id: "([^"]+)"', b2, re.M)
    print(f"\n  UNMANAGED_ENV 成员 {len(ids2)} 条：")
    for i in ids2:
        print(f"      · {i}")
    variants = re.findall(r"^\s{4}([A-Z]\w+)(?:\(|\s*\{|,)", re.search(
        r"pub enum ToolDestination \{(.*?)\n\}", txt, re.S).group(1), re.M) if re.search(
        r"pub enum ToolDestination \{(.*?)\n\}", txt, re.S) else []
    print(f"\n  ToolDestination 变体 {len(variants)} 个：{variants}")
    print("  ⚠ 现算，不写死〔13b〕。")


def sect_8_ci():
    head("§8 三条产出路在 release.yml 上的住址（量于 tree-a 的 .github/workflows/release.yml）")
    p = TREE_A / ".github" / "workflows" / "release.yml"
    txt = readtxt(p) or ""
    interesting = [
        "Cross-compile daemon for both musl targets",
        "Stage binaries",
        "Place + verify embedded daemons",
        "Build local backend sidecar (native)",
        "Stage sidecar for externalBin",
        "Stage native daemon for self-extract",
        "tauri build",
        "Place embedded daemons",
        "tauri build (deb)",
    ]
    for i, line in enumerate(txt.splitlines(), 1):
        s = line.strip()
        if s.startswith("- name:") and any(k in s for k in interesting):
            print(f"  release.yml:{i:<4} {s}")
        if s.startswith("jobs:") or re.match(r"^  [a-z-]+:$", line):
            if re.match(r"^  (ci-gate|build-daemons|build-windows|build-linux):$", line):
                print(f"  release.yml:{i:<4} ── job {s}")
    print("\n  ⇒ ①与③ 在 build-windows 里**从同一个文件拷两份**，中间没有任何重编步骤；")
    print("    ② 在另一个 job（ubuntu，cargo zigbuild，musl target）里产。逐字见那几步的 run:。")


def main():
    print("K-R68 载体普查 —— 量具住址 evidence/K-R68-carrier-census.py")
    print(f"量于（本机时钟）：{subprocess.run(['date', '-Is'], capture_output=True, text=True).stdout.strip()}")
    print(f"tree-a = {TREE_A}\ntree-b = {TREE_B}\nhome   = {HOME_CC}")
    sect_1_addresses()
    sect_2_tracked()
    sect_3_on_disk()
    sect_4_identity()
    sect_5_bumps()
    sect_6_sweep()
    sect_7_closedset()
    sect_8_ci()
    print("\n===== 普查完 =====")


if __name__ == "__main__":
    sys.exit(main())
