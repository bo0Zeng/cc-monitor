/**
 * 截图台架的假后端答 `prefs-read`（那台后端 `prefs.json` 那一份）：凡是场景里写了偏好的（`world.prefs` 里有那一台），
 * 答出来的就是那一份（`ok`）；没写的那几台照后端「没有这份」那一形答（`absent`、后端缺省那一份）。
 *
 * 为什么有这一格：偏好从 `config.json` 搬进各台后端（第 7 批）之后，台架的 `prefs-read` 起先一律答「没有这份」，
 * 场景写了什么都盖不住设置页那几格，截图照样过。
 *
 * 人群：`scenes/` 的全部场景 × 每个场景的每台机器（答法取场景世界里那一条，场景整条覆盖的也算）；外加一个自造的世界
 *（自检：写了偏好的那一台真答出那一份，没写的那一台答缺省 —— 场景里一份偏好都没写时，上面那一条在空人群上恒绿）。
 * 买不到：真后端读 `prefs.json` 的那一段（`control/machine_prefs.rs`，后端自己的判据管）。
 */
import { describe, expect, it } from "vitest";
import { SCENES } from "./scenes";
import { defaultWorld } from "./fake/world";
import type { World } from "./fake/types";
import { PREFS_UNSET } from "./fake/machine";

const read = (w: World, o: string) => w.ops["prefs-read"](o, {}, w) as { state: string; prefs: unknown };

describe("假后端的 prefs-read 照场景答", () => {
  it("自检：写了偏好的那台答那一份，没写的那台答「没有这份」与缺省", () => {
    const w = defaultWorld();
    const mine = { launchers: { claude: "ccm --resume" }, resumeInTmux: true, models: { work: "claude-opus-4-1" }, contextLimits: { opus: 1_000_000 }, agentHomes: {}, pbDir: null };
    w.prefs = { devbox: mine };
    expect(read(w, "devbox")).toMatchObject({ state: "ok", prefs: mine });
    expect(read(w, "<local>")).toMatchObject({ state: "absent", prefs: PREFS_UNSET });
  });

  it("★★ 每个场景的每台：写了偏好的 ⇒ 答出来的就是那一份；没写的 ⇒ 缺省", () => {
    const off: string[] = [];
    for (const s of SCENES) {
      const w = s.world();
      for (const o of w.machines) {
        const want = w.prefs?.[o];
        const got = read(w, o);
        const ok = want ? got.state === "ok" && JSON.stringify(got.prefs) === JSON.stringify(want) : got.state === "absent" && JSON.stringify(got.prefs) === JSON.stringify(PREFS_UNSET);
        if (!ok) off.push(`${s.id} · ${o}`);
      }
    }
    expect(off).toEqual([]);
  });
});
