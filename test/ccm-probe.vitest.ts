/**
 * `K-R53` `KR53D3`：**「探测出错」与「远端没装」是两件事，不许压成同一个值。**
 *
 * # 病灶逐字（现打于基线 `d231e50`，`src/ccm-probe.ts:41-44`）
 *
 * ```ts
 * } catch {
 *   // 探测失败（ssh 抖动/远端不可达等）→ 安全降级到兜底渲染器，不抛给调用方。
 *   value = NOT_INSTALLED;
 * }
 * ```
 *
 * 那句注释说的处置是对的（**降级**是对的），错的是**它把处置写进了值里**：
 * 一次 ssh 抖动之后，这个 origin 的探测结果**就是** `installed:false`，
 * 而且它还被 `probeCache.set` **缓存了 5 分钟** ⇒ 接下来 5 分钟里，
 * 「这台机器上没有 ccm」这句**它并不知道的话**，会被当成已知事实反复使用。
 *
 * ★ 这与本区那条最贵的病同形：**一个值装了两件事**。
 *
 * # 本条买什么、不买什么
 *
 * 买三格：
 *   ① 出错时的 `state` **不是** `not-installed`（值本身分得开）；
 *   ② 出错时**不进缓存** —— 下一次真的会再探一遍（「不知道」不许被当成答案存起来）；
 *   ③ 真答「没装」时**照旧缓存**（否则第 ② 条会退化成「干脆不缓存了」，那是另一种坏）。
 *
 * ⚠ **不买**：出错之后前端该不该弹点什么。渲染那一侧**仍然降级走兜底**
 * （`remote-launch-run.ts::renderLaunchCommand`）—— 这条判据不改那件事，
 * 也不该改：渲染不出来就走兜底本来就是对的，本条治的是**值**，不是处置。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../src/ipc/commands", () => ({
  commands: { probe_ccm_cli: vi.fn() },
}));

import { commands } from "../src/ipc/commands";
import { probeCcm, invalidateCcmProbeCache } from "../src/ccm-probe";

const probeMock = commands.probe_ccm_cli as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  probeMock.mockReset();
  invalidateCcmProbeCache();
});

describe("KR53D3：探测出错 ≠ 远端没装", () => {
  it("★★ IPC 抛错 ⇒ 结果是「不知道」，不是「没装」", async () => {
    probeMock.mockRejectedValue(new Error("ssh: connect to host x port 22: 连接超时"));
    const r = await probeCcm("h1");
    expect(
      r.state,
      "IPC 出错被写成了「没装」—— 那是把『我不知道』当成『我知道它没有』，\n" +
        "而下游（安装向导 / 机器卡片 / 降级理由）读到的就是一句自信的错答案。",
    ).toBe("unknown");
    // 理由要留得住：出错那一刻唯一的线索就是它。
    expect(r.state === "unknown" && r.error).toContain("连接超时");
  });

  it("★★ 「不知道」不许进缓存 —— 一次抖动不该锁死 5 分钟", async () => {
    probeMock.mockRejectedValueOnce(new Error("一次抖动"));
    const first = await probeCcm("h2");
    expect(first.state).toBe("unknown");
    // 第二发：真答案回来了。若第一发被缓存，这里会拿到缓存里那个「不知道」。
    probeMock.mockResolvedValueOnce({ installed: true, version: "9", capabilities: ["account"] });
    const second = await probeCcm("h2");
    expect(
      second.state,
      "上一次探测出错被缓存了 —— 那 5 分钟里每一次拉起都在用一句没人验证过的话做决定。",
    ).toBe("installed");
    expect(probeMock).toHaveBeenCalledTimes(2);
  });

  it("★ 反向自检：真答「没装」时**照旧**缓存（否则上一条会退化成「干脆不缓存」）", async () => {
    probeMock.mockResolvedValue({ installed: false, version: null, capabilities: [] });
    expect((await probeCcm("h3")).state).toBe("not-installed");
    expect((await probeCcm("h3")).state).toBe("not-installed");
    expect(
      probeMock,
      "「没装」是一个**真的答案**，它该被缓存 —— 每点一次 resume 多一次 ssh 往返是本模块头注逐字要避的。",
    ).toHaveBeenCalledTimes(1);
  });

  it("★ 装了 ⇒ 版本与能力集原样带出来（抽取器自检：上面几条不是在空转）", async () => {
    probeMock.mockResolvedValue({
      installed: true,
      version: "2026.09",
      capabilities: ["account", "tmux"],
    });
    const r = await probeCcm("h4");
    expect(r.state).toBe("installed");
    expect(r.state === "installed" && r.version).toBe("2026.09");
    expect(r.state === "installed" && r.capabilities.has("tmux")).toBe(true);
  });
});
