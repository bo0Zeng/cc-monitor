/**
 * Batch13-F40b(D 审计发现 1):removeEmptyToolGroupShell 单测。
 * fallback 孤儿单元是组内实体(timeline entry = 组 root)——单元被摘后组壳若空,
 * 必须连根摘壳并返回 root 供出账;组内还有其他单元则不动。
 */
import { describe, expect, it } from "vitest";
import { removeEmptyToolGroupShell } from "./cards";

function mkGroup(unitCount: number): { root: HTMLElement; body: HTMLElement } {
  const root = document.createElement("details");
  root.className = "card card-tool-group";
  const summary = document.createElement("summary");
  root.appendChild(summary);
  const body = document.createElement("div");
  body.className = "card-tool-group-body";
  root.appendChild(body);
  for (let i = 0; i < unitCount; i++) {
    body.appendChild(document.createElement("div"));
  }
  document.body.appendChild(root);
  return { root, body };
}

describe("removeEmptyToolGroupShell", () => {
  it("组 body 已空 → 摘壳并返回 root(供 timeline.removeByElement 出账)", () => {
    const { root, body } = mkGroup(1);
    body.firstElementChild!.remove(); // 模拟 reconcile 摘走唯一 fallback 单元
    const shell = removeEmptyToolGroupShell(root);
    expect(shell).toBe(root);
    expect(root.isConnected).toBe(false);
  });

  it("组内还有其他单元 → 不摘壳,返回 null", () => {
    const { root, body } = mkGroup(2);
    body.firstElementChild!.remove();
    expect(removeEmptyToolGroupShell(root)).toBeNull();
    expect(root.isConnected).toBe(true);
  });

  it("host 为 null(单元不在任何组内,防御)→ null", () => {
    expect(removeEmptyToolGroupShell(null)).toBeNull();
  });
});
