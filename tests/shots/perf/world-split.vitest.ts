/**
 * 性能台架的世界分两份（A17-45-3-6-3）：页里那份只有会话名单与答法、不造记录正文，带配方名；
 * 伺服照配方在 node 里造整份盘（`disks.ts`）。两份对得上：同一批会话；页里接着写的那一轮接得上盘上那一份的末一条。
 */
import { describe, expect, it } from "vitest";
import { hugeTurns, PERF_TURNS, perfHugeWorld, perfMainWorld, perfWorld, turn } from "./world";
import { recipeDisk } from "./disks";
import { Convo, jsonlOf } from "../fake/records";

describe("性能世界：页里不造正文，伺服照配方造盘", () => {
  it("页里那份：每个会话都没有记录正文，带配方名", () => {
    for (const w of [perfWorld(), perfMainWorld(), perfHugeWorld(50)]) {
      expect(w.diskFrom).toBeTypeOf("string");
      expect(w.sessions.every((s) => s.convo === null)).toBe(true);
    }
    expect(perfWorld().sessions).toHaveLength(PERF_TURNS.length);
    expect(perfHugeWorld(50).sessions).toHaveLength(hugeTurns(50).length);
  });

  it("伺服那份：同一批会话、每个会话的记录文件都在盘上", () => {
    const page = perfMainWorld();
    const { machines, tails } = recipeDisk(page.diskFrom!);
    const files = Object.values(machines).flatMap((m) => Object.keys(m.files));
    for (const s of page.sessions) {
      expect(files).toContain(jsonlOf(s.cwd, s.sid));
      expect(tails[s.sid]).toBeDefined();
    }
    expect(Object.values(machines).reduce((a, m) => a + m.live.length, 0)).toBe(page.sessions.length);
  });

  it("页里接着写的一轮接得上盘上那份的末一条（父链 · 不撞号）", () => {
    const page = perfWorld();
    const { machines, tails } = recipeDisk(page.diskFrom!);
    const s = page.sessions[0];
    const rel = jsonlOf(s.cwd, s.sid);
    const text = Object.values(machines).find((m) => rel in m.files)!.files[rel];
    const disk = text.trim().split("\n").map((l) => JSON.parse(l) as { uuid?: string });
    const lastUuid = [...disk].reverse().find((l) => l.uuid)!.uuid;
    const c = Convo.resume(tails[s.sid]);
    turn(c, s.cwd, 1000, false);
    const added = c.lines.filter((l): l is Record<string, unknown> => typeof l !== "string");
    expect(added.length).toBeGreaterThan(0);
    expect(added.find((l) => typeof l.uuid === "string")!.parentUuid).toBe(lastUuid);
    const seen = new Set(Object.values(machines).flatMap((m) => Object.values(m.files).flatMap((t) => [...t.matchAll(/"uuid":"([^"]+)"/g)].map((x) => x[1]))));
    for (const l of added) if (typeof l.uuid === "string") expect(seen.has(l.uuid)).toBe(false);
  });
});
