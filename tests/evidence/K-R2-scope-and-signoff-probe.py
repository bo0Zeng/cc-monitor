#!/usr/bin/env python3
"""K-R2 下一拍的量具：判据作用域那两件事的**设计读数**（不是判定读数）。

住址（只属于本道，别人不要覆盖）：`evidence/K-R2-scope-and-signoff-probe.py`
被测对象：**命令行给的那棵树**（必须显式给，不猜、不用脚本自己的位置推）——
理由是 brief 风险 `5k`：同一住址下先后住过两份「被测对象指向不同树」的量具时，
输出长得一模一样，那是一次**静默的假读数**。

用法：
    python3 evidence/K-R2-scope-and-signoff-probe.py <工作树绝对路径>

它出三段读数：
  ① 每棵树的 `.rs` 份数 + 两根针的命中住址（`panorama_seam_registry::engine_port_scope`
     那张 `TREES` 的地板与期望住址表是照这一段定的）；
  ② 两份清单依赖段里的仓内 `path` 依赖（那条「分母自己也钉住」的判据的分母）；
  ③ 仓内各 crate 生产段的**写面 / 起进程面**（`readonly_guard::g6_dependency_signoff`
     里 `已量·未见写面` / `已量·有写面` 两档的尺子就是这一段）。

⚠ **口径与判据不完全同**，写清免得有人拿它当判定读数：
  · 本脚本剥注释只剥「整行 `//`」+「`#[cfg(test)] mod X { … }` 到列 0 的 `}`」，
    而判据走 `guard_core::production_code`（还剥块注释与**行尾**注释）
    ⇒ 判据看到的文本 **⊆** 本脚本看到的文本 ⇒ 本脚本的命中数是**上界**。
  · 本脚本的针是**裸子串**，判据的针带**词边界**（`guard_core::contains_word`）
    ⇒ 同一个方向：本脚本可能多报，不会少报。
  ⇒ **判定读数只有一个来源：沙箱门禁里 `cargo test` 的真实输出。**
"""

import os
import sys

FS_MUTATION = [
    "fs::write",
    "fs::create_dir",
    "fs::remove_file",
    "fs::remove_dir",
    "fs::rename",
    "fs::copy",
    "fs::hard_link",
    "fs::soft_link",
    "fs::symlink",
    "fs::set_permissions",
    "File::create",
    "File::options",
    "OpenOptions",
]
SPAWN = ["Command::new"]

TREES = [
    ("monitor", "src-tauri/src"),
    ("daemon", "remote-daemon-proto/src"),
    ("共享 crate", "src-tauri/crates"),
]
NEEDLES = ["Engine::open", "code_picture_core"]
MANIFESTS = [("src-tauri/Cargo.toml", "src-tauri"),
             ("remote-daemon-proto/Cargo.toml", "remote-daemon-proto")]


def strip_line_comments(text):
    return "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("//"))


def strip_test_mods(text):
    """剥 `#[cfg(...test...)]` + 下一行 `mod X {` 到**列 0 的 `}`** 那一段。

    与 `guard_core::test_module_ranges` 同一条判据（属性下一行以 `mod ` 打头、以 `{` 收尾）。
    """
    lines = text.split("\n")
    out, i = [], 0
    while i < len(lines):
        head = lines[i].strip()
        nxt = lines[i + 1].strip() if i + 1 < len(lines) else ""
        if head.startswith("#[cfg(") and "test" in head and nxt.startswith("mod ") and nxt.endswith("{"):
            i += 2
            while i < len(lines) and not lines[i].startswith("}"):
                i += 1
            i += 1
            continue
        out.append(lines[i])
        i += 1
    return "\n".join(out)


def production(text):
    return strip_line_comments(strip_test_mods(text))


def rs_files(root):
    for dirpath, _dirs, files in os.walk(root):
        for f in sorted(files):
            if f.endswith(".rs"):
                yield os.path.join(dirpath, f)


def path_deps(manifest_text, home):
    """清单**依赖段**里的 `path = "…"`，归一化成仓根相对目录。"""
    out, in_deps = [], False
    for line in manifest_text.split("\n"):
        entry = line.strip()
        if entry.startswith("["):
            in_deps = entry.endswith("dependencies]")
            continue
        if not in_deps or entry.startswith("#"):
            continue
        quoted = entry.split('"')
        for k in range(1, len(quoted), 2):
            before = quoted[k - 1]
            if "path" not in before.replace("paths", ""):
                continue
            norm = []
            for part in (home + "/" + quoted[k]).split("/"):
                if part in (".", ""):
                    continue
                if part == "..":
                    if norm:
                        norm.pop()
                    continue
                norm.append(part)
            out.append("/".join(norm))
    return out


def dep_entries(manifest_text):
    """清单依赖段逐条 `(段, crate 名)` —— 与判据 `dep_entries` 同一条判据。"""
    out, section = [], ""
    for line in manifest_text.split("\n"):
        entry = line.strip()
        if entry.startswith("["):
            section = entry if entry.endswith("dependencies]") else ""
            continue
        if not section or entry.startswith("#") or (line[:1].isspace() if line else False):
            continue
        if "=" not in entry:
            continue
        name = entry.split("=", 1)[0].strip()
        if not name or not all(c.isalnum() or c in "-_" for c in name):
            continue
        out.append((section, name))
    return sorted(set(out))


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    root = os.path.abspath(sys.argv[1])
    print(f"被测对象（那棵树）：{root}")
    print()

    print("① 每棵树的份数与两根针的命中住址")
    for label, rel in TREES:
        d = os.path.join(root, rel)
        if not os.path.isdir(d):
            print(f"  {label:10s} {rel:26s} 读不到这棵树 —— 判不了，不是 0")
            continue
        n, hits = 0, {needle: [] for needle in NEEDLES}
        for p in rs_files(d):
            n += 1
            prod = production(open(p, encoding="utf-8").read())
            for needle in NEEDLES:
                if needle in prod:
                    hits[needle].append(os.path.relpath(p, d))
        print(f"  {label:10s} {rel:26s} {n:4d} 份 .rs")
        for needle in NEEDLES:
            print(f"      针 {needle:20s} {len(hits[needle])} 处：{hits[needle]}")
    print()

    print("② 两份清单依赖段里的仓内 path 依赖（那条「分母自己也钉住」的分母）")
    dirs = []
    for rel, home in MANIFESTS:
        text = open(os.path.join(root, rel), encoding="utf-8").read()
        got = path_deps(text, home)
        print(f"  {rel:34s} {len(got)} 条")
        dirs.extend(got)
    uniq = sorted(set(dirs))
    print(f"  去重合计 {len(uniq)} 条：")
    for d in uniq:
        print(f"      {d}")
    print()

    print("③ 仓内 crate 生产段的写面 / 起进程面（`已量·*` 两档的尺子）")
    crates = os.path.join(root, "src-tauri/crates")
    for c in sorted(os.listdir(crates)):
        src = os.path.join(crates, c, "src")
        if not os.path.isdir(src):
            continue
        n, hits = 0, {}
        for p in rs_files(src):
            n += 1
            prod = production(open(p, encoding="utf-8").read())
            for pat in FS_MUTATION + SPAWN:
                k = prod.count(pat)
                if k:
                    hits[f"{os.path.relpath(p, src)} {pat}"] = k
        print(f"  {c:20s} {n} 份 .rs · 命中 {len(hits)} 处：{hits if hits else '（0 处）'}")
    print()

    print("④ daemon 清单的依赖面（那张签字表的分母）")
    text = open(os.path.join(root, "remote-daemon-proto/Cargo.toml"), encoding="utf-8").read()
    entries = dep_entries(text)
    print(f"  依赖段 {sorted({s for s, _ in entries})}")
    print(f"  逐条 {len(entries)} 条：{[n for _, n in entries]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
