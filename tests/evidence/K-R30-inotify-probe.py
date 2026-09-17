#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R30 · KR30D2「乙 事件驱动地等」那一格的探针：**有没有一个可等的事件？**

`KR30D2` 逐字问：「那个模块的既有形状是『读到 EOF』，本格有没有对应的可等事件？**没有就说没有**」。
本探针不去猜，去打一发。

# 被测的那条假设

`ETXTBSY` 要等的是「那个 struct file 的**最后一个**引用被丢掉」。而内核在 `__fput` 那一刻
会发一条 `IN_CLOSE_WRITE`（inotify）—— **不是**在我们自己 `close()` 那一刻：
我们 `close()` 时孩子还攥着一个引用 ⇒ 引用计数没到 0 ⇒ 不 `__fput` ⇒ **不发**。
⇒ 如果这条假设成立，那么 `IN_CLOSE_WRITE` 精确地就是「窗口关上了」这个事件。

# 台架

    inofd = inotify_init1(); inotify_add_watch(inofd, 目标, IN_CLOSE_WRITE)
    fd = open(目标, O_WRONLY)
    fork 一个持有者（攥着继承来的写 fd，hold 之后 execve）
    close(fd)                      # 我们自己那把
    t0 = now()
    read(inofd)                    # ★ 阻塞在事件上 —— 事件驱动，**一个定时器都没有**
    t_evt = now()
    posix_spawn(目标)               # 事件到了之后**只试一次**

# 两臂（本探针的非空对照）

  · hold=0      ：窗口 ≈ 持有者的 fork→execve
  · hold=20ms   ：窗口 ≈ 20 ms
两臂的 `t_evt - t0` 必须**明显不同**（后者 ≈ 20 ms）—— 相同 ⇒ 这条事件跟的不是那个窗口，
探针作废。⚠ 这一格就是本探针的「一格不一样」：没有它，一把只会说「事件到了」的尺子
与一把真的在跟窗口的尺子，输出上长得一模一样。

# 用法

    python3 evidence/K-R30-inotify-probe.py [--n 100]

⚠ **本探针不改任何生产代码**，也不主张走这条路 —— `KR30D2` 明令「不许自批选路」。
它只把「有没有这个事件」从「我觉得」变成一个读数，代价那一栏归件文件 `§8`。
"""

from __future__ import annotations

import argparse
import ctypes
import os
import platform
import select
import shutil
import struct
import sys
import tempfile
import time

SPACER = "/bin/true"
IN_CLOSE_WRITE = 0x00000008
EVENT_HDR = struct.Struct("iIII")  # wd, mask, cookie, len


def die(msg: str) -> None:
    print(f"\n台架自检：FAIL —— {msg}")
    sys.stdout.flush()
    raise SystemExit(2)


def pct(xs: list[float], q: float) -> float:
    if not xs:
        return float("nan")
    s = sorted(xs)
    i = min(len(s) - 1, max(0, int(round(q * (len(s) - 1)))))
    return s[i]


def fmt_us(x: float) -> str:
    return "nan" if x != x else f"{x * 1e6:.0f}"


def spin(seconds: float) -> None:
    if seconds <= 0:
        return
    end = time.perf_counter() + seconds
    while time.perf_counter() < end:
        pass


class Inotify:
    def __init__(self) -> None:
        self.libc = ctypes.CDLL("libc.so.6", use_errno=True)
        self.libc.inotify_init1.restype = ctypes.c_int
        self.libc.inotify_init1.argtypes = [ctypes.c_int]
        self.libc.inotify_add_watch.restype = ctypes.c_int
        self.libc.inotify_add_watch.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32]

    def init(self) -> int:
        fd = self.libc.inotify_init1(0)
        if fd < 0:
            die(f"inotify_init1 失败：errno={ctypes.get_errno()}")
        return fd

    def add(self, fd: int, path: str, mask: int) -> int:
        wd = self.libc.inotify_add_watch(fd, path.encode(), mask)
        if wd < 0:
            die(f"inotify_add_watch 失败：errno={ctypes.get_errno()}")
        return wd


def one_sample(ino: Inotify, target: str, hold_s: float) -> tuple[float, int, int]:
    """返回 `(等到事件花了多久, 收到几条事件, 事件之后 exec 试了几次)`。"""
    inofd = ino.init()
    ino.add(inofd, target, IN_CLOSE_WRITE)
    fd = os.open(target, os.O_WRONLY)
    holder = os.fork()
    if holder == 0:
        try:
            spin(hold_s)
            os.execv(SPACER, [SPACER])
        except BaseException:
            pass
        os._exit(127)
    os.close(fd)
    t0 = time.perf_counter()
    # ★ 阻塞在事件上。`select` 那个 5 秒只是台架的保险丝，不是等待策略。
    ready, _, _ = select.select([inofd], [], [], 5.0)
    dt = time.perf_counter() - t0
    if not ready:
        os.close(inofd)
        os.waitpid(holder, 0)
        return float("nan"), 0, -1
    buf = os.read(inofd, 4096)
    n_evt = 0
    off = 0
    while off + EVENT_HDR.size <= len(buf):
        _wd, _mask, _cookie, ln = EVENT_HDR.unpack_from(buf, off)
        off += EVENT_HDR.size + ln
        n_evt += 1
    # 事件到了之后**只试一次**：如果这条事件真的等对了，这一次就该成。
    tries = 0
    while tries < 64:
        tries += 1
        try:
            child = os.posix_spawn(target, [target], {})
        except OSError:
            continue
        os.waitpid(child, 0)
        break
    os.close(inofd)
    os.waitpid(holder, 0)
    return dt, n_evt, tries


def run(ino: Inotify, target: str, n: int, hold_s: float) -> dict:
    dts: list[float] = []
    evts: list[int] = []
    tries: list[int] = []
    for _ in range(n):
        d, e, t = one_sample(ino, target, hold_s)
        dts.append(d)
        evts.append(e)
        tries.append(t)
    return {"dt": dts, "evt": evts, "tries": tries}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=100)
    args = ap.parse_args()

    if not os.path.exists(SPACER):
        die(f"{SPACER} 不在")
    workdir = tempfile.mkdtemp(prefix="kr30i-")
    target = os.path.join(workdir, "probe-bin")  # 中性名
    shutil.copy2(SPACER, target)
    os.chmod(target, 0o755)
    ino = Inotify()

    print("=" * 78)
    print("K-R30 · KR30D2 乙 —— 「有没有一个可等的事件」探针")
    print("=" * 78)
    print(f"· 量于   ：{time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"· 内核   ：{platform.platform()}")
    print(f"· python ：{sys.version.split()[0]} · nproc {os.cpu_count()}")
    print(f"· 分母   ：每臂 {args.n} 个样本；等待用 `select` 阻塞在 inotify fd 上，**没有定时器**")
    print(f"· 掩码   ：IN_CLOSE_WRITE (0x{IN_CLOSE_WRITE:x})")
    print()

    arms = {"hold=0（窗口 ≈ 持有者的 fork→execve）": 0.0, "hold=20ms（长窗口对照）": 0.020}
    out = {}
    print("| 臂 | n | 等到事件的 µs p50/p99/max | 事件条数 min/max | 事件之后 exec 的 tries min/p50/max | 超时(5s)的样本 |")
    print("|---|---|---|---|---|---|")
    for name, hold in arms.items():
        r = run(ino, target, args.n, hold)
        good = [x for x in r["dt"] if x == x]
        to = len(r["dt"]) - len(good)
        tr = [x for x in r["tries"] if x > 0]
        out[name] = r
        print(
            f"| {name} | {len(r['dt'])} | {fmt_us(pct(good, 0.5))}/{fmt_us(pct(good, 0.99))}/"
            f"{fmt_us(max(good)) if good else 'nan'} | {min(r['evt'])}/{max(r['evt'])} | "
            f"{min(tr) if tr else '-'}/{pct([float(x) for x in tr], 0.5):.0f}/{max(tr) if tr else '-'} | {to} |"
        )
    print()

    short = [x for x in out["hold=0（窗口 ≈ 持有者的 fork→execve）"]["dt"] if x == x]
    long_ = [x for x in out["hold=20ms（长窗口对照）"]["dt"] if x == x]
    s50, l50 = pct(short, 0.5), pct(long_, 0.5)
    print("── 判定 ──────────────────────────────────────────────────────────")
    print(f"· 两臂的 p50 分别是 {fmt_us(s50)} µs 与 {fmt_us(l50)} µs。")
    if l50 > s50 * 3:
        print("· ⇒ **这条事件真的在跟那个窗口**（长窗口臂等得明显更久）⇒ 可等的事件**存在**。")
    else:
        print("· ⇒ 🔴 两臂等得一样久 ⇒ 这条事件跟的不是那个窗口，本探针作废。")
    all_tries = [x for arm in out.values() for x in arm["tries"] if x > 0]
    print(f"· 事件到了之后 exec 的 tries：全体 {len(all_tries)} 个样本里最大 {max(all_tries)}"
          f"（= 1 意味着**等对了就一次成**）。")
    print()
    print("── 诚实边界 ──────────────────────────────────────────────────────")
    print("· 本探针只证明「**这个事件存在、而且对得上那个窗口**」。它**不**主张本仓该走这条路 ——")
    print("  代价（平台 cfg / 新依赖 / 谁来建这个 watch / Windows 上根本没有 ETXTBSY）归件文件 §8，")
    print("  而选路是 PM / 用户的板（`KR30D2` 逐字：不许自批选路）。")
    print("· 沙箱读数：容器内、这个内核、这个文件系统。真机未必同值。")

    shutil.rmtree(workdir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
