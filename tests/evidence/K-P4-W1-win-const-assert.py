#!/usr/bin/env python3
"""K-P4 下一拍 · 那条只在 Windows 编译时开口的断言，**真的编得过、也真的会炸**。

# 它为什么要单独有一把尺子

本拍买的那一格（「Windows 上握手帧第四条面不许是空表」）分两半：

  ① **答案怎么算** —— `tmux_present(平台, PATH)` 把平台当**入参** ⇒ 在这台 Linux 上
     真跑得出读数（判据 `the_windows_answer_is_confirmed_absent_not_unknown`）。
  ② **Windows 上真的选了那一档** —— 那是 `TMUX_PLATFORM` 那一行的事。
     Linux 侧只有源码文本判据（`the_windows_arm_is_wired_into_the_source`）；
     真正读「编进去的是哪一支」的是 `main.rs` 里那条 `#[cfg(windows)] const _`，
     而它**在本仓门禁上根本不存在**（本机是 Linux）。

⇒ 于是 ② 有一个天然的假绿：那条断言可以是**写错的**（编不过 / 恒真），
   而 Linux 上没有任何东西会说。本尺子就是去掉这个假绿的。

# 它证到什么、证不到什么

· **证**：那个构造（`const _: () = assert!(matches!(常量, 变体), "…")`）在
  `x86_64-pc-windows-msvc` 上**编得过**；把 windows 那一支改成 `NoOpinion` 之后
  同一次编译**当场 E0080**（也就是说它不是恒真的摆设）；同一份「坏」代码给 linux target
  却**照样编过** —— 那正是「Linux 门禁看不见这一格」的正面读数。
· **证不到**：整个 daemon crate 在 Windows 上编得过。那一格要 `zig`
  （`ring` 的 build script 要编 C，cc-rs 在 Linux 上找不到 `lib.exe`），
  而 devbox 镜像里**没有 zig**（现打 `which zig` ⇒ 空）。那一格归 CI
  （`.github/workflows/ci.yml` 那条 `cargo check --all-targets --target x86_64-pc-windows-msvc`）。

# 跑法

    python3 evidence/K-P4-W1-win-const-assert.py

宿主上不编译任何东西：两趟 `rustc` 都在 `ccmon-devbox:latest` 里跑。
退出码：0 = 两条读数都拿到；3 = 量不了（没镜像 / 装不上 target）。
"""

import subprocess
import sys

IMAGE = "ccmon-devbox:latest"

# 与 `remote-daemon-proto/src/main.rs` 里那一段**同形**（不是同一份文本：那边带真注释，
# 这里只留承重的骨架）。⚠ 骨架变了就要跟着改这里 —— 本尺子不自动对拍那两份。
PROBE = """#![allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TmuxPlatform { AskThePath, AbsentUnlessExeOnPath, NoOpinion }

const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {
    TmuxPlatform::__WIN_ARM__
} else if cfg!(unix) {
    TmuxPlatform::AskThePath
} else {
    TmuxPlatform::NoOpinion
};

#[cfg(windows)]
const _: () = assert!(
    matches!(TMUX_PLATFORM, TmuxPlatform::AbsentUnlessExeOnPath),
    "Windows 上平台这一维必须是「确证没有」而不是「不知道」"
);
"""


def rustc(src: str, target: str):
    """在沙箱里给 `target` 编一次这份源码，回 (退出码, 输出)。"""
    # ⚠ `cd /tmp` 不是装饰：镜像的默认工作目录在容器里**不可写**，
    #   而 `rustc` 要在 cwd 建临时目录 ⇒ 不 cd 的话正向那趟会以
    #   `couldn't create a temp dir: Permission denied` 假红（第一版实测撞上了）。
    script = (
        "rustup target add %s >/dev/null 2>&1; cd /tmp; "
        "cat > /tmp/probe.rs <<'RS'\n%s\nRS\n"
        "rustc --target %s --emit=metadata --crate-type lib /tmp/probe.rs 2>&1"
    ) % (target, src, target)
    p = subprocess.run(
        ["docker", "run", "--rm", "--network", "host", "-e", "HOME=/home/zbl",
         IMAGE, "bash", "-lc", script],
        capture_output=True, text=True,
    )
    return p.returncode, (p.stdout + p.stderr).strip()


def main() -> int:
    have = subprocess.run(["docker", "image", "inspect", IMAGE],
                          capture_output=True, text=True)
    if have.returncode != 0:
        print("量不了：没有镜像 %s（不许退回宿主跑）" % IMAGE)
        return 3

    win = "x86_64-pc-windows-msvc"
    good = PROBE.replace("__WIN_ARM__", "AbsentUnlessExeOnPath")
    bad = PROBE.replace("__WIN_ARM__", "NoOpinion")

    rc_good, out_good = rustc(good, win)
    if rc_good != 0 and "error: could not find" in out_good:
        print("量不了：装不上 %s 的 std ——\n%s" % (win, out_good[:400]))
        return 3
    print("【正向】windows target 编那条断言 ⇒ EXIT=%d" % rc_good)
    if out_good:
        print("  输出：%s" % out_good[:300])

    rc_bad, out_bad = rustc(bad, win)
    print("【反向】windows 那一支改成 NoOpinion ⇒ EXIT=%d" % rc_bad)
    first = next((l for l in out_bad.splitlines() if "E0080" in l or "error" in l), "")
    print("  第一条错：%s" % first[:200])

    rc_lin, _ = rustc(bad, "x86_64-unknown-linux-gnu")
    print("【对照】同一份**坏**代码给 linux target ⇒ EXIT=%d" % rc_lin)

    ok = rc_good == 0 and rc_bad != 0 and rc_lin == 0
    print()
    print("结论：%s" % (
        "构造成立、断言真会炸、而 Linux 那侧看不见它（三条都对上了）" if ok else
        "🔴 三条读数没对上 —— 别把这一格当买到了"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
