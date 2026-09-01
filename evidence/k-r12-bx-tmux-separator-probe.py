#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R12 · Bx 摸底量具：`query_tmux_server()` 的 TAB 分隔符在 tmux 上被换成 `_` 的成因测定。

住址（唯一）：<树根>/evidence/k-r12-bx-tmux-separator-probe.py
被测对象：**本量具所在的那棵树**（树根现算 `git rev-parse --show-toplevel`，不写死）。

🔴 只许在容器里跑。启动时硬性自检：
   ① `/.dockerenv` 在（否则拒跑）；
   ② 所有 tmux 调用都带 `TMUX_TMPDIR=<私有临时目录>`，并断言 tmux 自报的 socket_path
      落在那个私有前缀下 —— 对不上立刻 abort，绝不碰 `/tmp/tmux-1000`。

量法：一律量**真实输出的字节**（`od -c` + python bytes repr），不量源码。
用法：python3 evidence/k-r12-bx-tmux-separator-probe.py
"""

import hashlib
import os
import re
import shutil
import subprocess
import sys

SELF = os.path.abspath(__file__)
SELF_DIR = os.path.dirname(SELF)
SUBJECT_REL = "remote-daemon-proto/src/observe/watcher.rs"

# 私有 socket 目录：只属于本次运行，容器内路径。
PRIV = "/tmp/k-r12-bx-tmuxprobe-%d" % os.getpid()


def run(argv, env=None, cwd=None, input_bytes=None):
    return subprocess.run(
        argv, capture_output=True, env=env, cwd=cwd, input=input_bytes
    )


def od_c(data: bytes) -> str:
    p = subprocess.run(["od", "-c"], input=data, capture_output=True)
    return p.stdout.decode("utf-8", "replace").rstrip("\n")


def show(title, data: bytes, extra=""):
    print("  ── %s%s" % (title, (" " + extra) if extra else ""))
    print("     bytes = %r" % data)
    for ln in od_c(data).splitlines():
        print("     %s" % ln)


def preamble():
    root = run(["git", "-C", SELF_DIR, "rev-parse", "--show-toplevel"])
    if root.returncode != 0:
        print("!! 拿不到树根：%s" % root.stderr.decode("utf-8", "replace"))
        sys.exit(3)
    root = root.stdout.decode().strip()
    head = run(["git", "-C", root, "rev-parse", "HEAD"]).stdout.decode().strip()
    subj = os.path.join(root, SUBJECT_REL)
    with open(subj, "rb") as f:
        blob = f.read()
    md5 = hashlib.md5(blob).hexdigest()
    st = run(["git", "-C", root, "status", "--porcelain"]).stdout.decode(
        "utf-8", "replace"
    )
    dirty = len([x for x in st.splitlines() if x.strip()])
    print("=" * 78)
    print("K-R12 · Bx 量具自绑定读数")
    print("  量具住址      : %s" % SELF)
    print("  树根          : %s" % root)
    print("  HEAD          : %s" % head)
    print("  被测文件      : %s" % SUBJECT_REL)
    print("  被测文件 md5  : %s  (%d 字节)" % (md5, len(blob)))
    print("  未提交改动处数: %d" % dirty)
    if dirty:
        for x in st.splitlines():
            print("      %s" % x)
    print("=" * 78)
    return root, subj, blob


def guard_container():
    print("\n### 安全自检（红线：绝不碰宿主 /tmp/tmux-1000）")
    incontainer = os.path.exists("/.dockerenv")
    print("  /.dockerenv 存在 : %s" % incontainer)
    print("  /tmp/tmux-1000 在容器里可见 : %s" % os.path.exists("/tmp/tmux-1000"))
    print("  本次私有 socket 目录 : %s" % PRIV)
    if not incontainer:
        print("!! 不在容器里 —— 拒跑（本量具会起 tmux server）")
        sys.exit(4)
    os.makedirs(PRIV, exist_ok=True)


def env_readout():
    print("\n### E0 环境读数（容器内现打）")
    for cmd in (["tmux", "-V"], ["uname", "-a"], ["id"]):
        p = run(cmd)
        print("  $ %s -> rc=%d  %s" % (" ".join(cmd), p.returncode,
                                       p.stdout.decode().strip()))
    p = run(["locale"])
    print("  $ locale ->")
    for ln in p.stdout.decode().splitlines():
        print("      %s" % ln)
    for k in ("LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMUX_TMPDIR", "HOME"):
        print("  env %-12s = %r" % (k, os.environ.get(k)))
    p = run(["sh", "-c", "locale -a"])
    print("  locale -a : %s" % " ".join(p.stdout.decode().split()))
    # 🔴 量具自身的污染，必须显式报出来：CPython 在 C/POSIX locale 下会做
    #    PEP 538 强制转码（把 `LC_CTYPE=C.UTF-8` 塞进**自己的** os.environ）+ PEP 540
    #    UTF-8 模式 ⇒ **它 spawn 的每个子进程都继承 `LC_CTYPE=C.UTF-8`**，
    #    而那正是本件要测的那个变量。本量具所有 tmux 调用都显式 pop 掉它（`UTF8_OFF`）。
    print("  ⚠ 量具自污染读数：sys.flags.utf8_mode=%d  os.environ['LC_CTYPE']=%r"
          % (sys.flags.utf8_mode, os.environ.get("LC_CTYPE")))
    p = run(["env"])
    inherited = [x for x in p.stdout.decode().split()
                 if x.startswith(("LANG", "LC_"))]
    print("  ⚠ 未处理时子进程继承到的 locale 变量 = %r" % inherited)


def extract_literal(subj_path):
    """从被测文件里**原样**抠出 query_tmux_server 那条 Rust 字面量的源码文本。

    只抠文本，不解释它 —— 解释交给 rustc（见 E2）。
    """
    with open(subj_path, "r", encoding="utf-8") as f:
        src = f.read()
    # ⚠ 刻意**不用**带转义类的正则去啃 Rust 字面量 —— 第一版就是在 `\\.` 那一段
    #   把「一个反斜杠 + 任意字符」写成了「两个反斜杠 + 任意字符」，静默匹配不到。
    #   改成：定位 fn 之后的第一行 `let script = "...";`，切首个 `"` 到行尾 `";`。
    i = src.find("fn query_tmux_server()")
    if i < 0:
        print("!! 树里没有 fn query_tmux_server() —— 量具与树不匹配，停")
        sys.exit(5)
    j = src.find("let script = ", i)
    k = src.find('";', j)
    if j < 0 or k < 0:
        print("!! 抠不到 query_tmux_server 的 script 字面量 —— 量具与树不匹配，停")
        sys.exit(5)
    lit = src[src.find('"', j):k + 1]
    print("\n### E1 从被测文件抠出的 Rust 字面量（**源码文本原样**，未解释）")
    print("     源码文本 = %s" % lit)
    show("源码文本的字节", lit.encode("utf-8"))
    return lit


RUST_REPRO = r'''
use std::process::Command;
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect::<Vec<_>>().join(" ")
}
fn main() {
    let script = __LITERAL__;
    println!("SCRIPT_LEN {}", script.len());
    println!("SCRIPT_HEX {}", hex(script.as_bytes()));
    println!("SCRIPT_HAS_0x09 {}", script.as_bytes().contains(&0x09u8));
    let out = Command::new("sh").arg("-c").arg(script).output().expect("spawn");
    println!("RC {:?}", out.status.code());
    println!("STDOUT_HEX {}", hex(&out.stdout));
    // 与生产逐字同一段解析
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("");
    let mut it = line.split('\t');
    let pid = it.next().and_then(|x| x.trim().parse::<u32>().ok());
    let sock = it.next().map(str::trim).filter(|x| !x.is_empty());
    println!("PARSED pid={:?} sock={:?}", pid, sock);
}
'''


def build_repro(lit, workdir):
    """把抠出来的字面量塞进一个最小 Rust 程序，用 rustc 真编真跑。

    ⇒ 「Rust 把 \\t 编译成什么」这一问由**编译器的真实输出**回答，不由我读源码回答。
    """
    src = RUST_REPRO.replace("__LITERAL__", lit)
    p = os.path.join(workdir, "kr12_repro.rs")
    with open(p, "w", encoding="utf-8") as f:
        f.write(src)
    exe = os.path.join(workdir, "kr12_repro")
    rc = run(["rustc", "-O", "-o", exe, p])
    if rc.returncode != 0:
        print("!! rustc 失败: %s" % rc.stderr.decode("utf-8", "replace")[:2000])
        sys.exit(6)
    return exe


def tmux_env(base=None, **over):
    e = dict(os.environ if base is None else base)
    e["TMUX_TMPDIR"] = PRIV
    e.pop("TMUX", None)
    for k, v in over.items():
        if v is None:
            e.pop(k, None)
        else:
            e[k] = v
    return e


def start_private_server():
    print("\n### E2 起一台**私有** tmux server（容器内）")
    e = tmux_env(LANG=None, LC_ALL=None, LC_CTYPE=None)
    p = run(["tmux", "new-session", "-d", "-s", "kr12probe", "sleep", "3600"], env=e)
    print("  $ TMUX_TMPDIR=%s tmux new-session -d -s kr12probe -> rc=%d %s"
          % (PRIV, p.returncode, p.stderr.decode().strip()))
    # 让 tmux 自报 socket_path，并**断言**它落在私有前缀下
    q = run(["tmux", "display-message", "-p", "#{socket_path}"], env=e)
    sock = q.stdout.decode().strip()
    print("  tmux 自报 socket_path = %r" % sock)
    if not sock.startswith(PRIV):
        print("!! socket 不在私有前缀 %s 下 —— 立即 abort，不许继续" % PRIV)
        run(["tmux", "kill-server"], env=e)
        sys.exit(7)
    print("  ✔ socket 落在私有前缀下，继续")
    return e, sock


def kill_private_server(e):
    run(["tmux", "kill-server"], env=e)
    shutil.rmtree(PRIV, ignore_errors=True)
    print("\n  已 kill-server 并删掉 %s" % PRIV)


UTF8_ON = dict(LANG="C.UTF-8", LC_ALL="C.UTF-8", LC_CTYPE="C.UTF-8")
UTF8_OFF = dict(LANG=None, LC_ALL=None, LC_CTYPE=None)


def run_repro(exe, label, over):
    e = tmux_env(**over)
    p = run([exe], env=e)
    print("\n  ── 生产字面量复刻 · %s" % label)
    print("     环境: LANG=%r LC_ALL=%r LC_CTYPE=%r TMUX_TMPDIR=%r"
          % (e.get("LANG"), e.get("LC_ALL"), e.get("LC_CTYPE"), e.get("TMUX_TMPDIR")))
    for ln in p.stdout.decode().splitlines():
        print("     %s" % ln)
    if p.stderr.strip():
        print("     stderr: %s" % p.stderr.decode()[:400])


def fmt_probe(label, fmt_bytes, over, argv_extra=()):
    """跑一次 tmux display-message -p <fmt>，量真实 stdout 字节。"""
    e = tmux_env(**over)
    argv = ["tmux"] + list(argv_extra) + ["display-message", "-p"]
    p = subprocess.run(argv + [fmt_bytes.decode("latin-1")],
                       capture_output=True, env=e)
    show(label, p.stdout, "(rc=%d)" % p.returncode)
    return p.stdout


def main():
    root, subj, _ = preamble()
    guard_container()
    env_readout()
    lit = extract_literal(subj)

    work = os.path.join(PRIV, "build")
    os.makedirs(work, exist_ok=True)
    exe = build_repro(lit, work)

    e_base, sock = start_private_server()
    try:
        print("\n### E3 生产字面量端到端复刻（Rust 编译 → sh → tmux → 字节 → 生产同款解析）")
        run_repro(exe, "POSIX locale（= 沙箱镜像默认）", UTF8_OFF)
        run_repro(exe, "C.UTF-8 locale（唯一变量：locale）", UTF8_ON)

        print("\n### E4 分隔符逐字符扫描（同一台 server、同一条命令，只换分隔符/locale）")
        cases = [
            ("TAB 0x09", b"\x09"),
            ("SOH 0x01", b"\x01"),
            ("US  0x1f", b"\x1f"),
            ("SP  0x20", b"\x20"),
            ("竖线 0x7c", b"\x7c"),
            ("逗号 0x2c", b"\x2c"),
            ("DEL 0x7f", b"\x7f"),
            ("反斜杠+t（两个可打印字符）", b"\\t"),
            ("LF  0x0a", b"\x0a"),
        ]
        for lbl, sep in cases:
            fmt = b"#{pid}" + sep + b"#{socket_path}"
            for loc_lbl, over in (("POSIX", UTF8_OFF), ("C.UTF-8", UTF8_ON)):
                fmt_probe("E4 %-28s locale=%-8s" % (lbl, loc_lbl), fmt, over)

        print("\n### E5 `tmux -u`（强制把客户端标成 UTF-8，locale 仍是 POSIX）")
        fmt_probe("E5 TAB + tmux -u  locale=POSIX", b"#{pid}\x09#{socket_path}",
                  UTF8_OFF, argv_extra=("-u",))

        print("\n### E6 改法 B：一字段一次调用（两个 subprocess）")
        for loc_lbl, over in (("POSIX", UTF8_OFF), ("C.UTF-8", UTF8_ON)):
            for f in (b"#{pid}", b"#{socket_path}"):
                fmt_probe("E6 %-14s locale=%-8s" % (f.decode(), loc_lbl), f, over)

        print("\n### E7 血溅面：同一条 sanitize 会不会也打到 `tmux ls -F '<TMUX_LS_FMT>'`")
        with open(subj, "r", encoding="utf-8") as f:
            src = f.read()
        m = re.search(r'const TMUX_LS_FMT: &str = "(.*?)";', src)
        raw_fmt = m.group(1) if m else None
        print("  从被测文件抠到的 TMUX_LS_FMT 源码文本 = %r" % raw_fmt)
        ls_fmt = raw_fmt.replace("\\t", "\t") if raw_fmt else None
        if ls_fmt:
            for loc_lbl, over in (("POSIX", UTF8_OFF), ("C.UTF-8", UTF8_ON)):
                e = tmux_env(**over)
                p = run(["tmux", "ls", "-F", ls_fmt], env=e)
                show("E7 tmux ls -F TMUX_LS_FMT  locale=%s" % loc_lbl,
                     p.stdout, "(rc=%d)" % p.returncode)
                # 生产 session_names 的等价解析
                first = p.stdout.decode("utf-8", "replace").splitlines()
                first = first[0] if first else ""
                print("     生产 session_names 等价解析（split('\\t') 取第一段）= %r"
                      % first.split("\t")[0].strip())

        print("\n### E8 显式 `-S` 形式（红线要求的写法，同一台私有 server）")
        e = tmux_env(**UTF8_OFF)
        p = run(["tmux", "-S", sock, "display-message", "-p",
                 "#{pid}\t#{socket_path}"], env=e)
        show("E8 tmux -S %s display-message" % sock, p.stdout,
             "(rc=%d)" % p.returncode)
    finally:
        kill_private_server(e_base)

    print("\n完。所有 tmux 调用的 socket 前缀 = %s（容器内私有）" % PRIV)


if __name__ == "__main__":
    main()
