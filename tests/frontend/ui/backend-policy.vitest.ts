/**
 * 〔B2 · 条 66 · `设计/01 §3.3b`〕「退出行为」那个值**搬走了** —— 这一份钉「搬干净了」。
 *
 * ## 搬家前这里钉的是什么（留档，别当成今天的形状）
 *
 * P2s-Y4：策略**存进 monitor 的 config.json、每台机各记各的、先推后存、写盘串行**，
 * 外加一条接线钉「启动时真的推了一次」。那一整套的前提是「值住 monitor 这一侧」，
 * 而 `§3.3b ①` 逐字给过它真会犯的错（C 拿自己那份默认值悄悄改掉 B 的行为）⇒ 条 66 把值搬到了
 * **后端所在那台机器上**，只有后端写、决定那一刻现读。⇒ 那几条判据的对象整个不存在了，随之退役。
 *
 * ## 今天钉什么
 *
 * - **推送链与本地持久化零命中**（带正控）：前端源码里再也叫不出那条推送命令、
 *   也不再有「把这个值存进 config.json」的那几个名字；而新的两条命令**真的在**包装层里。
 * - 三态 / 壳 / 那几句话的逐格等号住 `settings/backend-section.vitest.ts`（`E3` / `E4`）。
 *
 * 〔PB1 · `设计/90 §4` 阶段 B〕原来这里还有一组「K-P3 那句读数 —— 三档逐格钉死」（TS 那份 `describeBackendHealth` 的运行时判据）。
 * 三档判定搬到了后端（`backend_policy.rs::health_face`）、TS 那份删了 ⇒ 那一组的对象不存在了，随之退役；
 * 判准逐格（全零 · 四个计数各自 = 1）住 `tests/frontend/shell/backend_policy_tests.rs::the_health_face_is_judged_by_the_whole_ledger_not_by_crashes_alone`，
 * TS 零实现由 `judgment-single-home.vitest.ts` 的 J21 钉。
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

/** `src/` 下全部生产 `.ts`，剥掉注释 —— 只看代码（遍历住 `test-support`，本文件不另写一份）。 */
function frontendCode(): { rel: string; code: string }[] {
  return productionTsFiles("src").map((f) => ({
    rel: f.file,
    code: stripComments(f.text, "ts"),
  }));
}

describe("B2 · 「退出行为」那个值不住前端", () => {
  it("★★ 推送链与本地持久化零命中 —— 两向带正控", () => {
    const files = frontendCode();
    expect(
      files.length,
      "只扫到这么几份前端源码 —— 遍历坏了，零命中在空人群上恒绿",
    ).toBeGreaterThan(100);
    // 名字**运行时拼**：本文件自己不在人群里（它住 tests/），但这样读者一眼看得出它们是「已退役的名字」。
    const retired = [
      ["set_backend_", "kill_on_exit"],
      ["init", "BackendPolicy"],
      ["push", "PolicyToBackend"],
      ["set", "KillOnExit"],
      ["backend", "Policy:"],
    ].map((p) => p.join(""));
    for (const name of retired) {
      const hits = files.filter((f) => f.code.includes(name)).map((f) => f.rel);
      expect(
        hits,
        `退役的名字 \`${name}\` 还在前端代码里：${hits.join(" / ")}\n` +
          "`设计/01 §3.3b ②④`：monitor 的 config 里不许再留一份、那条推送随之退役 —— 留着就是第二个真相源。",
      ).toEqual([]);
    }
    // ★ 正控：问 / 交写那两问**真的**还在（否则上面那几条零命中可能只是「整片都没了」）。
    // 〔C4c · 第四波 4B〕两条包装层命令退役，设置页经通道直接说后端那两条帧命令 ⇒ 正控改成钉那两处 `chan.call`。
    const sec = files.find((f) => f.rel === "src/frontend/ui/settings/backend-section.ts");
    expect(sec, "找不到 settings/backend-section.ts —— 人群坏了").toBeDefined();
    for (const op of ["exit-policy-read", "exit-policy-set"]) {
      expect(sec!.code.includes(`chan.call(origin, "${op}"`), `设置页没有经通道说 ${op}`).toBe(true);
    }
  });
});
