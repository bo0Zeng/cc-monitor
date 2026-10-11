#!/usr/bin/env python3
# 冒烟驱动的小工具：经 adb（台架那台模拟器）按字找控件点 · 敲字 · 截屏（截屏另存一份缩小的给人看）。
# 用法：ui.py dump | tap <字> [第几个] | has <字> | wait <字> [秒] | text <串> | key <键码> | shot <路径.png>
import os, re, subprocess, sys, time, xml.etree.ElementTree as ET

def adb(*a, **k):
    return subprocess.run(["adb", *a], capture_output=True, stdin=subprocess.DEVNULL, **k)

def nodes():
    for _ in range(5):
        r = adb("exec-out", "uiautomator", "dump", "/dev/tty")
        s = r.stdout.decode("utf-8", "replace")
        i = s.find("<?xml"); j = s.rfind("</hierarchy>")
        if i >= 0 and j > 0:
            return list(ET.fromstring(s[i:j + 12]).iter("node"))
        time.sleep(0.5)
    return []

def label(n):
    return " ".join(x for x in (n.get("text"), n.get("content-desc"), n.get("resource-id")) if x)

def find(want, nth=0):
    ns = nodes()
    hit = [n for n in ns if want in (n.get("text"), n.get("content-desc"), n.get("resource-id"))]
    hit = hit or [n for n in ns if want in label(n)]
    return hit[nth] if len(hit) > nth else None

def center(n):
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
    return (x1 + x2) // 2, (y1 + y2) // 2

def main():
    cmd, *a = sys.argv[1:]
    if cmd == "dump":
        for n in nodes():
            if label(n):
                print(n.get("bounds"), label(n))
    elif cmd in ("tap", "has"):
        n = find(a[0], int(a[1]) if len(a) > 1 else 0)
        if n is None:
            print(f"ui: 找不到「{a[0]}」", file=sys.stderr); sys.exit(1)
        if cmd == "tap":
            adb("shell", "input", "tap", *map(str, center(n)))
    elif cmd == "wait":
        end = time.time() + float(a[1] if len(a) > 1 else 30)
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

main()
