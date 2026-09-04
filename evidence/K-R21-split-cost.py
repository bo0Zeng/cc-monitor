#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R21 第三把尺子：`KR21D2` ——**把三义拆开要付多少钱**，两条候选各自算。

🔴 **本尺子不选路** —— `KR21D2` 明令「不许自批选一条」。它只把两笔账算出来。

# 甲（三态返回）要付的钱：**改一个 `pub(crate)` 签名，会碰到多少个「不在写区」的文件**

`proc_env_var` 只有 3 个生产调用点（第一把尺子量的），看起来很便宜。
**但它的调用形状被好几把尺子按字面串钉着**，而那些尺子**散在两个 crate 里**。
本段就是把那个**联动面**数出来：谁按字面钉着它、住哪个 crate、**在不在本拍写区**。

  射程：`REGISTERED_WRITE_AREA` 是**本拍单子给的写区**（4 项），写死在这里。
  ⚠ 它是**本拍**的写区，不是「这文件永远不能改」——数出来的是**要不要跨拍/跨持有人**，
    不是「禁止」。

# 乙（只拎「读不到」）要论证的东西：**「键不在」≡「值是空串」对每个调用方都真等价吗**

`--rust-check` 用 **Rust 现打**回答，不靠读代码猜：
  ① 读侧：`VAR=""` 与 `VAR` 未设，`proc_env_var` 是不是**真的**同回 `None`；
  ② 写侧：同一个空串，`std::env::var_os` 回的是 `None` 还是 `Some("")`。
🔴 ② 是本段的要害：读侧把空串当「没设」，**写侧不一定**。两侧口径若不同，
   乙那条「另两支合并无害」的论证就**不是全称成立**，而是**逐调用方成立**。

# ⚠ 本尺子答不到的（射程边界，别读宽）

- 它**数不出**「改了之后哪条判据会红」——那要真改再跑门禁，本拍没落治法。
- 「代价」里的**决定成本**（每个调用方拿到新那一支该做什么）不可机检，
  写在件文件 `§7`，由 PM 裁。

# 用法

    python3 evidence/K-R21-split-cost.py [--repo <工作树根>] [--rust-check]

`--rust-check` 起进程、要 `rustc` ⇒ **只许在沙箱里跑**（`K31`，判据 `/.dockerenv`）。
纯文本那一段不起任何进程，宿主上跑也不违反。
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile

# 本拍单子给的写区（4 项）。前缀匹配。
REGISTERED_WRITE_AREA = (
    "remote-daemon-proto/src/platform/proc.rs",
    "remote-daemon-proto/src/observe/accounts_query.rs",
    "evidence/",
    ".claude/planned-build/backend-consolidation/features/K-R21-",
)

# 按**字面调用形状**钉着它的那几把尺子的针
SHAPE_NEEDLES = ('"proc_env_var(pid, ', "proc_env_var(pid, crate::", "proc_env_var(pid, LAUNCH_ID_ENV)")
SKIP_DIRS = {"target", "node_modules", ".git", "dist", ".venv"}


def in_write_area(rel: str) -> bool:
    return any(rel.startswith(p) for p in REGISTERED_WRITE_AREA)


def crate_of(rel: str) -> str:
    if rel.startswith("remote-daemon-proto/"):
        return "remote-daemon-proto"
    if rel.startswith("src-tauri/"):
        return "src-tauri"
    if rel.startswith("doc/"):
        return "doc/"
    return "<其它>"


def census(repo: str) -> None:
    me = os.path.abspath(__file__)
    rows: list[tuple[str, int, str]] = []
    scanned = 0
    for root, dirs, names in os.walk(repo):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for n in sorted(names):
            path = os.path.join(root, n)
            if os.path.abspath(path) == me:
                continue
            try:
                with open(path, "r", encoding="utf-8") as fh:
                    text = fh.read()
            except (UnicodeDecodeError, OSError):
                continue
            scanned += 1
            for i, line in enumerate(text.splitlines(), 1):
                if any(nd in line for nd in SHAPE_NEEDLES):
                    rows.append((os.path.relpath(path, repo), i, line.strip()))

    print("## 甲的联动面：**按字面调用形状钉着 `proc_env_var` 的地方**")
    print(f"   分母：可读文本文件 {scanned} 个（排除 {sorted(SKIP_DIRS)} 与本脚本）")
    print()
    out_of_area = 0
    by_crate: dict[str, int] = {}
    for rel, ln, txt in rows:
        ok = in_write_area(rel)
        if not ok:
            out_of_area += 1
        by_crate[crate_of(rel)] = by_crate.get(crate_of(rel), 0) + 1
        mark = "写区内" if ok else "🔴 写区外"
        print(f"  {mark}  [{crate_of(rel)}]  {rel}:{ln}")
        print(f"            {txt[:110]}")
    print()
    print(f"  合计 {len(rows)} 处 · **写区外 {out_of_area} 处** · 跨 {len(by_crate)} 个 crate：{by_crate}")
    print()
    print("  ⇒ 读法：甲**只要不改调用的字面形状**（`proc_env_var(pid, <键>)` 这一串保持不变，")
    print("     只换返回类型 + 在调用点后面接一个 `.某方法()`），上面这些针**一条都不动**。")
    print("     🔴 反过来，甲若顺手把函数改名/换调用形状，就要**跨 crate 联动**，")
    print("     其中 `src-tauri/src/doc_claim_registry.rs` 那一处是**跨 crate 读另一个 crate 的源码**。")
    print()


RUST_CHECK = r'''
// K-R21 乙那条论证的现打：空串与未设，两侧口径一样吗？
fn proc_env_var(pid: u32, name: &str) -> Option<String> {
    // 与 proc.rs:84..:101 逐字同形
    let bytes = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    for entry in bytes.split(|b| *b == 0) {
        if entry.is_empty() { continue; }
        let s = String::from_utf8_lossy(entry);
        if let Some(v) = s.strip_prefix(&format!("{name}=")) {
            if v.is_empty() { return None; }   // 支二
            return Some(v.to_string());
        }
    }
    None                                       // 支三
}

fn spawn_with(env: Option<&str>) -> std::process::Child {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg("exec sleep 5");
    match env {
        Some(v) => { c.env("K_R21_PROBE", v); }
        None => { c.env_remove("K_R21_PROBE"); }
    }
    c.spawn().expect("spawn")
}

fn settle(pid: u32) {
    // 等过 exec 窗口，免得把第四态混进这一格
    for _ in 0..100000 {
        if let Ok(b) = std::fs::read(format!("/proc/{pid}/environ")) {
            if !b.is_empty() { return; }
        }
    }
}

fn main() {
    println!("① 读侧（`proc_env_var`）—— 空串 vs 未设：");
    for (label, val) in [("值是空串 K_R21_PROBE=\"\"", Some("")),
                         ("压根没这个键", None),
                         ("对照：有值", Some("abc"))] {
        let mut c = spawn_with(val);
        settle(c.id());
        let got = proc_env_var(c.id(), "K_R21_PROBE");
        println!("   {label:<28} => {got:?}");
        let _ = c.kill(); let _ = c.wait();
    }
    println!();
    println!("② 写侧（`agents/claudecode/paths.rs:19` 那一句 `std::env::var_os`）——同一个空串：");
    std::env::set_var("K_R21_PROBE", "");
    println!("   var_os(\"K_R21_PROBE\") 于空串   => {:?}", std::env::var_os("K_R21_PROBE"));
    std::env::remove_var("K_R21_PROBE");
    println!("   var_os(\"K_R21_PROBE\") 于未设   => {:?}", std::env::var_os("K_R21_PROBE"));
    println!();
    println!("⇒ ① 两行同回 None ⇒ **读侧**把「空串」与「没这个键」压成了同一件事；");
    println!("⇒ ② 空串回 Some(\"\")、未设回 None ⇒ **写侧分得开**。两侧口径**不一致**。");
}
'''


def rust_check() -> None:
    print("## 乙要论证的那条：「键不在」≡「值是空串」—— Rust 现打")
    if not os.path.exists("/.dockerenv"):
        print("  ❌ `--rust-check` 起进程，只许在沙箱里跑（K31）。没看见 /.dockerenv ⇒ 跳过。")
        print("  🔴 **跳过不是「跑了没红」** —— 本格没有读数。")
        return
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "k_r21_split.rs")
        with open(src, "w", encoding="utf-8") as fh:
            fh.write(RUST_CHECK)
        exe = os.path.join(td, "k_r21_split")
        r = subprocess.run(["rustc", "-O", "-o", exe, src], capture_output=True, text=True)
        if r.returncode != 0:
            print("  ❌ rustc 编不过 —— 本格没有读数，别当成跑过了：")
            print("  " + (r.stderr or "").strip()[:900])
            return
        out = subprocess.run([exe], capture_output=True, text=True)
        for line in (out.stdout + out.stderr).splitlines():
            print("  " + line)
    print()


def main() -> int:
    ap = argparse.ArgumentParser(description="K-R21：拆开三义的代价")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--rust-check", action="store_true")
    args = ap.parse_args()
    repo = os.path.abspath(args.repo)
    try:
        sha = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                             capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        sha = "<不是 git 树>"
    print(f"# 量于提交 {sha}")
    print()
    census(repo)
    if args.rust_check:
        rust_check()
    print("🔴 本尺子**不选路**：两笔账都摆着，选哪条归 PM（`KR21D2` 明令不许自批）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
