// F69（补 D20）：全景图「默认关、每仓手动开启」的门决策单测。钉死开面板时——从未索引就
// 走显式启用手势、不自动扫描（D20 违规回归防线）；已索引则直接加载。
import { describe, it, expect, vi } from "vitest";

// api.ts 顶部 import 了 invoke（panoramaLoadDecision 不用它，但导入模块会解析它）→ mock 掉。
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { panoramaLoadDecision } from "./api";

describe("F69 panoramaLoadDecision（D20 默认关门决策）", () => {
  it("symbols===0（从未索引 = 未启用）→ enable-gate（显式手势才扫描，绝不自动扫）", () => {
    expect(panoramaLoadDecision({ symbols: 0 })).toBe("enable-gate");
  });
  it("symbols>0（已索引 = 用户此前已启用）→ load（直接加载现有 overview）", () => {
    expect(panoramaLoadDecision({ symbols: 1 })).toBe("load");
    expect(panoramaLoadDecision({ symbols: 42000 })).toBe("load");
  });
});
