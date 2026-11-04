#!/usr/bin/env python3
"""截图台架的 WebKitGTK 那一份浏览器（由 `webkit.mjs` 起，在私有 Xvfb 上跑）。

用法：webkit-shoot.py <base-url> <out-dir> <boot.js> <上限毫秒> [正则]

照清单页（`tests/shots/manifest.html`）逐幕开页，等 `window.__shots` 落定（做完 / 没做成），从 Xvfb 截那一块存 PNG。
服务（`serve.mjs`：vite ＋ 真后端池 ＋ 假适配层）由外面起，本脚本只当浏览器。
标准输出一行一个 JSON：先 `{"count": n}`，再每幕一行 `{"scene": 清单那一项, "state", "error", "layout", "unhandled", "real": [...]}`；
`real` 列出这一幕要的真输入（`drag` · `pointer`：调试口那一套，这里没有），由 `webkit.mjs` 记成跳过。
"""
import json
import os
import re
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402
from PIL import ImageGrab  # noqa: E402

base, out, boot_path, cap_ms = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
only = re.compile(sys.argv[5]) if len(sys.argv) > 5 else None
with open(boot_path, encoding="utf-8") as f:
    boot = f.read()
# 减少动画：WebKitGTK 没有调试口那一句模拟，改成把过渡与动画时长压成 0（截下来那一帧不落在半途）。
NO_MOTION = (
    "document.addEventListener('DOMContentLoaded',()=>{const s=document.createElement('style');"
    "s.textContent='*,*::before,*::after{animation-duration:0s!important;transition-duration:0s!important}';"
    "document.head.append(s);});"
)
POLL_MS = 200
# 最后一帧落定（字体 · 重画）：Xvfb 上没有合成器，窗口那一帧可能停在早先，先逼一次重画再等这么久。
SETTLE_MS = 1500

ucm = WebKit2.UserContentManager()
ucm.add_script(
    WebKit2.UserScript.new(
        boot + "\n" + NO_MOTION,
        WebKit2.UserContentInjectedFrames.TOP_FRAME,
        WebKit2.UserScriptInjectionTime.START,
        None,
        None,
    )
)
view = WebKit2.WebView.new_with_user_content_manager(ucm)
win = Gtk.Window()
win.set_decorated(False)
win.move(0, 0)
win.add(view)
win.show_all()
loop = GLib.MainLoop()


def say(obj):
    print(json.dumps(obj, ensure_ascii=False), flush=True)


def ev(js, cb):
    def done(v, res):
        try:
            r = v.evaluate_javascript_finish(res)
            cb(json.loads(r.to_json(0)) if r is not None and not r.is_undefined() else None)
        except Exception as e:  # noqa: BLE001 —— 页里抛的、页没了，一律当这一问没答上
            cb({"__err": str(e)})

    view.evaluate_javascript(js, -1, None, None, None, done)


def after_load(cb):
    h = [None]

    def on(v, e):
        if e == WebKit2.LoadEvent.FINISHED:
            v.disconnect(h[0])
            cb()

    h[0] = view.connect("load-changed", on)


def later(ms, fn):
    GLib.timeout_add(ms, lambda: (fn(), False)[1])


PROBE = (
    "({state: window.__shots ? window.__shots.state : null,"
    " error: window.__shots ? String(window.__shots.error || '') : '',"
    " layout: (window.__shots && window.__shots.layout) || [],"
    " unhandled: (window.__shots && window.__shots.unhandled) || [],"
    " real: window.__shots ? ['drag', 'pointer'].filter((k) => window.__shots[k]) : []})"
)

scenes = []


def snap(sc):
    f = os.path.join(out, sc["dir"], sc["id"] + ".png")
    os.makedirs(os.path.dirname(f), exist_ok=True)
    ImageGrab.grab(bbox=(0, 0, sc["width"], sc["height"]), xdisplay=os.environ["DISPLAY"]).save(f)


def shoot(i):
    if i >= len(scenes):
        loop.quit()
        return
    sc = scenes[i]
    win.resize(sc["width"], sc["height"])
    view.set_size_request(sc["width"], sc["height"])
    q = f"&{sc['query']}" if sc.get("query") else ""
    polls = [0]

    def finish(r):
        def back():
            win.resize(sc["width"], sc["height"])

            def grab():
                row = {"scene": sc, **r}
                try:
                    snap(sc)
                except Exception as e:  # noqa: BLE001
                    row["state"] = "failed"
                    row["error"] = f"截不下来：{e}"
                say(row)
                shoot(i + 1)

            later(SETTLE_MS, grab)

        win.resize(sc["width"], sc["height"] - 1)
        later(300, back)

    def poll():
        polls[0] += 1

        def got(r):
            r = r if isinstance(r, dict) else {}
            if "__err" in r:
                r = {"state": None, "error": r["__err"]}
            if r.get("state") in ("done", "failed"):
                finish(r)
            elif polls[0] * POLL_MS > 2 * cap_ms:
                finish({**r, "state": "failed", "error": f"等了 {2 * cap_ms // 1000} 秒，场景既没做完也没报错（state {r.get('state')}）"})
            else:
                later(POLL_MS, poll)

        ev(PROBE, got)

    after_load(lambda: later(POLL_MS, poll))
    view.load_uri(f"{base}/{sc['page']}.html?scene={GLib.uri_escape_string(sc['id'], None, False)}{q}")


def start():
    def got(m):
        if not isinstance(m, list):
            later(500, lambda: ev("window.__shotsManifest || null", got))
            return
        scenes.extend(s for s in m if not only or only.search(s["id"]))
        say({"count": len(scenes)})
        shoot(0)

    after_load(lambda: ev("window.__shotsManifest || null", got))
    view.load_uri(f"{base}/tests/shots/manifest.html")


GLib.idle_add(lambda: (start(), False)[1])
loop.run()
