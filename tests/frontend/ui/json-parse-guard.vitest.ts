/**
 * 界面一个解码 · 一种订阅（零命中守卫，按字符串找、去注释后数）。
 *
 * - 界面生产代码里 `JSON.parse(` 只住两处：`ipc/decode.ts`（线上来的字节）与 `local-storage.ts`（浏览器本地存储）。
 *   别处线上来的东西经 `decodeJson`，读本地 / 会话存储经 `parseStored`。
 * - 「那台某样东西变了」只经 `events.ts` 订：设置几页调 `subscribeTopics`，不再各自 `chan.subscribe` 一条 `changed/<topic>`、
 *   也不再拿 `bindEvents` 配一套空的会话回调来订（`bindEvents` 只给主窗口）。
 *
 * 抓不到的：经 `any` 拼出来的 `JSON["parse"]`；它只让「又开一条」被看见。
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

const UI = "src/frontend/ui";
const files = productionTsFiles(UI).map((f) => ({ file: f.file, code: stripComments(f.text, "ts") }));

const holders = (needle: RegExp): string[] => files.filter((f) => needle.test(f.code)).map((f) => f.file).sort();

describe("界面一个解码", () => {
  it("扫到了界面的生产源码（人群不空）", () => {
    expect(files.length).toBeGreaterThan(50);
    expect(files.some((f) => f.file === `${UI}/ipc/decode.ts`)).toBe(true);
  });

  it("`JSON.parse(` 只住 ipc/decode.ts 与 local-storage.ts", () => {
    expect(holders(/\bJSON\.parse\(/)).toEqual([`${UI}/ipc/decode.ts`, `${UI}/local-storage.ts`]);
  });
});

describe("一种订阅", () => {
  it("`changed/<topic>` 那一批格只在 events.ts 里读（changedItems 只有它调）", () => {
    expect(holders(/\bchangedItems\(/)).toEqual([`${UI}/changed-stream.ts`, `${UI}/events.ts`]);
  });

  it("`bindEvents(` 只有主窗口调", () => {
    expect(holders(/\bbindEvents\(/)).toEqual([`${UI}/events.ts`, `${UI}/main.ts`]);
  });
});
