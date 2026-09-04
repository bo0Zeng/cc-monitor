#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R8 D3 的尺子：把「哪台 server」做成**类型上必须有的值**之后，
`K-R7` 那三刀还走不走得过去。

⚠⚠ 只 `rustc` 几个**独立的临时 .rs**：不 include 仓里任何东西、不起 tmux、
    不起 daemon、不碰这台机器的任何状态、不改仓里任何文件。

# 为什么要有这一刀

`K-R7 §0s` 裁定「钉调用处」买不到那条性质，两条理由：
  ① panic 接得住（`catch_unwind` 穿过唯一入口）；
  ② 生产合法地需要那条不隔离的路 ⇒ 没有类型能让「带 shim」变成强制的。
09-01 又追加一条：块注释 + `let shim = String::new();` ⇒ 四道判据一条没红。

**这三刀有一个共同前提：变异之后代码还得编译得过**（否则门禁直接红，
连「走过去」都谈不上）。⇒ 若那条性质改由**类型**承载，三刀里有两刀当场撞墙，
第三刀（块注释）变成 **no-op 而不是 defeat**。本脚本逐刀实打这件事。

# 判据（本脚本自己的）

  N1 没人交代就调       ⇒ 必须**编译失败**（E0061）
  N2 就地造一个默认的   ⇒ 必须**编译失败**（E0599，没有 Default）
  N3 catch_unwind 穿    ⇒ 必须**编译失败**（E0061；panic 接得住，类型错接不住）
  N4 块注释掉真调用     ⇒ **编译得过**，但那次装 hook 也跟着没了 ⇒ 是 no-op

# 如实边界（别把本脚本读成「买断了」）

  ★ 它证的是「**没交代 ⇒ 装不成**」这一格由编译器把着。
  ★ 它**不**证「没有人能绕过去」：同 crate 里另写一句
    `Command::new("tmux").args(["set-hook","-g",…])` 照样编译得过、照样打到默认 server。
    那一格仍旧只有「扫源码」能查，而 `K-R7` 已经裁定那档只买得到「防手滑」。
  ⇒ 本设计换掉的是**默认值**：从「谁都没说 ⇒ 它自己去找」变成
    「谁都没说 ⇒ 它拼不出这次调用」。绕过去要**专门写一句**，不再是躺着就发生。
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PRELUDE = r'''
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TmuxServer(Selector);

#[derive(Debug, Clone)]
pub enum Selector {
    SocketName(String),
    SocketPath(PathBuf),
}

impl TmuxServer {
    /// **唯一**构造器：必须给一个选择器。没有 Default、没有 from_env。
    pub fn from_handoff(sel: Selector) -> Self { TmuxServer(sel) }
    pub fn selector_args(&self) -> Vec<String> {
        match &self.0 {
            Selector::SocketName(n) => vec!["-L".into(), n.clone()],
            Selector::SocketPath(p) => vec!["-S".into(), p.to_string_lossy().into_owned()],
        }
    }
}

const HOOK_SLOT: u32 = 50;

fn hook_set_args(server: &TmuxServer, event: &str, exe: &Path, pid: u32, starttime: u64) -> Vec<String> {
    let payload = format!("'{}' --tmux-notify {pid} {starttime}", exe.display());
    let mut v = server.selector_args();
    v.push("set-hook".into());
    v.push("-g".into());
    v.push(format!("{event}[{HOOK_SLOT}]"));
    v.push(format!("run-shell -b '{payload}'"));
    v
}
'''

PROBES = [
    (
        "N1_没人交代就调",
        False,
        "E0061",
        '''fn main() {
    let a = hook_set_args("session-closed", Path::new("/opt/d"), 42, 999);
    println!("{a:?}");
}''',
    ),
    (
        "N2_就地造一个默认的",
        False,
        "E0599",
        '''fn main() {
    let s = TmuxServer::default();
    let a = hook_set_args(&s, "session-closed", Path::new("/opt/d"), 42, 999);
    println!("{a:?}");
}''',
    ),
    (
        "N3_catch_unwind穿过去",
        False,
        "E0061",
        '''fn main() {
    let r = std::panic::catch_unwind(|| {
        hook_set_args("session-closed", Path::new("/opt/d"), 42, 999)
    });
    println!("{r:?}");
}''',
    ),
    (
        "N4_块注释掉真调用",
        True,
        None,
        '''fn main() {
    let server = TmuxServer::from_handoff(Selector::SocketName("real".into()));
    /*
    let a = hook_set_args(&server, "session-closed", Path::new("/opt/d"), 42, 999);
    println!("{a:?}");
    */
    let shim = String::new();
    println!("NO-OP shim={shim:?} server={server:?}");
}''',
    ),
]

# 死值验形状：不起进程，只看 argv。
SHAPE = '''fn main() {
    let dead = TmuxServer::from_handoff(Selector::SocketName("k-r8-dead-value".into()));
    let a = hook_set_args(&dead, "session-closed", Path::new("/opt/ccm/daemon"), 42, 999);
    println!("{a:?}");
    assert_eq!(&a[0], "-L");
    assert_eq!(&a[1], "k-r8-dead-value");
    let hook_at = a.iter().position(|x| x == "set-hook").expect("有 set-hook");
    assert!(hook_at >= 2, "选择器必须排在子命令之前（tmux argv 语法要求）");

    let dead2 = TmuxServer::from_handoff(Selector::SocketPath("/run/ccm/k-r8.sock".into()));
    let b = hook_set_args(&dead2, "session-created", Path::new("/opt/d"), 7, 8);
    println!("{b:?}");
    assert_eq!(&b[0], "-S");
    assert_eq!(&b[1], "/run/ccm/k-r8.sock");
    println!("SHAPE-OK");
}'''


def rustc_try(src: str, workdir: Path, name: str):
    f = workdir / f"{name}.rs"
    f.write_text(PRELUDE + "\n" + src, encoding="utf-8")
    out = workdir / name
    p = subprocess.run(
        ["rustc", "--edition", "2021", "-o", str(out), str(f)],
        capture_output=True, text=True, timeout=180,
    )
    err_code = None
    for line in p.stderr.splitlines():
        if line.startswith("error[") and "]" in line:
            err_code = line[len("error["):line.index("]")]
            break
    return p.returncode == 0, err_code, out, p.stderr


def main():
    if shutil.which("rustc") is None:
        print("SKIP：这台机器上没有 rustc ⇒ 本尺子不出数（**不当成绿**）")
        return 2

    tmp = Path(tempfile.mkdtemp(prefix="k-r8-shape-"))
    failures = []
    try:
        print("=" * 74)
        print("K-R8 D3 · 结构性形状探针（独立 .rs，不碰仓、不碰 tmux）")
        print("=" * 74)

        print("\n── ① 死值验形状 ──")
        ok, code, exe, err = rustc_try(SHAPE, tmp, "shape")
        if not ok:
            failures.append(f"死值那一格编译不过：{err[:400]}")
            print("  ✘ 编译失败")
        else:
            r = subprocess.run([str(exe)], capture_output=True, text=True, timeout=60)
            print("  " + r.stdout.strip().replace("\n", "\n  "))
            if "SHAPE-OK" not in r.stdout:
                failures.append("死值形状断言没过")
            else:
                print("  ✔ 选择器进了 argv，且排在 set-hook 之前")

        print("\n── ② K-R7 那三刀 + 09-01 那一刀，逐刀实打 ──")
        for name, want_compile, want_code, src in PROBES:
            ok, code, exe, err = rustc_try(src, tmp, name.split("_")[0])
            verdict = "编译通过" if ok else f"编译失败[{code}]"
            good = (ok == want_compile) and (want_code is None or code == want_code)
            print(f"  {'✔' if good else '✘'} {name}: {verdict}"
                  f"（期望 {'通过' if want_compile else '失败' + (f'[{want_code}]' if want_code else '')}）")
            if not good:
                failures.append(f"{name}: 实得 {verdict}")
            if ok and want_compile:
                r = subprocess.run([str(exe)], capture_output=True, text=True, timeout=60)
                print(f"      真跑 ⇒ {r.stdout.strip()}")
                if "NO-OP" not in r.stdout:
                    failures.append("N4 不是 no-op —— 它真装了 hook？")

        print("\n" + "=" * 74)
        if failures:
            print("✘ 本尺子红：")
            for f_ in failures:
                print(f"   - {f_}")
            return 1
        print("✔ N1/N2/N3 撞编译器；N4 编译得过但是个 no-op（装 hook 跟着一起没了）。")
        print("  ⇒ 「没交代 ⇒ 装不成」由**编译器**把着，不由扫源码把着。")
        print("  ⚠ 边界：另写一句裸 Command::new(\"tmux\") 仍绕得过 —— 那格只有")
        print("     「防手滑」，K-R7 已裁。本设计换的是**默认值**，不是买断。")
        print("=" * 74)
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
