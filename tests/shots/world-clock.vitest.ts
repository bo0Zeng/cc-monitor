/**
 * 截图台架造的会话记录与「现在」对得上：每个场景的世界里，每段会话记录（连同子 agent 那几段）的时刻都不晚于开页那一刻；
 * 活着的会话（在跑 / 在等 / 后台任务）末一条在一天之内；pidfile 说的进程起始时刻不晚于它记录的第一条；一段记录里后写的那条不早于前一条
 * （场景往已经挪到「现在」前面的那一段后面接着写时，拿固定时刻 `clock()` 很容易写到它前面去）。
 *
 * 为什么有这一格：交真后端之后，后端照记录与盘上的时刻算成品 —— 记录写的是一周前（或几小时之后）、会话却说它此刻在跑，
 * 历史页的时段写成「10-01 17:00 – 10-08 14:30」、「已等 2m」配一条一周前的待批准；进程起在开页那一刻，
 * 记录里 12 分钟前起的后台命令被后端当成上一个进程留下的、会话头少了那条命令。截出来「长得像」，数据自己打架。
 *
 * 人群：`scenes/` 里登记的全部场景（`SCENES`）的世界。
 * 买不到：场景在 `act` 里开页之后再写进去的行（`appendLines` · `writeFile`）不在这里；只看带 `timestamp` 的那几种行。
 */
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { Convo } from "./fake/records";
import { liveOf } from "./fake/world";

// 同 `real/pool.mjs::SHOTS_NOW_MS`（页里拨到这一刻；这里拨系统钟，世界照它造）。
const NOW = Date.parse("2026-10-08T14:30:00+08:00");
const DAY = 24 * 3600_000;

function stamps(c: Convo): number[] {
  const own = c.lines.filter((l): l is Record<string, unknown> => typeof l !== "string" && typeof l.timestamp === "string").map((l) => Date.parse(l.timestamp as string));
  return [...own, ...[...c.children.values()].flatMap(stamps)];
}

describe("截图台架的会话记录与「现在」对得上", () => {
  beforeAll(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(NOW);
  });
  afterAll(() => vi.useRealTimers());

  it("每个场景：记录不晚于开页那一刻 · 活会话末一条在一天之内 · 进程起在记录第一条之前", async () => {
    const { SCENES } = await import("./scenes");
    const bad: string[] = [];
    let seen = 0;
    for (const s of SCENES) {
      const w = s.world();
      for (const x of w.sessions) {
        const c = x.convo;
        if (!c) continue;
        const ts = stamps(c);
        if (ts.length === 0) continue;
        seen++;
        const last = Math.max(...ts);
        if (last > Date.now()) bad.push(`${s.id} · ${x.sid}：末一条比开页那一刻晚 ${Math.round((last - Date.now()) / 60_000)} 分钟`);
        const alive = !x.ended && (x.activity === "working" || x.activity === "needs_you" || x.activity === "background_work");
        if (alive && Date.now() - last > DAY) bad.push(`${s.id} · ${x.sid}：说它此刻${x.activity}，末一条却是 ${Math.round((Date.now() - last) / DAY)} 天前`);
        const own = c.lines.filter((l): l is Record<string, unknown> => typeof l !== "string" && typeof l.timestamp === "string").map((l) => Date.parse(l.timestamp as string));
        if (own.some((t, i) => i > 0 && t < own[i - 1])) bad.push(`${s.id} · ${x.sid}：记录里有一条比前一条早`);
        const born = liveOf(x).startedAt;
        if (born !== undefined && born > Math.min(...ts)) bad.push(`${s.id} · ${x.sid}：进程起在它记录第一条之后`);
      }
    }
    expect(seen).toBeGreaterThan(300);
    expect(bad).toEqual([]);
  });
});
