/**
 * 图集页：按目录分节，每张图一个标题 ＋ 一句「这是什么状态」，一页翻完。
 * 每次跑把本次的结果并进 `shots.json`（只重截了几张时，其余的照旧列着、位置不变）。
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const DIR_TITLES = {
  "主窗口": "主窗口",
  "主窗口-卡片": "主窗口 · 消息流里的各种卡",
  "主窗口-面板": "主窗口 · 面板与浮层",
  "设置": "设置窗",
  "历史": "历史 / 查看窗",
  "文件窗口": "文件窗口（egui）",
};

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

export function writeIndex(out, results) {
  const ledger = path.join(out, "shots.json");
  const prev = existsSync(ledger) ? JSON.parse(readFileSync(ledger, "utf8")) : [];
  const byId = new Map(prev.map((r) => [r.id, r]));
  for (const r of results) byId.set(r.id, r);
  // 已有的照原位（第一次全量跑的那个顺序），新出现的接在后面；分节按界面固定的先后排。
  const order = [...prev.map((r) => r.id), ...results.map((r) => r.id).filter((id) => !prev.some((r) => r.id === id))];
  const rank = (d) => {
    const i = Object.keys(DIR_TITLES).indexOf(d);
    return i < 0 ? 99 : i;
  };
  const all = order.map((id) => byId.get(id)).sort((a, b) => rank(a.dir) - rank(b.dir));
  writeFileSync(ledger, JSON.stringify(all, null, 1));

  const dirs = [...new Set(all.map((r) => r.dir))];
  const toc = dirs.map((d) => `<a href="#${esc(d)}">${esc(DIR_TITLES[d] ?? d)}（${all.filter((r) => r.dir === d).length}）</a>`).join(" · ");
  const sections = dirs
    .map((d) => {
      const cards = all
        .filter((r) => r.dir === d)
        .map(
          (r) => `
      <figure id="${esc(r.id)}"${r.ok ? "" : ' class="bad"'}>
        <figcaption><b>${esc(r.title)}</b><span>${esc(r.desc)}</span><code>${esc(r.file)}</code></figcaption>
        <a href="${esc(r.file)}"><img loading="lazy" src="${esc(r.file)}" width="${r.width}" height="${r.height}" alt="${esc(r.title)}"></a>
      </figure>`,
        )
        .join("");
      return `<section id="${esc(d)}"><h2>${esc(DIR_TITLES[d] ?? d)}</h2>${cards}</section>`;
    })
    .join("\n");

  const html = `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>cc-monitor 界面图集</title>
<style>
  :root { color-scheme: light dark; --bg: #f6f6f4; --fg: #1d1d1b; --muted: #6b6b66; --card: #fff; --line: #ddd; }
  @media (prefers-color-scheme: dark) { :root { --bg: #161616; --fg: #e8e8e4; --muted: #9a9a94; --card: #202020; --line: #333; } }
  body { margin: 0; padding: 16px 24px 64px; background: var(--bg); color: var(--fg); font: 14px/1.6 system-ui, "Noto Sans CJK SC", sans-serif; }
  header { position: sticky; top: 0; background: var(--bg); padding: 8px 0; border-bottom: 1px solid var(--line); z-index: 1; }
  header h1 { margin: 0 0 4px; font-size: 18px; }
  header nav a { color: inherit; }
  header p { margin: 4px 0 0; color: var(--muted); font-size: 12px; }
  h2 { margin: 32px 0 12px; font-size: 16px; }
  figure { margin: 0 0 28px; background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 12px; max-width: 1320px; }
  figure.bad { border-color: #c0392b; }
  figcaption { display: flex; gap: 12px; align-items: baseline; flex-wrap: wrap; margin-bottom: 8px; }
  figcaption span { color: var(--muted); }
  figcaption code { margin-left: auto; color: var(--muted); font-size: 11px; }
  img { max-width: 100%; height: auto; border: 1px solid var(--line); display: block; }
</style>
</head>
<body>
<header>
  <h1>cc-monitor 界面图集（${all.filter((r) => r.ok).length} 张）</h1>
  <nav>${toc}</nav>
  <p>网页那三扇窗在无头 Chromium 里画（≈ Windows 上的 WebView2；Linux 真窗口是 WebKitGTK，字体与滚动条细节会有出入），数据是合成的；文件窗口在私有 Xvfb 上起真窗口、窗口内截屏。</p>
</header>
${sections}
</body>
</html>
`;
  writeFileSync(path.join(out, "index.html"), html);
}
