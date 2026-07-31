/**
 * S7 收尾（Phase G 核账逮出来的）：**诊断分节要给「待生效」那条常驻条供货**。
 *
 * 病灶：后端对「启用 log 文件」这一项明确回 `RestartHint = "needs_restart"`，
 * 而前端此前只弹一个 6 秒 toast。S7 立的规矩是「有改动没生效」是**状态**、
 * 唯一去处是底部那条常驻条 —— 漏了这个供给方，用户切完 log 开关、错过 toast，
 * 再看条子是空的，会读成「没有待生效的改动」。
 * **条子的存在本身让它显得权威**，所以漏供比没有条子更误导。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { setDiag, restartHint } = vi.hoisted(() => ({
  setDiag: vi.fn(),
  restartHint: { value: "none" as "none" | "needs_restart" },
}));

vi.mock("../ipc/commands", () => ({
  commands: {
    set_diagnostics_config: (...a: unknown[]) => {
      setDiag(...a);
      return Promise.resolve(restartHint.value);
    },
    get_diagnostics_config: () =>
      Promise.resolve({
        log_enabled: true,
        log_level: "info",
        error_toast: true,
        max_files: 3,
      }),
    get_log_file_info: () => Promise.resolve({ path: "/tmp/x.log", size: 0 }),
  },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

import { DiagnosticsSection } from "./diagnostics-section";
import {
  restartReasons,
  __resetRestartNoticeForTests,
} from "./restart-notice";

/** 切一下「启用 log 文件」复选框，等异步 save 落地。 */
async function toggleLogEnabled(): Promise<void> {
  const sec = new DiagnosticsSection();
  await Promise.resolve();
  const cb = sec.element.querySelector<HTMLInputElement>(
    'input[type="checkbox"]',
  )!;
  cb.checked = !cb.checked;
  cb.dispatchEvent(new Event("change"));
  // save() → invoke → then；两个微任务轮足够
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("诊断分节 → 「需重启」常驻条", () => {
  beforeEach(() => {
    __resetRestartNoticeForTests();
    setDiag.mockClear();
    restartHint.value = "none";
    document.body.replaceChildren();
  });

  it("★ 后端说 needs_restart → 条子上必须列出这一项（不能只弹一个会消失的 toast）", async () => {
    restartHint.value = "needs_restart";
    await toggleLogEnabled();
    expect(setDiag, "先确认 save 真的发出去了（否则下面断言是空转）").toHaveBeenCalled();
    expect(restartReasons()).toContain("诊断日志开关");
  });

  it("★ 反向自检：后端说 none 时**不许**点亮条子（恒亮 = 背景噪音）", async () => {
    restartHint.value = "none";
    await toggleLogEnabled();
    expect(setDiag).toHaveBeenCalled();
    expect(restartReasons()).toEqual([]);
  });
});
