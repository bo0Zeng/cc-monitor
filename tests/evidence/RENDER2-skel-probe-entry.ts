/**
 * 〔RENDER2 · `设计/10 §7` 第 1 条「第一级的外框常数（`height-estimate.ts` 的 `SKEL_*`）是估的、没有秤对拍」〕
 * **骨架外框秤的真浏览器探针**（只这一把，题面点名的那一把）。
 *
 * 与秤 2 同一份语料、同一套生产样式、同一个 `.stream > .stream-content`，关掉 `content-visibility` 让每张卡真排版；
 * 读的不是卡自己的高，而是**卡在流里占的位置**（下一张卡的顶 − 这一张的顶，含折叠后的卡间距）——骨架占位顶的就是这个。
 * 每张卡另记头 / 体的高，外框 = 位置 − 头 − 体（第一级公式里正文之外那一段）。只 Chromium 一个引擎（生产 WebView2 同族）。
 * 结果写 `window.__RESULT` ＋ `window.__DONE`，由 `RENDER2-skel-run.ts` 读回、写金样 `RENDER2-skel-golden.json`。
 */
import "../../src/frontend/ui/styles/layers.css";
import "../../src/frontend/ui/styles/reset.css";
import "../../src/frontend/ui/styles/tokens.css";
import "../../src/frontend/ui/styles/layout.css";
import "../../src/frontend/ui/styles/shared.css";
import "../../src/frontend/ui/styles.css";
import fixtureJsonl from "../__fixtures__/scale2-height-records.jsonl?raw";
import { buildCorpus } from "../frontend/ui/scale2-height-corpus";

declare global {
  interface Window {
    __RESULT?: string;
    __DONE?: boolean;
  }
}

const h = (el: Element | null): number => (el ? el.getBoundingClientRect().height : 0);

async function main(): Promise<void> {
  try {
    await (document as unknown as { fonts?: { ready?: Promise<unknown> } }).fonts?.ready;
  } catch {
    /* 没有 FontFaceSet 的引擎照跑 */
  }
  const stream = document.createElement("div");
  stream.className = "stream active";
  stream.id = "skel-truth";
  const content = document.createElement("div");
  content.className = "stream-content";
  stream.appendChild(content);
  document.body.appendChild(stream);
  const off = document.createElement("style");
  off.textContent = "#skel-truth .stream-content > * { content-visibility: visible !important; } html, body { margin: 0; }";
  document.head.appendChild(off);
  const corpus = buildCorpus(fixtureJsonl);
  for (const item of corpus) content.appendChild(item.element);
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const rows = corpus.slice(0, -1).map((item, i) => {
    const el = item.element;
    const slot = corpus[i + 1].element.getBoundingClientRect().top - el.getBoundingClientRect().top;
    return {
      id: item.id,
      cls: item.cls,
      slot,
      header: h(el.querySelector(":scope > .card-header")),
      body: h(el.querySelector(":scope > .card-body")),
    };
  });
  window.__RESULT = JSON.stringify({
    ua: navigator.userAgent,
    colWidth: content.getBoundingClientRect().width,
    rows,
  });
  window.__DONE = true;
}

void main();
