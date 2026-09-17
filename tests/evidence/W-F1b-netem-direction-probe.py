#!/usr/bin/env python3
"""W-F1b 摸底量具 ①：**netem 打在容器自己的 netns 上，对「宿主 → 容器」那个方向生不生效**。

件文件 `§0a⑤③` 那一格 —— PM 明说没量过，`WF1bD3` 的候选路「丙」整条压在它上面。
死值验 `W1bM2` 要「正反各打一次」：本量具每一条规则都跑 **前 / 中 / 后** 三个读数
（加规则前、加了规则、删了规则），分不开就是判不了。

## 形状（与台架 `e2e/weak-net/rig.sh` 的差别就是本件的正题）

    台架：  容器 A（跑 `ssh` 命令）── 自建 docker 网络 ──> 容器 B（sshd）
            netem 打在 **A 的 eth0**（客户端侧 egress）

    本量具：**宿主**（客户端）────────── 自建 docker 网络 ──> 容器 B
            netem 打在 **B 的 eth0**（**服务端**侧 egress）—— 客户端那一侧是宿主，
            而宿主网卡按定框 `W3` **一个规则都不许打** ⇒ 只剩 B 自己这一侧可打。

⇒ 于是问题变成：**只打得到 B 的 egress 时，「宿主 → B」那个方向还量得出弱网吗？**

## 为什么用「带宽」当方向判别器，不用「延迟」

`ping` 的 RTT 一来一回各过一次链路 ⇒ 只打 B 的 egress 也会让 RTT 变大。
**RTT 变大证明不了「宿主→B 被整形了」** —— 那是一族空真（回程被整形，去程原样，RTT 照样涨）。
所以方向判别器取**单向吞吐**：

  · 下行 B→宿主：B 用 `nc` 吐 N 字节，宿主读到 EOF 为止 —— 走的是 **B 的 egress**。
  · 上行 宿主→B：宿主发 N 字节后 `shutdown(SHUT_WR)`，B 的 `nc` 收到 EOF 退出、
    连接关闭 ⇒ 宿主 `recv()` 返回 0 —— 那一刻数据**已经被 B 收完**。
    走的是 **B 的 ingress**（= 宿主的 egress，而宿主那侧没有规则）。

`tbf rate 2mbit` 下：下行该被压到理论值附近，上行**若也变慢**才说明 ingress 方向被管到了。

⚠ 上行读数里混着一格**不是「ingress 被整形」的**慢：TCP 的 ACK 是 B 发的，
   ACK 走 B 的 egress ⇒ 规则会拖慢 ACK ⇒ 上行也会**间接**变慢。
   所以本量具对上行**不只看快慢，还看「慢成什么样」**：被 `rate` 直接整形的方向
   会贴着理论传输时间，而只是被 ACK 拖慢的方向不会。两者的分界写在输出的判读里。

## 红线（本量具自己受同一套约束）

  · `tc` 只在 **B 自己的 netns** 里下（一律经 `docker exec`），**不碰宿主任何网卡**。
    量具每一步都把宿主的 `tc qdisc show` 抓一次，断言宿主面 `netem|tbf` **恒零命中**。
  · 运行时**不用** `--network host`，只给 `--cap-add=NET_ADMIN`，**不给 `--privileged`**。
  · 不起真 claude / 真 daemon / 真 tmux server；不碰用户真实的 `~/.ssh` 与 `~/.claude`。
  · 用**自己的**容器名与网络名（`ccmon-wf1b-*`），不与台架的 `ccmon-weaknet-*` 撞名；
    跑完全删，并对宿主的 `ip link` / `docker network ls` 前后逐字比。

## 被测对象指向哪棵树

本量具**不读代码仓**，被测对象是「docker + 宿主内核 + `ccmon-weaknet:latest` 镜像」这台机器本身。
⇒ 换一棵工作树重跑，读数应当相同；读数变了说明的是机器变了，不是树变了。

用法：  python3 evidence/W-F1b-netem-direction-probe.py            # 跑一趟
       python3 evidence/W-F1b-netem-direction-probe.py --clean    # 只清残留
"""

from __future__ import annotations

import hashlib
import json
import re
import socket
import subprocess
import sys
import time

IMG = "ccmon-weaknet:latest"
NET = "ccmon-wf1b-net"
CB = "ccmon-wf1b-b"
DEV = "eth0"

PAYLOAD = 1 << 20  # 1 MiB
RATE_MBIT = 2
DELAY_MS = 200
# 理论传输毫秒 = 载荷比特 / 速率（与 rig.sh 同一算法，整数运算，无浮点）
THEORY_BW_MS = PAYLOAD * 8 * 1000 // (RATE_MBIT * 1_000_000)

PORT_DOWN = 5202
PORT_UP = 5201

CONNECT_TIMEOUT = 4.0
IO_TIMEOUT = 30.0


def run(cmd: list[str], timeout: float = 60.0) -> tuple[int, str, str]:
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout, p.stderr


def host_qdisc() -> str:
    return run(["tc", "qdisc", "show"])[1]


def host_links() -> list[str]:
    out = run(["ip", "-o", "link", "show"])[1]
    return sorted(ln.split(": ", 2)[1].split("@")[0] for ln in out.splitlines() if ": " in ln)


def docker_networks() -> list[str]:
    return sorted(run(["docker", "network", "ls", "--format", "{{.Name}}"])[1].split())


def assert_host_clean(step: str, log: list) -> bool:
    """🔴 红线断言：宿主面任何网卡上都不许出现 netem / tbf。"""
    q = host_qdisc()
    hits = [ln for ln in q.splitlines() if re.search(r"\bnetem\b|\btbf\b", ln)]
    log.append({"step": step, "host_netem_tbf_hits": len(hits), "lines": hits})
    return not hits


def cleanup() -> None:
    run(["docker", "rm", "-f", CB], timeout=60)
    run(["docker", "network", "rm", NET], timeout=60)


def dexec(args: list[str], timeout: float = 30.0) -> tuple[int, str, str]:
    return run(["docker", "exec", CB] + args, timeout=timeout)


def tc_add(spec: list[str]) -> tuple[bool, str]:
    rc, _, err = dexec(["tc", "qdisc", "add", "dev", DEV, "root"] + spec)
    return rc == 0, err.strip()


def tc_del() -> None:
    dexec(["tc", "qdisc", "del", "dev", DEV, "root"])


def tc_show() -> str:
    return dexec(["tc", "qdisc", "show", "dev", DEV])[1].strip()


# ── 三把量具 ────────────────────────────────────────────────────────────────
def measure_rtt(ip: str) -> float | None:
    """宿主 → B 的 ICMP RTT（毫秒）。一来一回 ⇒ **不是**方向判别器，只当佐证。"""
    rc, out, _ = run(["ping", "-c", "5", "-i", "0.2", "-W", "2", ip], timeout=25)
    m = re.search(r"= [\d.]+/([\d.]+)/", out)
    if rc != 0 or not m:
        return None
    return float(m.group(1))


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
    """B → 宿主 N 字节的毫秒数。走 **B 的 egress** ⇒ 规则该直接管到它。"""
    proc = subprocess.Popen(
        ["docker", "exec", CB, "sh", "-c",
         f"head -c {PAYLOAD} /dev/zero | nc -l -N {PORT_DOWN}"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
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
    """宿主 → B N 字节的毫秒数。走 **B 的 ingress**（宿主 egress 无规则）。

    完成信号：宿主 `shutdown(SHUT_WR)` 后，B 的 `nc` 读到 EOF 退出 ⇒ 回一个 FIN
    ⇒ 宿主 `recv()` 得 0。那一刻 B **已经把 N 字节收完**（不是「宿主发出去了」）。
    """
    proc = subprocess.Popen(
        ["docker", "exec", CB, "sh", "-c", f"nc -l {PORT_UP} > /dev/null"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
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
                n = s.send(buf[: min(65536, PAYLOAD - sent)])
                sent += n
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
    return {
        "label": label,
        "rtt_ms": measure_rtt(ip),
        "down_ms": measure_download(ip),
        "up_ms": measure_upload(ip),
    }


def fmt(v) -> str:
    return "断" if v is None else str(v)


def main() -> int:
    if "--clean" in sys.argv:
        cleanup()
        print(f"[probe] 已清掉 {CB} / {NET}（若在）")
        return 0

    rc, _, _ = run(["docker", "image", "inspect", IMG])
    if rc != 0:
        print(f"需要镜像 {IMG}（由 e2e/weak-net/build-image.sh 建）", file=sys.stderr)
        return 2

    log: list = []
    report: dict = {"payload_bytes": PAYLOAD, "theory_bw_ms": THEORY_BW_MS, "samples": []}

    links_before = host_links()
    nets_before = docker_networks()
    qdisc_before = host_qdisc()
    report["host_before"] = {
        "qdisc_md5": hashlib.md5(qdisc_before.encode()).hexdigest(),
        "links": links_before,
        "networks": nets_before,
    }
    ok_lines = assert_host_clean("开跑前", log)

    cleanup()  # 上一趟崩掉的残留
    if run(["docker", "network", "create", NET])[0] != 0:
        print("建网络失败", file=sys.stderr)
        return 2
    rc, _, err = run(
        ["docker", "run", "-d", "--name", CB, "--network", NET,
         "--cap-add=NET_ADMIN", IMG, "sleep", "infinity"], timeout=90)
    if rc != 0:
        print(f"起容器失败：{err}", file=sys.stderr)
        cleanup()
        return 2

    try:
        ip = run(["docker", "inspect", "-f",
                  "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}", CB])[1].strip()
        report["b_ip"] = ip

        # netns 分离的证据（这一格证明「打在容器自己的 netns 里」不是一句话）
        host_ns = open("/proc/self/ns/net").read() if False else \
            run(["readlink", "/proc/self/ns/net"])[1].strip()
        cb_ns = dexec(["readlink", "/proc/self/ns/net"])[1].strip()
        report["netns"] = {"host": host_ns, "B": cb_ns, "separated": host_ns != cb_ns}

        # `--privileged` 没给的证据：容器的有效能力集里只该有 NET_ADMIN 这一档的增量
        report["cap_probe"] = dexec(["sh", "-c", "grep CapEff /proc/self/status"])[1].strip()

        ok_lines &= assert_host_clean("容器起好·未加规则", log)

        report["samples"].append(sample(ip, "① 无规则（基线）"))

        # ── 规则一：带宽（方向判别器）──────────────────────────────────────
        added, err = tc_add(["tbf", "rate", f"{RATE_MBIT}mbit", "burst", "32kbit",
                             "latency", "400ms"])
        report["tbf_added"] = {"ok": added, "err": err, "qdisc": tc_show()}
        ok_lines &= assert_host_clean("B 加了 tbf", log)
        report["samples"].append(sample(ip, f"② B egress tbf rate {RATE_MBIT}mbit"))
        tc_del()
        report["samples"].append(sample(ip, "③ 删掉 tbf（回得来吗）"))

        # ── 规则二：延迟（佐证，不当判别器）────────────────────────────────
        added, err = tc_add(["netem", "delay", f"{DELAY_MS}ms"])
        report["netem_delay_added"] = {"ok": added, "err": err, "qdisc": tc_show()}
        ok_lines &= assert_host_clean("B 加了 netem delay", log)
        report["samples"].append(sample(ip, f"④ B egress netem delay {DELAY_MS}ms"))
        tc_del()
        report["samples"].append(sample(ip, "⑤ 删掉 delay"))

        # ── 规则三：全断 ──────────────────────────────────────────────────
        added, err = tc_add(["netem", "loss", "100%"])
        report["netem_cut_added"] = {"ok": added, "err": err, "qdisc": tc_show()}
        ok_lines &= assert_host_clean("B 加了 loss 100%", log)
        report["samples"].append(sample(ip, "⑥ B egress netem loss 100%"))
        tc_del()
        report["samples"].append(sample(ip, "⑦ 删掉 loss（回得来吗）"))

        # ── ingress 那一侧今天有没有路：ifb 能不能在容器 netns 里建起来 ──────
        rc_ifb, _, err_ifb = dexec(["ip", "link", "add", "ifb0", "type", "ifb"])
        report["ifb_in_ns"] = {"rc": rc_ifb, "err": err_ifb.strip()}
        if rc_ifb == 0:
            dexec(["ip", "link", "del", "ifb0"])
    finally:
        cleanup()

    qdisc_after = host_qdisc()
    report["host_after"] = {
        "qdisc_md5": hashlib.md5(qdisc_after.encode()).hexdigest(),
        "links": host_links(),
        "networks": docker_networks(),
    }
    report["host_unchanged"] = {
        "links": host_links() == links_before,
        "networks": docker_networks() == nets_before,
        "qdisc_md5": report["host_after"]["qdisc_md5"] == report["host_before"]["qdisc_md5"],
    }
    ok_lines &= assert_host_clean("收尾后", log)
    report["redline_host_netem_tbf_always_zero"] = bool(ok_lines)
    report["redline_log"] = log

    # ── 印表 ────────────────────────────────────────────────────────────────
    print("=" * 78)
    print(f"W-F1b · netem 方向摸底  载荷={PAYLOAD} 字节  理论 {RATE_MBIT}mbit 传输 ≈ {THEORY_BW_MS}ms")
    print(f"B 的 IP={report['b_ip']}  netns 分离={report['netns']['separated']}"
          f"（宿主 {report['netns']['host']} / B {report['netns']['B']}）")
    print("=" * 78)
    print(f"{'读数':<34}{'RTT(ms)':>10}{'下行 B→宿主(ms)':>18}{'上行 宿主→B(ms)':>18}")
    for s in report["samples"]:
        print(f"{s['label']:<34}{fmt(s['rtt_ms']):>10}"
              f"{fmt(s['down_ms']):>18}{fmt(s['up_ms']):>18}")
    print("-" * 78)
    print(f"🔴 宿主面 netem/tbf 恒零命中：{report['redline_host_netem_tbf_always_zero']}"
          f"（{len(log)} 个检查点）")
    print(f"宿主 ip link 前后一致={report['host_unchanged']['links']} · "
          f"docker network 前后一致={report['host_unchanged']['networks']} · "
          f"tc qdisc md5 前后一致={report['host_unchanged']['qdisc_md5']}")
    print(f"容器 netns 里建 ifb：rc={report['ifb_in_ns']['rc']} "
          f"{report['ifb_in_ns']['err'] or '(成功)'}")
    print(f"容器 CapEff：{report['cap_probe']}")
    print("=" * 78)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
