#!/usr/bin/env python3
# 冒烟驱动的小工具：经 adb（台架那台模拟器）按字找控件点 · 敲字 · 截屏（截屏另存一份缩小的给人看）。
# 用法：ui.py dump | tap <字> [第几个] | has <字> | wait <字> [秒] | text <串> | key <键码> | shot <路径.png>
# 等都按条件等、期限按本机忙闲放宽（within）；读屏时碰上「应用无响应」框就记一条（anrs）、点「等待」关掉再读。
import os, re, subprocess, sys, time, xml.etree.ElementTree as ET

def adb(*a, **k):
    """一条 adb。模拟器卡住时 adb shell 一直不回：一分钟（按忙闲放宽）没回就当它什么也没回（退出码 124），等它的那一圈照自己的期限收。"""
    try:
        return subprocess.run(["adb", *a], capture_output=True, stdin=subprocess.DEVNULL, timeout=60 * patience(), **k)
    except subprocess.TimeoutExpired as e:
        return subprocess.CompletedProcess(e.cmd, 124, e.stdout or b"", e.stderr or b"")


def patience():
    """本机忙时期限放宽的倍数：一分钟负载 / 核数，夹在 1–3（忙时模拟器每一步慢几倍，但不无限等）。"""
    return min(3.0, max(1.0, os.getloadavg()[0] / (os.cpu_count() or 1)))


def within(secs):
    """[secs] 秒的期限按此刻的忙闲放宽后落在哪一刻。"""
    return time.time() + secs * patience()


# 读屏时撞见的「应用无响应」框：(时刻, 框上的字)。冒烟照实记、关掉接着走，有一条就不算绿。
anrs = []


def _dump():
    r = adb("exec-out", "uiautomator", "dump", "/dev/tty")
    s = r.stdout.decode("utf-8", "replace")
    i = s.find("<?xml"); j = s.rfind("</hierarchy>")
    if i < 0 or j < 0:
        return None
    try:
        return list(ET.fromstring(s[i:j + 12]).iter("node"))
    except ET.ParseError:
        return None


def _anr(ns):
    """屏上是「应用无响应」框 ⇒ 记下、点「等待」（App 留着接着跑），回真；不是回假。"""
    key = next((n for n in ns if n.get("resource-id") == "android:id/aerr_wait"), None)
    if key is None:
        return False
    anrs.append((time.strftime("%T"), " ".join(n.get("text") for n in ns if n.get("text"))))
    adb("shell", "input", "tap", *map(str, center(key)))
    time.sleep(1)
    return True


def nodes(secs=10):
    """此刻屏上的控件。界面一直不空闲 · dump 失败就接着读，到期限还读不出回空。"""
    end = within(secs)
    while True:
        ns = _dump()
        if ns is not None and not _anr(ns):
            return ns
        if time.time() >= end:
            return ns or []
        time.sleep(0.5)

def label(n):
    return " ".join(x for x in (n.get("text"), n.get("content-desc"), n.get("resource-id")) if x)

def find_in(ns, want, nth=0):
    hit = [n for n in ns if want in (n.get("text"), n.get("content-desc"), n.get("resource-id"))]
    hit = hit or [n for n in ns if want in label(n)]
    return hit[nth] if len(hit) > nth else None


def find(want, nth=0):
    return find_in(nodes(), want, nth)

def bounds(n):
    return tuple(map(int, re.findall(r"\d+", n.get("bounds"))))


def center(n):
    x1, y1, x2, y2 = bounds(n)
    return (x1 + x2) // 2, (y1 + y2) // 2


def settled(want, nth=0, secs=10):
    """[want] 出现、且连着两回读都在同一处（不在滑开 · 不在重排）⇒ 回它；期限到了还在动就回最后看到的那一处，一直没有回 None。"""
    end = within(secs)
    last = seen = None
    while True:
        n = find(want, nth)
        b = bounds(n) if n is not None else None
        if b is not None and b == last:
            return n
        last, seen = b, n or seen
        if time.time() >= end:
            return seen
        time.sleep(0.3)

def main():
    cmd, *a = sys.argv[1:]
    if cmd == "dump":
        for n in nodes():
            if label(n):
                print(n.get("bounds"), label(n))
    elif cmd in ("tap", "has"):
        n = settled(a[0], int(a[1]) if len(a) > 1 else 0) if cmd == "tap" else find(a[0])
        if n is None:
            print(f"ui: 找不到「{a[0]}」", file=sys.stderr); sys.exit(1)
        if cmd == "tap":
            adb("shell", "input", "tap", *map(str, center(n)))
    elif cmd == "wait":
        end = within(float(a[1] if len(a) > 1 else 30))
        while time.time() < end:
            if find(a[0]) is not None:
                return
            time.sleep(1)
        print(f"ui: 等不到「{a[0]}」", file=sys.stderr); sys.exit(1)
    elif cmd == "text":
        adb("shell", "input", "text", a[0].replace(" ", "%s"))
    elif cmd == "key":
        adb("shell", "input", "keyevent", a[0])
    elif cmd == "shot":
        png = adb("exec-out", "screencap", "-p").stdout
        open(a[0], "wb").write(png)
        small = a[0][:-4] + ".small.png"
        try:
            from PIL import Image
            im = Image.open(a[0]); im.resize((im.width * 2 // 5, im.height * 2 // 5)).save(small)
        except ImportError:
            pass

if __name__ == "__main__":
    main()
