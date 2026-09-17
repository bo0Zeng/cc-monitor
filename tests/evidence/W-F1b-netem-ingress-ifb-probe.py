#!/usr/bin/env python3
"""W-F1b 摸底量具 ②：**容器自己的 netns 里能不能把「宿主 → 容器」那个方向也整形**。

量具 ① （`W-F1b-netem-direction-probe.py`）答出的是：`tc ... root` 打在 B 的 `eth0` 上
**只管 egress** ⇒ 带宽那一维只压得住 `B→宿主`，`宿主→B` 纹丝不动（151→152ms）。
本量具接着问的是：**那个方向今天有没有别的路**——`ifb` + `mirred` 把 ingress 重定向出来，
**全套动作仍然只在 B 自己的 netns 里**（定框 `W3` 允许的那一格）。

## 动作（全部经 `docker exec`，一条都不落在宿主网卡上）

    ip link add ifb0 type ifb ; ip link set ifb0 up
    tc qdisc  add dev eth0 handle ffff: ingress
    tc filter add dev eth0 parent ffff: protocol ip u32 match u32 0 0 \\
             action mirred egress redirect dev ifb0
    tc qdisc  add dev ifb0 root tbf rate 2mbit burst 32kbit latency 400ms

## 判读（方向判别器与量具 ① 同一把）

  · **上行 宿主→B** 被压到理论值附近 ⇒ ingress 方向**真被整形了**。
  · **下行 B→宿主** 保持基线 ⇒ 这一刀**只**打在 ingress 上（射程正确，不是粗刀把两边一起打红）。
  两条**都要**成立才算这条路通；只看上行变慢是不够的（`W-F1` 的 `judge` 同族纪律）。

## 🔴 一格**宿主副作用**，写在这里别装作没有

`ip link add ... type ifb` / `action mirred` / `handle ffff: ingress` 会让内核**按需加载**
宿主的内核模块（`ifb` / `act_mirred` / `cls_u32` / `sch_ingress`）。
量具 ① 09-05 实测：跑之前宿主 `lsmod | grep ^ifb` **零命中**，跑完之后 `ifb 16384 0` ——
**是这一步把它装上去的**。⇒ 这条路的前提是「宿主内核有这几个模块且允许自动加载」，
那是**一条环境前提**，不是「容器里自己解决的事」。本量具每趟前后各抓一次 `lsmod`，把它当读数报。

## 红线（同量具 ①）

`tc` 只在 B 自己的 netns 里下 · 运行时不用 `--network host` · 只给 `--cap-add=NET_ADMIN`
不给 `--privileged` · 宿主面 `netem|tbf` 每个检查点都断言零命中 · 跑完全删。

## 被测对象指向哪棵树

不读代码仓；被测对象是「docker + 宿主内核 + `ccmon-weaknet:latest`」这台机器本身。

用法：  python3 evidence/W-F1b-netem-ingress-ifb-probe.py
       python3 evidence/W-F1b-netem-ingress-ifb-probe.py --clean
"""

from __future__ import annotations

import json
import re
import socket
import subprocess
import sys
import time

IMG = "ccmon-weaknet:latest"
NET = "ccmon-wf1b-ing-net"
CB = "ccmon-wf1b-ing-b"
DEV = "eth0"
IFB = "ifb0"

PAYLOAD = 1 << 20
RATE_MBIT = 2
THEORY_BW_MS = PAYLOAD * 8 * 1000 // (RATE_MBIT * 1_000_000)

PORT_DOWN = 5212
PORT_UP = 5211
CONNECT_TIMEOUT = 4.0
IO_TIMEOUT = 40.0

MODS = ("ifb", "act_mirred", "cls_u32", "sch_ingress")


def run(cmd: list[str], timeout: float = 60.0) -> tuple[int, str, str]:
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout, p.stderr


def lsmod_state() -> dict:
    out = run(["lsmod"])[1]
    names = {ln.split()[0] for ln in out.splitlines()[1:] if ln.split()}
    return {m: (m in names) for m in MODS}


def host_netem_hits() -> list[str]:
    q = run(["tc", "qdisc", "show"])[1]
    return [ln for ln in q.splitlines() if re.search(r"\bnetem\b|\btbf\b", ln)]


def cleanup() -> None:
    run(["docker", "rm", "-f", CB], timeout=60)
    run(["docker", "network", "rm", NET], timeout=60)


def dexec(args: list[str], timeout: float = 30.0) -> tuple[int, str, str]:
    return run(["docker", "exec", CB] + args, timeout=timeout)


def dsh(script: str, timeout: float = 30.0) -> tuple[int, str, str]:
    return dexec(["sh", "-c", script], timeout=timeout)


def _wait_listener(ip: str, port: int, deadline: float) -> socket.socket | None:
    while time.monotonic() < deadline:
        s = socket.socket()
        s.settimeout(CONNECT_TIMEOUT)
        try:
            s.connect((ip, port))
            return s
        except OSError:
            s.close()
            time.sleep(0.15)
    return None


def measure_download(ip: str) -> int | None:
    proc = subprocess.Popen(
        ["docker", "exec", CB, "sh", "-c",
         f"head -c {PAYLOAD} /dev/zero | nc -l -N {PORT_DOWN}"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        t0 = time.monotonic()
        s = _wait_listener(ip, PORT_DOWN, t0 + 6)
        if s is None:
            return None
        got = 0
        s.settimeout(IO_TIMEOUT)
        try:
            while True:
                b = s.recv(65536)
                if not b:
                    break
                got += len(b)
        except OSError:
            return None
        finally:
            s.close()
        ms = int((time.monotonic() - t0) * 1000)
        return ms if got == PAYLOAD else None
    finally:
        proc.kill()
        proc.wait(timeout=10)


def measure_upload(ip: str) -> int | None:
    proc = subprocess.Popen(
        ["docker", "exec", CB, "sh", "-c", f"nc -l {PORT_UP} > /dev/null"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        t0 = time.monotonic()
        s = _wait_listener(ip, PORT_UP, t0 + 6)
        if s is None:
            return None
        s.settimeout(IO_TIMEOUT)
        buf = b"\0" * 65536
        sent = 0
        try:
            while sent < PAYLOAD:
                sent += s.send(buf[: min(65536, PAYLOAD - sent)])
            s.shutdown(socket.SHUT_WR)
            while s.recv(65536):
                pass
        except OSError:
            return None
        finally:
            s.close()
        return int((time.monotonic() - t0) * 1000)
    finally:
        proc.kill()
        proc.wait(timeout=10)


def sample(ip: str, label: str) -> dict:
    return {"label": label, "down_ms": measure_download(ip), "up_ms": measure_upload(ip)}


def fmt(v) -> str:
    return "断" if v is None else str(v)


def main() -> int:
    if "--clean" in sys.argv:
        cleanup()
        print(f"[probe] 已清掉 {CB} / {NET}")
        return 0
    if run(["docker", "image", "inspect", IMG])[0] != 0:
        print(f"需要镜像 {IMG}", file=sys.stderr)
        return 2

    report: dict = {"payload_bytes": PAYLOAD, "theory_bw_ms": THEORY_BW_MS,
                    "lsmod_before": lsmod_state(), "samples": [], "redline": []}
    report["redline"].append({"step": "开跑前", "host_netem_tbf": host_netem_hits()})

    cleanup()
    if run(["docker", "network", "create", NET])[0] != 0:
        print("建网络失败", file=sys.stderr)
        return 2
    rc, _, err = run(["docker", "run", "-d", "--name", CB, "--network", NET,
                      "--cap-add=NET_ADMIN", IMG, "sleep", "infinity"], timeout=90)
    if rc != 0:
        print(f"起容器失败：{err}", file=sys.stderr)
        cleanup()
        return 2
    try:
        ip = run(["docker", "inspect", "-f",
                  "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}", CB])[1].strip()
        report["b_ip"] = ip
        report["samples"].append(sample(ip, "① 无规则（基线）"))

        steps = [
            ("ip link add", f"ip link add {IFB} type ifb"),
            ("ip link up", f"ip link set {IFB} up"),
            ("ingress qdisc", f"tc qdisc add dev {DEV} handle ffff: ingress"),
            ("mirred filter", f"tc filter add dev {DEV} parent ffff: protocol ip "
                              f"u32 match u32 0 0 action mirred egress redirect dev {IFB}"),
            ("ifb tbf", f"tc qdisc add dev {IFB} root tbf rate {RATE_MBIT}mbit "
                        f"burst 32kbit latency 400ms"),
        ]
        setup: list = []
        all_ok = True
        for name, cmd in steps:
            rc_s, _, err_s = dsh(cmd)
            setup.append({"step": name, "cmd": cmd, "rc": rc_s, "err": err_s.strip()})
            all_ok &= rc_s == 0
        report["setup"] = setup
        report["setup_all_ok"] = all_ok
        report["qdisc_in_B"] = dsh("tc qdisc show; echo '-- filter --'; "
                                   f"tc filter show dev {DEV} parent ffff:")[1].strip()
        report["redline"].append({"step": "B 装好 ifb ingress", "host_netem_tbf": host_netem_hits()})

        if all_ok:
            report["samples"].append(
                sample(ip, f"② B ingress(ifb) tbf {RATE_MBIT}mbit"))
            dsh(f"tc qdisc del dev {IFB} root; tc qdisc del dev {DEV} ingress; "
                f"ip link del {IFB}")
            report["samples"].append(sample(ip, "③ 全拆掉（回得来吗）"))
        report["redline"].append({"step": "收尾前", "host_netem_tbf": host_netem_hits()})
    finally:
        cleanup()

    report["lsmod_after"] = lsmod_state()
    report["redline"].append({"step": "收尾后", "host_netem_tbf": host_netem_hits()})
    report["redline_host_always_zero"] = all(not r["host_netem_tbf"] for r in report["redline"])

    print("=" * 78)
    print(f"W-F1b · ingress(ifb) 方向摸底  载荷={PAYLOAD} 字节  "
          f"理论 {RATE_MBIT}mbit 传输 ≈ {THEORY_BW_MS}ms   B={report.get('b_ip')}")
    print("=" * 78)
    print(f"{'读数':<40}{'下行 B→宿主(ms)':>18}{'上行 宿主→B(ms)':>18}")
    for s in report["samples"]:
        print(f"{s['label']:<40}{fmt(s['down_ms']):>18}{fmt(s['up_ms']):>18}")
    print("-" * 78)
    print(f"装 ingress 五步全成功：{report.get('setup_all_ok')}")
    for st in report.get("setup", []):
        print(f"  rc={st['rc']}  {st['step']:<16}{st['err']}")
    print(f"🔴 宿主面 netem/tbf 恒零命中：{report['redline_host_always_zero']}")
    print(f"宿主内核模块 跑前={report['lsmod_before']}")
    print(f"宿主内核模块 跑后={report['lsmod_after']}")
    print("=" * 78)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
