#!/usr/bin/env python3
"""截图台架的 WebKitGTK 那一份浏览器（由 `webkit.mjs` 起，在私有 Xvfb 上跑）。

用法：webkit-shoot.py <base-url> <out-dir> <boot.js> <上限毫秒> [正则]

照清单页（`tests/shots/manifest.html`）逐幕开页，等 `window.__shots` 落定（做完 / 没做成），让 WebKit 自己画一张可见区（`webkit_web_view_get_snapshot`）存 PNG。
不从 Xvfb 截：没有合成器的 Xvfb 上，WebKitGTK 往窗口上交帧到某一刻就停了、之后换页也不再交（页里照常画，自己那一张是对的），
从 X 截到的是停住那一刻的旧帧，几幕逐字节相同。
服务（`serve.mjs`：vite ＋ 真后端池 ＋ 假适配层）由外面起，本脚本只当浏览器。
标准输出一行一个 JSON：先 `{"count": n}`，再每幕一行 `{"scene": 清单那一项, "state", "error", "layout", "unhandled", "real": [...]}`；
`real` 列出这一幕要的真输入（`drag` · `pointer`：调试口那一套，这里没有），由 `webkit.mjs` 记成跳过。
"""
import ctypes
import json
import os
import re
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

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
# 场景报做完之后再等这么久才截（字体 · 最后一次重画落定）。
SETTLE_MS = 1500

# WebKit 自己画的那一张是 cairo 的面：python-gi 的 cairo 转换（python3-gi-cairo）这台不一定有 ⇒ 经 ctypes 直接调 C 那几个口。
_wk = ctypes.CDLL("libwebkit2gtk-4.1.so.0")
_cairo = ctypes.CDLL("libcairo.so.2")
_READY = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p)
_wk.webkit_web_view_get_snapshot.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, _READY, ctypes.c_void_p]
_wk.webkit_web_view_get_snapshot_finish.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
_wk.webkit_web_view_get_snapshot_finish.restype = ctypes.c_void_p
_cairo.cairo_surface_write_to_png.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
_cairo.cairo_surface_write_to_png.restype = ctypes.c_int
_cairo.cairo_surface_destroy.argtypes = [ctypes.c_void_p]
ctypes.pythonapi.PyCapsule_GetPointer.restype = ctypes.c_void_p
ctypes.pythonapi.PyCapsule_GetPointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
# 回调对象在 C 那一头回来之前不能被回收。
_pending = []

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


def snap(sc, then):
    """WebKit 画这一页的可见区存成 PNG；`then(None)` 是存好了，`then(那一句)` 是没存成。"""
    f = os.path.join(out, sc["dir"], sc["id"] + ".png")
    os.makedirs(os.path.dirname(f), exist_ok=True)

    def ready(src, res, _):
        _pending.remove(cb)
        err = ctypes.c_void_p()
        surface = _wk.webkit_web_view_get_snapshot_finish(src, res, ctypes.byref(err))
        if not surface:
            then("WebKit 没交出这一页的画面")
            return
        status = _cairo.cairo_surface_write_to_png(surface, f.encode())
        _cairo.cairo_surface_destroy(surface)
        then(None if status == 0 else f"PNG 没写成（cairo 状态 {status}）")

    cb = _READY(ready)
    _pending.append(cb)
    # 区域 0 ＝ 可见区（WEBKIT_SNAPSHOT_REGION_VISIBLE）；选项 0 ＝ 照原样（WEBKIT_SNAPSHOT_OPTIONS_NONE）。
    _wk.webkit_web_view_get_snapshot(ctypes.pythonapi.PyCapsule_GetPointer(view.__gpointer__, None), 0, 0, None, cb, None)


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
        def grab():
            row = {"scene": sc, **r}

            def saved(err):
                if err:
                    row["state"] = "failed"
                    row["error"] = f"截不下来：{err}"
                say(row)
                shoot(i + 1)

            snap(sc, saved)

        later(SETTLE_MS, grab)

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
