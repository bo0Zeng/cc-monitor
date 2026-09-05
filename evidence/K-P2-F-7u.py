#!/usr/bin/env python3
"""K-P2 `F` 拍的 `7u`：**把实现整个退掉，还有多少条新断言仍绿**。

用法（宿主上跑，判据一律进沙箱容器）：
    python3 evidence/K-P2-F-7u.py <被测工作树绝对路径> <退回到哪个提交>

做法：只把 `shared/ccm` 换成 `<提交>:shared/ccm` 那份（**判据文件一个字节不动**），
逐条跑本拍新增/改写的那几条判据，看哪几条仍绿。跑完逐字节还原（比 md5）。

为什么这条要单独量（`brief` 第 129-130 行）：
  「仍绿不等于仪式：它的牙可能在别的刀上。」——但**一条都不留地说「全红」也要有读数**，
  不能靠推演。本仓三份审计独立判过，真空转的分别是 1 / 0 / 0 条。

⚠ 射程如实写：它退掉的是 `shared/ccm` 那一半（**实现**），
  没退 `ccm_cli_contract.rs` 那一半（**判据**）—— 退了判据就没有断言可数了。
"""

import hashlib
import pathlib
import subprocess
import sys

IMAGE = "ccmon-devbox:latest"
PROJ = "/home/zbl/文档/claudecode-frontend"

# 本拍**新增或改写**的判据（`git diff` 里带 `fn <名>` 的那几条）。
JUDGES = [
    ("the_backend_unreachable_failure_face_has_exactly_one_home", "新增（`KP2C` 唯一失败面）"),
    ("the_local_launch_recipe_is_reachable_only_from_print", "新增（`KP2C ①` 可达性）"),
    ("every_backend_backed_path_in_ccm_keeps_an_observable_fallback", "改写（三档 + 第二条棘轮）"),
    ("an_explicit_tmux_name_collision_fails_loudly", "改写（一处定义 + 每个报出口都响亮）"),
    # 对照：本拍**没改**的两条，用来证「退掉实现」不是把整个模块打红
    ("the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move", "对照 · 本拍零改动（`KP2D`）"),
    ("ccm_cli_strength_is_at_or_above_baseline", "对照 · 本拍零改动（`KP2B` 强度基线）"),
]


def run_one(tree: pathlib.Path, name: str) -> str:
    p = subprocess.run(
        [
            "docker", "run", "--rm", "--network", "none",
            "-v", f"{PROJ}:{PROJ}",
            "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/k-p2f",
            "-e", "HOME=/home/zbl",
            "-w", f"{tree}/src-tauri", IMAGE,
            "bash", "-o", "pipefail", "-c", f"cargo test --lib {name} 2>&1",
        ],
        capture_output=True, text=True,
    )
    for ln in (p.stdout + p.stderr).splitlines():
        if ln.startswith("test result:"):
            return ln.strip()
    return "<抓不到判定行 —— 按 CRASH 记>"


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    tree = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    head = subprocess.run(["git", "-C", str(tree), "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    print(f"# 被测树：{tree}")
    print(f"# 量具：  {pathlib.Path(__file__).resolve()}")
    print(f"# HEAD = {head}；`7u` = 只把 `shared/ccm` 退回 {rev}，判据文件一个字节不动")

    ccm = tree / "shared/ccm"
    now = ccm.read_bytes()
    md5_now = hashlib.md5(now).hexdigest()
    old = subprocess.run(["git", "-C", str(tree), "show", f"{rev}:shared/ccm"],
                         capture_output=True).stdout
    assert old and old != now, "取不到退回那份、或它与现在一样"

    ccm.write_bytes(old)
    try:
        rows = [(n, why, run_one(tree, n)) for n, why in JUDGES]
    finally:
        ccm.write_bytes(now)
        assert hashlib.md5(ccm.read_bytes()).hexdigest() == md5_now, "还原不逐字节！"

    print()
    print("| 判据 | 本拍动过它吗 | 退掉实现之后的判定行 |")
    print("|---|---|---|")
    still_green = 0
    for n, why, line in rows:
        if line.startswith("test result: ok"):
            still_green += 1
        print(f"| `{n}` | {why} | `{line}` |")
    print()
    changed = [r for r in rows if not r[1].startswith("对照")]
    green_changed = [r for r in changed if r[2].startswith("test result: ok")]
    print(f"⇒ 本拍**动过的** {len(changed)} 条里，退掉实现之后**仍绿 {len(green_changed)} 条**"
          f"（分母 = 上表里不带「对照」的那几行）。")
    print(f"⇒ 全表 {len(rows)} 条里仍绿 {still_green} 条（含对照）。")
    print(f"⚠ 还原对拍：这一行打得出来就说明 `shared/ccm` 逐字节回到 HEAD 那份（md5 {md5_now}）。")


if __name__ == "__main__":
    main()
