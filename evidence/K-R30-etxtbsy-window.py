#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R30 · KR30D1 的量具：**真造** `ETXTBSY` 竞态，量「从写 fd 关掉到 exec 成功」那个窗口。

被测对象**不是**本仓任何一行 Rust。被测对象是两样东西：
  ① 内核那条规则本身（`execve` 的目标此刻还被某个 struct file 打开着写 ⇒ `ETXTBSY`）；
  ② 「**按次数封顶、不按时间**」这把尺子（`SPAWN_ETXTBSY_TRIES` 次立即重试）够不够盖住 ①。

# 造法

每一个样本的骨架，逐字对着 `local_backend.rs` 那一段头注写的成因：

    fd = open(目标, O_WRONLY)      # 我们自己那把写 fd（Python 与 Rust std 一样默认 CLOEXEC）
    <某处 fork>                    # 别人（另一个线程 / 另一处 spawn）在这一刻 fork
                                   # 那个孩子攥着继承来的写 fd —— CLOEXEC 要到它 execve 那一刻才生效
    close(fd)                      # 我们自己那把立刻就关（`extract_embedded_to` 的形状）
    t0 = now()                     # ★ 计时从这里开始 —— 这就是 D1 要的「从写 fd 关掉」
    loop { posix_spawn(目标) }      # 立即重试，**两次之间什么都不做**，直到成功
                                   # ★ 停表 = 第一次 exec 成功 —— 这就是 D1 要的「到 exec 成功」

⚠ 内核那一侧的机制（为什么「我自己关了」还不够）：`i_writecount` 是**按一次 open 记的**，
  fork 出去的孩子拿到的是**同一个 struct file 的又一个引用** ⇒ 只有**最后一个**引用被丢掉
  （`__fput` → `put_write_access`）那一刻，`execve` 才不再 `ETXTBSY`。

# 四臂

  · 臂 A「写 fd 立刻就关」= **一个持有者都不 fork**，窗口恒等于 0。
    这一臂就是 `KR30D1` 点名的那个**假窗口**（acceptor 失效口）：它的答案恒是「够」。
  · 臂 B「自然泄漏（Python 子进程）」= fork 一个持有者、hold=0，子进程立刻 `execv`。
    ⚠ 它量到的是 **Python 解释器**跑完 fork→execv 那几行的间隔 ⇒ 真机（Rust/C 子进程）那个的**上界**。
  · 臂 B2「自然泄漏（C 级 · posix_spawn）」= 另一个**线程**调 `os.posix_spawn`，
    fork→execve 全在 glibc 的 C 路径里 ⇒ 与生产段（Rust `Command::spawn` 在另一个线程里）
    形状最像的一臂。
    🔴 **它不是一格，是一条生存曲线**：那个线程的 clone 时刻控不住，只测「信号一到就关 fd」
    这一个点，读到的 0 命中**分不开**两件事 ——「窗口太短」与「我们抢在它 clone 之前就关了」。
    ⇒ 本臂**扫一条延迟**：收到「我马上要 spawn 了」之后再等 `offset` 才关自己那把 fd。
    `offset` 大到 clone 必然已经发生之后，命中率就只由「那个孩子 execve 了没有」决定
    ⇒ **命中率随 offset 的下降曲线 ≈ 那个窗口的生存函数**。
    ⚠ offset 从「线程宣告」起算，比从 clone 起算**偏大**（clone 在宣告之后几 µs）。
  · 臂 C「长窗口对照」= hold 取「明显长于 N 次立即重试打完」的值（由 §2 的实测 C_N **现算**，
    不是拍一个数）。这一臂必须答「**不够**」—— 与臂 A 同答 ⇒ 尺子瞎了，读数作废。
  · 扫描：hold 从 0 扫到 10 ms，给出「预算 N 盖得住多长的窗口」那条曲线与拐点。

# 口径（分母怎么数的）

  · 一个「样本」= 上面那一趟骨架跑完一次。`tries` = **exec 的尝试次数**（成功那次算在内）
    ⇒ `tries == 1` 意味着**一次 ETXTBSY 都没撞上**；`tries == k` 意味着撞了 `k-1` 次。
  · `dt` = 从 `close(fd)` 返回到**第一次 exec 成功**之间的 `perf_counter` 差（单调钟）。
  · 「在预算内」= `tries <= N`，`N` 是 `SPAWN_ETXTBSY_TRIES`，**现打自 Rust 源码**
    （本量具不复述那个数，见 `read_budget`）。
  · 每一格都给 min / p50 / p90 / p99 / max —— 尾部才是这件事的要害，平均数不算数。
  · 负载：两趟。`idle` = 不额外加载；`loaded` = 起 `nproc` 个纯自旋进程占满 CPU。
    每一趟都现打 `loadavg` 记在读数旁边。
  · ⚠ 一处**没消掉的混杂**：臂 B/C 与扫描的持有者用**自旋**撑住 hold（`sleep` 在亚毫秒上量不准）
    ⇒ 它占着一个核。`idle` 那一趟是 1/nproc 的占用，`loaded` 那一趟本来就满载 ⇒ 影响更小。
    §2 那一格**不吃这个混杂**：那里的持有者阻塞在管道上，不烧 CPU。

# 🔴 本量具自己的非空对照（本仓最贵的那族：「一把只会说『一样』的尺子」）

臂 A 的「撞上过 ETXTBSY 的样本数」在构造上恒为 0（那是**恒同格**）。
⇒ 本量具**每一次输出**里都同时打臂 C 的同一个计数（**非零格**），两格并排。
两格若一起是 0 ⇒ §1 的台架自检当场 FAIL 退出，而不是打一张全绿表。

# 用法

    python3 evidence/K-R30-etxtbsy-window.py [--n 300] [--long-n 100] [--sweep-n 200]
                                             [--cost-n 200] [--src <local_backend.rs>]
                                             [--mutate fake-long-window]

`--mutate fake-long-window` 是 `§3` 的 `R30M1` 那一刀：把长窗口对照臂的持有者也改成
「写 fd 立刻就关」⇒ 两臂同答「够」，用来实打「尺子瞎了长什么样」。
"""

from __future__ import annotations

import argparse
import errno
import os
import platform
import re
import shutil
import sys
import tempfile
import threading
import time

# ── 台架常量（都是量具自己的，不是被测对象的） ────────────────────────────────
SPACER = "/bin/true"  # 持有者最后 execve 的那个东西（与目标不同名，免得两件事缠在一起）
ATTEMPT_CAP = 200000  # 单个样本最多试几次（防跑飞；命中它 ⇒ 台架自检 FAIL）


def die(msg: str) -> None:
    print(f"\n台架自检：FAIL —— {msg}")
    sys.stdout.flush()
    raise SystemExit(2)


def read_budget(src_path: str) -> tuple[int, str]:
    """把预算**现打**自 Rust 源码，不在本量具里复述那个数（`brief` 13b：闭集只许一个住址）。"""
    with open(src_path, "r", encoding="utf-8") as fh:
        src = fh.read()
    hits = re.findall(r"pub const SPAWN_ETXTBSY_TRIES: u32 = (\d+);", src)
    if len(hits) != 1:
        die(f"在 {src_path} 里找 `SPAWN_ETXTBSY_TRIES` 的定义，命中 {len(hits)} 处（要恰好 1 处）")
    return int(hits[0]), src_path


def pct(xs: list[float], q: float) -> float:
    if not xs:
        return float("nan")
    s = sorted(xs)
    i = min(len(s) - 1, max(0, int(round(q * (len(s) - 1)))))
    return s[i]


def spin(seconds: float) -> None:
    """自旋等待。**不是** `sleep`：小于毫秒的 hold 用 `sleep` 量不准。"""
    if seconds <= 0:
        return
    end = time.perf_counter() + seconds
    while time.perf_counter() < end:
        pass


def make_target(workdir: str, name: str) -> str:
    """造一个真 ELF 目标：拷 `/bin/true`。**不用 shebang 脚本** —— 那条路的 ETXTBSY 语义另说。"""
    dst = os.path.join(workdir, name)
    shutil.copy2(SPACER, dst)
    os.chmod(dst, 0o755)
    with open(dst, "rb") as fh:
        magic = fh.read(4)
    if magic != b"\x7fELF":
        die(f"目标不是 ELF（magic={magic!r}）—— 台架的前提没建立")
    return dst


def open_write_fd(target: str) -> int:
    fd = os.open(target, os.O_WRONLY)
    if os.get_inheritable(fd):
        os.close(fd)
        die("写 fd 不是 CLOEXEC —— 台架的形状与 Rust std 对不上，读数不能算")
    return fd


def retry_until_exec_ok(target: str) -> tuple[int, float]:
    """立即重试到 exec 成功。返回 `(tries, dt)`；`dt` 的表从调用前一刻起算。"""
    t0 = time.perf_counter()
    tries = 0
    while tries < ATTEMPT_CAP:
        tries += 1
        try:
            child = os.posix_spawn(target, [target], {})
        except OSError as e:
            if e.errno != errno.ETXTBSY:
                die(f"exec 撞到的不是 ETXTBSY 而是 {e.errno}（{e.strerror}）—— 台架造错了")
            continue
        dt = time.perf_counter() - t0
        os.waitpid(child, 0)
        return tries, dt
    die(f"一个样本试满了 {ATTEMPT_CAP} 次还没成功 —— 台架跑飞了")
    return 0, 0.0  # 到不了


# ── 一个样本：臂 A / B / C（fork 一个持有者，hold 由参数给） ──────────────────
def one_sample(target: str, hold_s: float, with_holder: bool) -> tuple[int, float]:
    fd = open_write_fd(target)
    holder = -1
    if with_holder:
        holder = os.fork()
        if holder == 0:
            # ★ 子进程：手里攥着继承来的那个写 fd。CLOEXEC 要到 execve 那一刻才生效
            #   ⇒ 在它 execve 之前，那个 struct file 一直活着，i_writecount 就一直不为 0。
            try:
                spin(hold_s)
                os.execv(SPACER, [SPACER])
            except BaseException:
                pass
            os._exit(127)
    os.close(fd)
    tries, dt = retry_until_exec_ok(target)
    if holder > 0:
        os.waitpid(holder, 0)
    return tries, dt


# ── 一个样本：臂 B2（另一个**线程**调 posix_spawn —— 与生产段形状最像的那一臂） ──
def one_sample_thread_spawn(target: str, pre_close_s: float) -> tuple[int, float, float]:
    """返回 `(tries, dt, lag)`。

    `lag` = 主线程关掉自己那把 fd 的时刻 **减去** 那个线程的 `posix_spawn` 返回的时刻。
    🔴 它是本臂的**自检位**：`lag > 0` ⇒ 主线程是在孩子 execve **之后**才醒的
    ⇒ 这一臂在构造上就撞不到，读到的 0 命中**不是**「窗口太短」，是**判不了**。
    """
    fd = open_write_fd(target)
    about_to = threading.Event()
    stamp: list[float] = []

    def worker() -> None:
        about_to.set()
        pid = os.posix_spawn(SPACER, [SPACER], {})
        stamp.append(time.perf_counter())  # 孩子 execve 之后（vfork：父线程被挂起到那一刻）
        os.waitpid(pid, 0)

    th = threading.Thread(target=worker)
    th.start()
    about_to.wait()
    spin(pre_close_s)  # 让那个线程的 clone 先发生（offset=0 时读到的 0 命中分不开两件事）
    os.close(fd)
    t_close = time.perf_counter()
    tries, dt = retry_until_exec_ok(target)
    th.join()
    lag = (t_close - stamp[0]) if stamp else float("nan")
    return tries, dt, lag


def measure_spawn_wall(n: int) -> list[float]:
    """★ 本量具最要紧的一格：**一次 `posix_spawn` 调用的墙钟时长**。

    glibc 用 `clone(CLONE_VM|CLONE_VFORK)` ⇒ **调用线程一直被挂起到那个孩子 execve 为止**。
    ⇒ 这个时长是「孩子 fork→execve 窗口」的一个**上界**，而且**全在 C 里**，
    不吃 CPython `PyOS_AfterFork_Child` 那笔账（臂 B 吃的正是它）。

    ⚠ 它是上界不是等号：里面还含调用方进 clone 之前的准备与 `execve` 的入口那一段。
    """
    out: list[float] = []
    for _ in range(n):
        t0 = time.perf_counter()
        pid = os.posix_spawn(SPACER, [SPACER], {})
        out.append(time.perf_counter() - t0)
        os.waitpid(pid, 0)
    return out


def run_arm(target: str, n: int, hold_s: float, with_holder: bool) -> dict:
    tries_l: list[int] = []
    dt_l: list[float] = []
    for _ in range(n):
        t, d = one_sample(target, hold_s, with_holder)
        tries_l.append(t)
        dt_l.append(d)
    return {"tries": tries_l, "dt": dt_l}


def run_arm_thread_spawn(target: str, n: int, pre_close_s: float) -> dict:
    tries_l: list[int] = []
    dt_l: list[float] = []
    lag_l: list[float] = []
    for _ in range(n):
        t, d, lag = one_sample_thread_spawn(target, pre_close_s)
        tries_l.append(t)
        dt_l.append(d)
        lag_l.append(lag)
    return {"tries": tries_l, "dt": dt_l, "lag": lag_l}


def measure_budget_cost(target: str, n: int, budget: int) -> dict:
    """§2：**`budget` 次立即重试打完花多久** —— 这把尺子自己的长度。

    持有者**阻塞在一根管道上**（不烧 CPU），窗口由本函数在打完之后才关掉
    ⇒ 那 `budget` 次**必然全部**撞上 `ETXTBSY`，读到的是纯粹的重试代价。
    这是 `KR30D3` 要写进出处的那个数。
    """
    out: list[float] = []
    all_failed = 0
    for _ in range(n):
        rd, wr = os.pipe()
        fd = open_write_fd(target)
        holder = os.fork()
        if holder == 0:
            try:
                os.close(wr)
                os.read(rd, 1)  # 阻塞着等放行，期间一直攥着那个写 fd
                os.execv(SPACER, [SPACER])
            except BaseException:
                pass
            os._exit(127)
        os.close(rd)
        os.close(fd)
        t0 = time.perf_counter()
        failed = 0
        for _i in range(budget):
            try:
                child = os.posix_spawn(target, [target], {})
            except OSError as e:
                if e.errno != errno.ETXTBSY:
                    die(f"exec 撞到的不是 ETXTBSY 而是 {e.errno} —— 台架造错了")
                failed += 1
                continue
            os.waitpid(child, 0)
            break
        out.append(time.perf_counter() - t0)
        if failed == budget:
            all_failed += 1
        os.write(wr, b"g")
        os.close(wr)
        os.waitpid(holder, 0)
    return {"dt": out, "all_failed": all_failed, "n": n}


# ── 负载 ────────────────────────────────────────────────────────────────────
class Load:
    """起 `k` 个纯自旋进程占 CPU。`stop()` 之前它们一直在跑。"""

    def __init__(self, k: int) -> None:
        self.pids: list[int] = []
        for _ in range(k):
            pid = os.fork()
            if pid == 0:
                try:
                    while True:
                        pass
                except BaseException:
                    pass
                os._exit(0)
            self.pids.append(pid)

    def stop(self) -> None:
        for pid in self.pids:
            try:
                os.kill(pid, 9)
                os.waitpid(pid, 0)
            except OSError:
                pass
        self.pids = []


# ── 打印 ────────────────────────────────────────────────────────────────────
def fmt_us(x: float) -> str:
    return "nan" if x != x else f"{x * 1e6:.0f}"


def arm_row(name: str, arm: dict, budget: int) -> str:
    t = arm["tries"]
    d = arm["dt"]
    n = len(t)
    hit = sum(1 for x in t if x > 1)
    within = sum(1 for x in t if x <= budget)
    tf = [float(x) for x in t]
    return (
        f"| {name} | {n} | {hit} | {within} ({100.0 * within / n:.1f}%) | "
        f"{min(t)}/{pct(tf, 0.5):.0f}/{pct(tf, 0.9):.0f}/{pct(tf, 0.99):.0f}/{max(t)} | "
        f"{fmt_us(min(d))}/{fmt_us(pct(d, 0.5))}/{fmt_us(pct(d, 0.9))}/"
        f"{fmt_us(pct(d, 0.99))}/{fmt_us(max(d))} |"
    )


ARM_A = "A 假窗口（写 fd 立刻就关，不 fork 持有者）"
ARM_B = "B 自然泄漏 · Python 子进程（fork 持有者，hold=0）"
ARM_C = "C 长窗口对照"
# 臂 B2 不进这张表 —— 它是 §3b 的一条曲线，不是一格。


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=300, help="臂 A/B 每臂、以及臂 B2 每个 offset 点的样本数")
    ap.add_argument("--long-n", type=int, default=60, help="臂 C 的样本数（它每个样本要真等一个长窗口）")
    ap.add_argument("--sweep-n", type=int, default=200, help="扫描每个 hold 点的样本数")
    ap.add_argument("--cost-n", type=int, default=200, help="§2「打完 N 次要多久」的样本数")
    ap.add_argument(
        "--src",
        default=os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "..",
            "src-tauri",
            "src",
            "backend",
            "control",
            "local_backend.rs",
        ),
    )
    ap.add_argument(
        "--mutate",
        default="",
        choices=["", "fake-long-window"],
        help="R30M1：把长窗口对照臂也改成「写 fd 立刻就关」",
    )
    args = ap.parse_args()

    budget, src_path = read_budget(os.path.abspath(args.src))
    if not os.path.exists(SPACER):
        die(f"{SPACER} 不在 —— 台架跑不了")

    workdir = tempfile.mkdtemp(prefix="kr30-")
    # ⚠ 目录名与文件名一律取**中性名**（`brief` 12·6g：断言里不许混进夹具的名字）。
    target = make_target(workdir, "probe-bin")
    nproc = os.cpu_count() or 1

    print("=" * 78)
    print("K-R30 · KR30D1 —— ETXTBSY 那个窗口的实测")
    print("=" * 78)
    print(f"· 量于        ：{time.strftime('%Y-%m-%d %H:%M:%S')}（本地钟；时长一律用单调钟 perf_counter）")
    print(f"· 内核        ：{platform.platform()}")
    print(f"· python      ：{sys.version.split()[0]}")
    print(f"· nproc       ：{nproc}")
    print(f"· 目标文件    ：{target}（`/bin/true` 的拷贝，真 ELF）")
    print(f"· spawn 原语  ：os.posix_spawn（glibc 走 clone(CLONE_VM|CLONE_VFORK)+execve，")
    print("                与 Rust `Command::spawn` 在无 pre_exec 时走的是同一条路）")
    print(f"· 预算 N      ：{budget} —— **现打自** {src_path}（本量具不复述那个数）")
    print(f"· 变异开关    ：{args.mutate or '（无）'}")
    print(f"· 样本数      ：臂A/B 各 {args.n} · 臂B2 每个 offset 点 {args.n} · 臂C {args.long_n}"
          f" · 扫描每点 {args.sweep_n} · §2 {args.cost_n}")
    print()

    # ── §1 台架自检（先证台架真造得出竞态，再谈读数） ──────────────────────
    print("── §1 台架自检 ────────────────────────────────────────────────────")
    probe = run_arm(target, 40, 0.02, True)  # 20ms 窗口：必须撞得到
    probe_hits = sum(1 for x in probe["tries"] if x > 1)
    print(f"· 20ms 窗口 40 样本，撞上过 ETXTBSY 的：{probe_hits}/40")
    if probe_hits == 0:
        die("台架一次竞态都没造出来 —— 后面的读数一律作废（这正是「造不出来」那一支）")
    noholder = run_arm(target, 40, 0.0, False)
    noholder_hits = sum(1 for x in noholder["tries"] if x > 1)
    print(f"· 不 fork 持有者 40 样本，撞上过 ETXTBSY 的：{noholder_hits}/40  ← 这一格构造上应当是 0")
    print(f"· 🔴 非空对照：恒同格 {noholder_hits}/40 与非零格 {probe_hits}/40 **同在这一次输出里**。")
    print("   两格若一起是 0 ⇒ 上一行已经 FAIL 退出了，不会有下面的表。")
    print()

    results: dict[str, dict] = {}

    for load_name in ("idle", "loaded"):
        load = None
        if load_name == "loaded":
            load = Load(nproc)
            spin(0.3)  # 让负载进程真跑起来
        la = os.getloadavg()
        print(f"── §2 「打完 {budget} 次立即重试」要花多久 · 负载 {load_name}"
              f"（loadavg {la[0]:.2f}/{la[1]:.2f}/{la[2]:.2f}） ──")
        cost = measure_budget_cost(target, args.cost_n, budget)
        d = cost["dt"]
        print(f"· 分母：{cost['n']} 个样本；持有者阻塞在管道上（不烧 CPU），窗口由本函数在打完之后才关。")
        verdict = "满格 ⇒ 这一格读到的是纯粹的重试代价" if cost["all_failed"] == cost["n"] \
            else "🔴 不是满格 ⇒ 有样本提前成功了，这一格读数要打折"
        print(f"· {budget} 次全部撞上 ETXTBSY 的样本：{cost['all_failed']}/{cost['n']}  ← {verdict}")
        print(f"· 打完 {budget} 次耗时 µs（min/p50/p90/p99/max）："
              f"{fmt_us(min(d))}/{fmt_us(pct(d, 0.5))}/{fmt_us(pct(d, 0.9))}/"
              f"{fmt_us(pct(d, 0.99))}/{fmt_us(max(d))}")
        c_n_p50 = pct(d, 0.5)
        c_n_p99 = pct(d, 0.99)
        print()

        # ── §2b 自然窗口的**上界**（全在 C 里，不吃解释器开销） ──────────────
        print(f"── §2b 一次 posix_spawn 的墙钟时长（= fork→execve 窗口的上界）· 负载 {load_name} ──")
        w = measure_spawn_wall(args.cost_n)
        print(f"· 分母：{len(w)} 次成功的 spawn；计时**不含** waitpid。")
        print(f"· µs（min/p50/p90/p99/max）：{fmt_us(min(w))}/{fmt_us(pct(w, 0.5))}/"
              f"{fmt_us(pct(w, 0.9))}/{fmt_us(pct(w, 0.99))}/{fmt_us(max(w))}")
        w_p99 = pct(w, 0.99)
        cover = float("inf") if w_p99 <= 0 else c_n_p50 / w_p99
        print(f"· ⇒ 「打完 {budget} 次立即重试」(p50 {fmt_us(c_n_p50)} µs) ÷ 「窗口上界」(p99 {fmt_us(w_p99)} µs)"
              f" = **{cover:.1f} 倍**")
        print()

        # 长窗口对照臂的 hold：由刚量到的 C_N **现算**，不拍数（上限 120 ms 是为了跑得完）。
        long_hold = 0.0 if args.mutate == "fake-long-window" else min(0.120, max(0.010, c_n_p99 * 10.0))
        ratio = float("inf") if c_n_p99 <= 0 else long_hold / c_n_p99
        print(f"── §3 三臂 · 负载 {load_name} ─────────────────────────────────────")
        if args.mutate == "fake-long-window":
            print("· 臂 C 的 hold = 0 —— **被 R30M1 切成了假窗口**")
        else:
            print(f"· 臂 C 的 hold = {fmt_us(long_hold)} µs = {ratio:.1f} × C_N(p99)"
                  f"（现算，上限 120000 µs；{'明显长于' if ratio >= 2 else '🔴 只有不到 2 倍，对照力度不足'}）")
        arms = {
            ARM_A: run_arm(target, args.n, 0.0, False),
            ARM_B: run_arm(target, args.n, 0.0, True),
            ARM_C: run_arm(target, args.long_n, long_hold, True),
        }
        print()
        print("| 臂 | n | 撞上过 ETXTBSY | 在预算内 (tries<=N) | tries min/p50/p90/p99/max | "
              "从关 fd 到 exec 成功 µs min/p50/p90/p99/max |")
        print("|---|---|---|---|---|---|")
        for k, v in arms.items():
            print(arm_row(k, v, budget))
        print()

        # ── §3b 臂 B2：C 级自然泄漏的**生存曲线** ────────────────────────────
        print(f"── §3b 臂 B2（C 级自然泄漏）· 延迟关 fd 扫描 · 负载 {load_name} ──")
        print("· offset = 收到「我马上要 spawn 了」之后再等多久才关自己那把写 fd。")
        print("· 命中率随 offset 的下降 ≈ 那个孩子 fork→execve 窗口的生存函数。")
        print("· 🔴 `lag` 是本臂的**自检位**：主线程关 fd 的时刻 − 那个线程 `posix_spawn` 返回的时刻。")
        print("     `lag` 恒 > 0 ⇒ 主线程只在孩子 execve **之后**才醒 ⇒ 这一臂构造上撞不到，")
        print("     那个 0 命中是**判不了**，不是「窗口太短」。")
        print("| offset µs | n | 撞上过 ETXTBSY | 在预算内 | tries p50/max | lag µs p50 | lag>0 的样本 |")
        print("|---|---|---|---|---|---|---|")
        b2 = []
        for off_us in (0, 10, 25, 50, 100, 200, 400):
            arm = run_arm_thread_spawn(target, args.n, off_us / 1e6)
            t = arm["tries"]
            lg = arm["lag"]
            tf = [float(x) for x in t]
            hit = sum(1 for x in t if x > 1)
            within = sum(1 for x in t if x <= budget)
            pos = sum(1 for x in lg if x == x and x > 0)
            b2.append((off_us, hit, len(t), within, pos))
            print(
                f"| {off_us} | {len(t)} | {hit} | {within} ({100.0 * within / len(t):.1f}%) | "
                f"{pct(tf, 0.5):.0f}/{max(t)} | {fmt_us(pct(lg, 0.5))} | {pos}/{len(lg)} |"
            )
        b2_hits = sum(h for _o, h, _n, _w, _p in b2)
        b2_n = sum(n for _o, _h, n, _w, _p in b2)
        b2_within = sum(w for _o, _h, _n, w, _p in b2)
        b2_pos = sum(p for _o, _h, _n, _w, p in b2)
        print(f"· 合计：{b2_hits}/{b2_n} 个样本撞上过；{b2_within}/{b2_n} 在预算内；"
              f"`lag > 0` 的 {b2_pos}/{b2_n}。")
        if b2_hits == 0 and b2_pos > b2_n * 0.9:
            print("· ⇒ 🔴 **本臂判不了**（不是「够」也不是「不够」）：GIL 把两个线程串成了一条线，")
            print("     `os.posix_spawn` 期间 CPython 不放 GIL ⇒ 主线程恒在孩子 execve 之后才拿到 GIL。")
            print("     这一格的结论**只能**由 §2b 那个上界给，不许拿这里的 0 当读数。")
        print()

        # ── §4 扫描：预算 N 盖得住多长的窗口 ────────────────────────────────
        print(f"── §4 扫描 hold → 在预算内的比例 · 负载 {load_name} ──────────────")
        print("| hold µs | n | 撞上过 ETXTBSY | 在预算内 | tries p50/p99/max | dt µs p50/p99/max |")
        print("|---|---|---|---|---|---|")
        sweep = []
        for hold_us in (0, 50, 100, 200, 500, 1000, 2000, 5000, 10000):
            arm = run_arm(target, args.sweep_n, hold_us / 1e6, True)
            t = arm["tries"]
            dd = arm["dt"]
            tf = [float(x) for x in t]
            hit = sum(1 for x in t if x > 1)
            within = sum(1 for x in t if x <= budget)
            sweep.append((hold_us, within / len(t)))
            print(
                f"| {hold_us} | {len(t)} | {hit} | {within} ({100.0 * within / len(t):.1f}%) | "
                f"{pct(tf, 0.5):.0f}/{pct(tf, 0.99):.0f}/{max(t)} | "
                f"{fmt_us(pct(dd, 0.5))}/{fmt_us(pct(dd, 0.99))}/{fmt_us(max(dd))} |"
            )
        print()

        results[load_name] = {
            "cost_p50": c_n_p50,
            "cost_p99": c_n_p99,
            "arms": arms,
            "sweep": sweep,
            "b2": b2,
            "loadavg": la,
            "long_hold": long_hold,
            "wall_p50": pct(w, 0.5),
            "wall_p99": w_p99,
            "cover": cover,
        }
        if load is not None:
            load.stop()

    # ── §5 结论行（机器读的摘要） ──────────────────────────────────────────
    print("── §5 摘要（每一行都只复述上面表里的数） ──────────────────────────")
    for load_name, r in results.items():
        a = r["arms"]

        def within(key: str) -> float:
            t = a[key]["tries"]
            return sum(1 for x in t if x <= budget) / len(t)

        b2_hits = sum(h for _o, h, _n, _w, _p in r["b2"])
        b2_n = sum(n for _o, _h, n, _w, _p in r["b2"])
        b2_pos = sum(p for _o, _h, _n, _w, p in r["b2"])
        b2_read = "判不了（GIL 串行，见 §3b）" if (b2_hits == 0 and b2_pos > b2_n * 0.9) else f"撞上过 {b2_hits}/{b2_n}"
        print(f"· [{load_name}] 打完 {budget} 次 = p50 {fmt_us(r['cost_p50'])} µs / p99 {fmt_us(r['cost_p99'])} µs")
        print(f"· [{load_name}] 窗口上界（一次 posix_spawn 的墙钟）= p50 {fmt_us(r['wall_p50'])} µs"
              f" / p99 {fmt_us(r['wall_p99'])} µs ⇒ 预算盖住它 {r['cover']:.1f} 倍")
        print(f"· [{load_name}] 在预算内的比例：臂A {within(ARM_A) * 100:.1f}% · 臂B {within(ARM_B) * 100:.1f}%"
              f" · 臂B2 {b2_read} · 臂C {within(ARM_C) * 100:.1f}%")
        gap = within(ARM_A) - within(ARM_C)
        sep = "分得开" if gap > 0.5 else "🔴 分不开（两臂同答 ⇒ 尺子瞎了）"
        print(f"· [{load_name}] 尺子：臂A - 臂C = {gap * 100:.1f} 个百分点 ⇒ {sep}")
        knee = [h for h, w in r["sweep"] if w < 1.0]
        print(f"· [{load_name}] 扫描拐点：hold 到 {knee[0] if knee else '（扫描范围内没出现）'} µs 时"
              f"「在预算内」首次跌破 100%")
    print()
    print("── §6 诚实边界 ────────────────────────────────────────────────────")
    print("· 臂 B 那个窗口里有一大块是 **CPython `os.fork()` 在子进程里的收尾**（`PyOS_AfterFork_Child`：")
    print("  重建 GIL / 线程状态 / 各种锁 / at_fork 钩子）—— 那是**解释器的账，不是内核的账**。")
    print("  ⇒ 臂 B 是真机（Rust/C 子进程）那个窗口的**上界**，别当成它本身。")
    print("· 与生产段形状最像的臂 B2 **判不了**（GIL 把两个线程串成一条线）⇒ C 级那个窗口的")
    print("  唯一可信读数是 §2b 的上界（`posix_spawn` 的墙钟时长，vfork 挂起父线程到孩子 execve 为止）。")
    print("· 本量具跑在容器里；真机的负载 / 文件系统 / 内核版本都可能不同 ⇒ 这是**沙箱读数**，")
    print("  不是「真机上就是这样」。这两句不许压成一句（件文件 §4 逐字）。")
    print("· 「在预算内」只说明**这一趟**试够了；它不说明真机上撞不到更长的窗口。")

    shutil.rmtree(workdir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
