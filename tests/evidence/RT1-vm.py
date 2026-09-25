#!/usr/bin/env python3
"""RT1 · 驱动 Win11 虚拟机真机测试的本机侧小工具（在 laptop 上跑，不在虚拟机里跑）。

守的要求：用户裁决 V115 逐字「能，用虚拟机」——「在虚拟机里留一个计划任务，用用户登录的那个桌面
（session 1）跑真窗口」。🔴 虚拟机上的持久改动只许：一个计划任务（`ccm-rt1`）＋ 测试用临时目录
（`%LOCALAPPDATA%\\Temp\\rt1`）；本工具不建第二个任务、不写别处。

子命令（脚本一律从 stdin 读）：
  ps            —— 经 SSH 在 session 0 跑一段 PowerShell（只做文件与进程的读、拷、收；**不许起 GUI**：
                   session 0 没有交互窗口站，`真相源/106 §4.3` 的 `0x80070578` 就是这一形）。
  job [秒]      —— 把脚本写成 `rt1\\jobs\\job.ps1`，`schtasks /run` 触发计划任务 `ccm-rt1`，
                   它在**用户登录的桌面 session 1** 里跑；等 runner 写出 END 再把那一次的日志取回来。
  push 本地 远端 / pull 远端 本地 —— scp（远端路径用正斜杠）。
  shot 名字     —— `virsh screenshot` 取整屏，存成 PNG（证据）。

进虚拟机的路（记忆 `win11-vm-headless-access`）：显式 `-i ~/.ssh/winvm_ed25519 user@192.0.2.11`；
**别用** `ssh win11`（配置过期）。远端 PowerShell 的三个坑（`真相源/106` 附录）：
EncodedCommand（UTF-16LE）＋ 先设 UTF-8 输出 ＋ 滤掉 `#< CLIXML` 尾巴。
"""
import base64
import os
import re
import subprocess
import sys
import time

HOST = "user@192.0.2.11"
KEY = os.path.expanduser("~/.ssh/winvm_ed25519")
SSH = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "LogLevel=ERROR", "-i", KEY, HOST]
ROOT = r"C:\Users\user\AppData\Local\Temp\rt1"
TASK = "ccm-rt1"
NOISE = re.compile(r"^#< CLIXML$|^<Objs Version=|post-quantum|store now, decrypt later|openssh\.com/pq\.html")


def ps(script: str, check: bool = False) -> str:
    body = "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; " + script
    enc = base64.b64encode(body.encode("utf-16-le")).decode()
    p = subprocess.run(SSH + [f"powershell -NoProfile -NonInteractive -EncodedCommand {enc}"],
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = "\n".join(l for l in (p.stdout + p.stderr).splitlines() if not NOISE.search(l))
    if check and p.returncode != 0:
        raise SystemExit(f"ps 失败 rc={p.returncode}\n{out}")
    return out


def job(script: str, timeout: float = 120) -> str:
    """在 session 1 里跑一段脚本，回它的输出。runner 把 START/END 写进 runner-last.txt。"""
    # 脚本经 base64 落盘成带 BOM 的 UTF-8（PS 5.1 读无 BOM 的 .ps1 按 ANSI 解，中文会坏）。
    b64 = base64.b64encode(script.encode("utf-8")).decode()
    before = ps(f"(Get-Content '{ROOT}\\logs\\runner-last.txt' -ErrorAction SilentlyContinue | Select-Object -First 1)")
    ps(
        f"$b=[Convert]::FromBase64String('{b64}'); $t=[Text.Encoding]::UTF8.GetString($b); "
        f"[IO.File]::WriteAllText('{ROOT}\\jobs\\job.ps1', $t, (New-Object Text.UTF8Encoding $true)); "
        f"Start-ScheduledTask -TaskName {TASK}",
        check=True,
    )
    t0 = time.time()
    while time.time() - t0 < timeout:
        time.sleep(1.0)
        st = ps(f"Get-Content '{ROOT}\\logs\\runner-last.txt' -ErrorAction SilentlyContinue")
        lines = [l for l in st.splitlines() if l.strip()]
        if lines and lines[0] != before.strip() and any(l.startswith("END ") for l in lines):
            m = re.search(r"log=(\S+)", lines[0])
            if not m:
                return st
            return ps(f"Get-Content '{m.group(1)}' -Encoding UTF8")
    return f"<job 超时 {timeout}s>\n" + ps(f"Get-Content '{ROOT}\\logs\\runner-last.txt'")


def scp(src: str, dst: str) -> None:
    subprocess.run(["scp", "-q", "-o", "LogLevel=ERROR", "-i", KEY, src, dst], check=True)


def shot(name: str, outdir: str) -> str:
    os.makedirs(outdir, exist_ok=True)
    ppm = os.path.join(outdir, name + ".ppm")
    png = os.path.join(outdir, name + ".png")
    subprocess.run(["sudo", "-n", "virsh", "screenshot", "win11", ppm], check=True, capture_output=True)
    from PIL import Image  # 本机有 PIL；virsh 的输出其实是 PNG
    Image.open(ppm).save(png)
    os.remove(ppm)
    return png


def main() -> None:
    a = sys.argv[1:]
    if not a:
        print(__doc__)
        sys.exit(2)
    cmd = a[0]
    if cmd == "ps":
        print(ps(sys.stdin.read()))
    elif cmd == "job":
        print(job(sys.stdin.read(), float(a[1]) if len(a) > 1 else 120))
    elif cmd == "push":
        scp(a[1], f"{HOST}:{a[2]}")
    elif cmd == "pull":
        scp(f"{HOST}:{a[1]}", a[2])
    elif cmd == "shot":
        print(shot(a[1], a[2] if len(a) > 2 else "."))
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
