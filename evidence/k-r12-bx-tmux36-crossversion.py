#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R12 · Bx 量具之二：把**宿主那一份 tmux 3.6** 拿到**容器里**跑，回答「换个版本还犯不犯」。

住址（唯一）：<树根>/evidence/k-r12-bx-tmux36-crossversion.py
被测对象：宿主 `/usr/bin/tmux`（3.6a）与沙箱镜像里的 `/usr/bin/tmux`（3.4），两版对拍。

🔴 红线：**绝不在宿主上起 tmux server、绝不碰 `/tmp/tmux-1000`。**
   做法：`stage` 一步只在宿主上**读**（`ldd` 解闭包 + `cp` 到一个暂存目录），
   `probe` 一步在**容器里**用 `ld.so --library-path <暂存>` 跑那个二进制，
   socket 走容器内私有 `TMUX_TMPDIR`，并断言 socket 落在私有前缀下，否则 abort。
   ⇒ 宿主上从头到尾只发生「读文件」与「跑 `tmux -V` / `ldd`」，没有 server。

用法：
   python3 evidence/k-r12-bx-tmux36-crossversion.py stage <暂存目录>     # 宿主上跑
   python3 evidence/k-r12-bx-tmux36-crossversion.py probe <暂存目录>     # 容器里跑
"""

import hashlib
import os
import shutil
import subprocess
import sys

SELF = os.path.abspath(__file__)
SELF_DIR = os.path.dirname(SELF)
SUBJECT_REL = "remote-daemon-proto/src/observe/watcher.rs"
PRIV = "/tmp/k-r12-bx-tmux36-%d" % os.getpid()


def run(argv, env=None, input_bytes=None):
    return subprocess.run(argv, capture_output=True, env=env, input=input_bytes)


def preamble(tag):
    root = run(["git", "-C", SELF_DIR, "rev-parse", "--show-toplevel"])
    root = root.stdout.decode().strip()
    head = run(["git", "-C", root, "rev-parse", "HEAD"]).stdout.decode().strip()
    subj = os.path.join(root, SUBJECT_REL)
    md5 = hashlib.md5(open(subj, "rb").read()).hexdigest()
    st = run(["git", "-C", root, "status", "--porcelain"]).stdout.decode("utf-8", "replace")
    dirty = len([x for x in st.splitlines() if x.strip()])
    print("=" * 78)
    print("K-R12 · Bx 量具之二（%s）" % tag)
    print("  量具住址      : %s" % SELF)
    print("  树根          : %s" % root)
    print("  HEAD          : %s" % head)
    print("  被测文件 md5  : %s  (%s)" % (md5, SUBJECT_REL))
    print("  未提交改动处数: %d" % dirty)
    print("=" * 78)


def ldd_closure(binary):
    """递归解 ldd 闭包。返回 {SONAME: 真实路径}。

    ⚠ 第一版只收了真实路径，落地成 `libtinfo.so.6.6` 这种**带小版本**的名字，
      而 `ld.so --library-path` 要找的是 **SONAME**（`libtinfo.so.6`）⇒ 找不到就
      **静默回退到容器自带的那一份**，实测报 `NCURSES6_TINFO_6.5... not found`。
      ⇒ 这里按 ldd 左边那一列（SONAME）落名，realpath 只用来读内容。
    """
    seen, todo, out = set(), [binary], {}
    while todo:
        b = todo.pop()
        if b in seen:
            continue
        seen.add(b)
        p = run(["ldd", b])
        for ln in p.stdout.decode("utf-8", "replace").splitlines():
            ln = ln.strip()
            soname, path = None, None
            if "=>" in ln:
                lhs, rhs = ln.split("=>", 1)
                soname = lhs.strip()
                rhs = rhs.strip()
                if rhs.startswith("/"):
                    path = rhs.split(" (")[0]
            elif ln.startswith("/"):
                path = ln.split(" (")[0]
                soname = os.path.basename(path)
            if path and soname and os.path.exists(path):
                out[soname] = path
                rp = os.path.realpath(path)
                if rp not in seen:
                    todo.append(rp)
    return out


def stage(dest):
    preamble("stage · 在宿主上只读")
    tmux = shutil.which("tmux")
    ver = run([tmux, "-V"]).stdout.decode().strip()
    print("  宿主 tmux 住址 : %s" % tmux)
    print("  宿主 tmux -V   : %s   (⚠ `-V` 不起 server)" % ver)
    print("  宿主 tmux md5  : %s" % hashlib.md5(open(tmux, "rb").read()).hexdigest())
    os.makedirs(os.path.join(dest, "lib"), exist_ok=True)
    shutil.copy2(tmux, os.path.join(dest, "tmux36"))
    files = ldd_closure(os.path.realpath(tmux))
    n = 0
    loader = None
    for soname, f in sorted(files.items()):
        shutil.copy2(f, os.path.join(dest, "lib", soname))
        n += 1
        if "ld-linux" in soname:
            loader = soname
    print("  SONAME 清单: %s" % " ".join(sorted(files)))
    # terminfo：tmux 客户端要它
    for ti in ("/usr/share/terminfo", "/lib/terminfo", "/etc/terminfo"):
        if os.path.isdir(ti):
            d = os.path.join(dest, "terminfo", os.path.basename(ti))
            if not os.path.exists(d):
                shutil.copytree(ti, d, symlinks=True)
    with open(os.path.join(dest, "LOADER"), "w") as f:
        f.write(loader or "")
    print("  已拷 %d 个 so 到 %s/lib，loader=%s" % (n, dest, loader))
    print("  ⚠ 宿主上本步只做了：ldd · cp · tmux -V —— **没有起过任何 tmux server**")


def od_c(data: bytes) -> str:
    return subprocess.run(["od", "-c"], input=data, capture_output=True) \
        .stdout.decode("utf-8", "replace").rstrip("\n")


def show(title, data, rc):
    print("  ── %s (rc=%d)" % (title, rc))
    print("     bytes = %r" % data)
    for ln in od_c(data).splitlines():
        print("     %s" % ln)


def probe(stage_dir):
    preamble("probe · 在容器里")
    if not os.path.exists("/.dockerenv"):
        print("!! 不在容器里 —— 拒跑"); sys.exit(4)
    os.makedirs(PRIV, exist_ok=True)
    loader = open(os.path.join(stage_dir, "LOADER")).read().strip()
    ldso = os.path.join(stage_dir, "lib", loader)
    libp = os.path.join(stage_dir, "lib")
    tmux36 = os.path.join(stage_dir, "tmux36")
    ti = os.path.join(stage_dir, "terminfo", "terminfo")

    def t36(args, over):
        e = dict(os.environ)
        e["TMUX_TMPDIR"] = PRIV
        e.pop("TMUX", None)
        if os.path.isdir(ti):
            e["TERMINFO_DIRS"] = ti
        for k, v in over.items():
            e.pop(k, None) if v is None else e.update({k: v})
        return run([ldso, "--library-path", libp, tmux36] + args, env=e)

    def t34(args, over):
        e = dict(os.environ)
        e["TMUX_TMPDIR"] = PRIV
        e.pop("TMUX", None)
        for k, v in over.items():
            e.pop(k, None) if v is None else e.update({k: v})
        return run(["tmux"] + args, env=e)

    OFF = dict(LANG=None, LC_ALL=None, LC_CTYPE=None)
    ON = dict(LANG="C.UTF-8", LC_ALL="C.UTF-8", LC_CTYPE="C.UTF-8")

    print("\n### V0 两个二进制各自自报版本（都在容器里跑）")
    print("  容器自带 tmux -V : %s" % t34(["-V"], OFF).stdout.decode().strip())
    p = t36(["-V"], OFF)
    print("  宿主搬来的 tmux -V : %s   (rc=%d) %s"
          % (p.stdout.decode().strip(), p.returncode,
             p.stderr.decode().strip()[:200]))
    if p.returncode != 0:
        print("!! 宿主 tmux 在容器里跑不起来 —— 本量具判不了 3.6，如实报"); sys.exit(0)

    for label, tm in (("tmux 3.4（镜像自带）", t34), ("tmux 3.6a（宿主搬来）", t36)):
        print("\n### V1 %s" % label)
        r = tm(["-f", "/dev/null", "new-session", "-d", "-s", "kr12x", "sleep", "600"], OFF)
        print("  new-session rc=%d %s" % (r.returncode, r.stderr.decode().strip()[:200]))
        if r.returncode != 0:
            print("  !! 起不来，这一版判不了"); continue
        sk = tm(["display-message", "-p", "#{socket_path}"], OFF).stdout.decode().strip()
        print("  socket_path = %r" % sk)
        if not sk.startswith(PRIV):
            print("!! socket 不在私有前缀 %s 下 —— abort" % PRIV)
            tm(["kill-server"], OFF); sys.exit(7)
        for loc, over in (("POSIX", OFF), ("C.UTF-8", ON)):
            r = tm(["display-message", "-p", "#{pid}\t#{socket_path}"], over)
            show("%s  locale=%-8s  TAB 分隔" % (label, loc), r.stdout, r.returncode)
        r = tm(["-u", "display-message", "-p", "#{pid}\t#{socket_path}"], OFF)
        show("%s  locale=POSIX + `-u`" % label, r.stdout, r.returncode)
        r = tm(["display-message", "-p", "-h"], OFF)
        print("  display-message 用法行: %s | %s"
              % (r.stdout.decode().strip()[:200], r.stderr.decode().strip()[:200]))
        tm(["kill-server"], OFF)
    shutil.rmtree(PRIV, ignore_errors=True)
    print("\n完。私有 socket 前缀 = %s（容器内）" % PRIV)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__); sys.exit(2)
    if sys.argv[1] == "stage":
        stage(sys.argv[2])
    elif sys.argv[1] == "probe":
        probe(sys.argv[2])
    else:
        print(__doc__); sys.exit(2)
