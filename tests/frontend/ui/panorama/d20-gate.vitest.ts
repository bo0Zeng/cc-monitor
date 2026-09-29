// F69（补 D20）：全景图「默认关、每仓手动开启」的门决策单测。钉死开面板时——从未索引就
// 走显式启用手势、不自动扫描（D20 违规回归防线）；已索引则直接加载。
import { describe, it, expect, vi } from "vitest";

// api.ts 顶部 import 了 invoke（panoramaLoadDecision 不用它，但导入模块会解析它）→ mock 掉。
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { panoramaLoadDecision } from "../../../../src/frontend/ui/panorama/api";

describe("F69 panoramaLoadDecision（D20 默认关门决策）", () => {
  it("symbols===0（从未索引 = 未启用）→ enable-gate（显式手势才扫描，绝不自动扫）", () => {
    expect(panoramaLoadDecision({ symbols: 0, indexedAt: null })).toBe("enable-gate");
  });
  it("symbols>0（已索引 = 用户此前已启用）→ load（直接加载现有 overview）", () => {
    expect(panoramaLoadDecision({ symbols: 1, indexedAt: 1 })).toBe("load");
    expect(panoramaLoadDecision({ symbols: 42000, indexedAt: 1790000000 })).toBe("load");
  });
  it("〔RM1f〕建索引被撤掉留下的半份（有符号、从没建完：indexedAt===null）→ enable-gate，不自动重跑", () => {
    // 形状取自真后端 × 真小程序现打：撤在 1.5 s 时 `{ symbols: 5622, indexedAt: null, stale: true }`。
    expect(panoramaLoadDecision({ symbols: 5622, indexedAt: null })).toBe("enable-gate");
  });
});
