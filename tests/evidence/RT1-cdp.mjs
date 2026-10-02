#!/usr/bin/env node
// RT1 · 经 WebView2 的远程调试口在 monitor 页里求值（本机侧跑；虚拟机的 9222 由 ssh -L 转到本机 19222）。
// 守的要求：虚拟机当真机测试资源 —— 读数要从真 WebView2 里拿，不是 jsdom。
// 用法：node RT1-cdp.mjs [页面 url 或标题的子串] < 表达式.js   （表达式可 await；结果按 JSON 打印）
// 口子怎么开：monitor 进程环境里 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222（RT1-lib.ps1::Rt1-Env）。
const PORT = process.env.RT1_CDP_PORT || "19222";
const want = process.argv[2] || "";
const expr = await new Promise((res) => { let s = ""; process.stdin.on("data", (d) => (s += d)); process.stdin.on("end", () => res(s)); });
const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const pages = list.filter((p) => p.type === "page" && (p.url.includes(want) || p.title.includes(want)));
if (!pages.length) { console.error("没有匹配的页：", list.map((p) => `${p.title} ${p.url}`)); process.exit(2); }
const url = pages[0].webSocketDebuggerUrl.replace(/127\.0\.0\.1:\d+/, `127.0.0.1:${PORT}`);
const ws = new WebSocket(url);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0; const wait = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && wait.has(d.id)) { wait.get(d.id)(d); wait.delete(d.id); } };
const call = (method, params) => new Promise((r) => { const i = ++id; wait.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const body = `(async () => { ${expr} })()`;
const r = await call("Runtime.evaluate", { expression: body, awaitPromise: true, returnByValue: true, timeout: 120000 });
if (r.result?.exceptionDetails) { console.log("EXC", JSON.stringify(r.result.exceptionDetails, null, 1)); process.exit(1); }
console.log(JSON.stringify(r.result?.result?.value, null, 1));
ws.close();
