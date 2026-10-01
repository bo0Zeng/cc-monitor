// F-E5 Tier2 —— Windows DOM 冒烟（WebdriverIO 经典 tauri-driver 路径）。
//
// 复用 auto-e2e spike 已验通的配置：child_process spawn tauri-driver
// （`--native-driver <msedgedriver>`），不依赖 @wdio/tauri-service（最稳、文档化）。
//
// 只在 Windows VM 的**交互会话（session 1）**里跑（见 README：schtasks /it hop）；
// session-0 SSH 起不来 WebView2。CI 不跑本套件（无 Windows GUI）。
//
// devDep-only：这些 wdio 包只在 package.json devDependencies，vite 生产构建不含。
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import path from "node:path";

let tauriDriver;

// 被测 app 产物 exe。**必须由 APP_EXE 给**。
//
// 🔴这里原先兜着一个默认值
// `C:/Users/vm260726/cc-monitor/src/frontend/shell/target/debug/monitor.exe` —— **三段全错**：
// 用户名（真机是 `zbl`）· 构建目录（本仓是 `.build/shell/debug/`，不是 `src/frontend/shell/target/`）·
// 以及**那台机器上压根没有这个仓**（现打：没有 git/cargo/node）。
// 而 `run-in-session1.ps1` 里**抄着同一个错值** ⇒ 一个错的默认值有两处住址。
//
// ⚠ 错的默认值比没有默认值更坏：它让这一档先印一句 `APP_EXE exists? False`，
// 然后死在 msedgedriver 那一层（`DevToolsActivePort file doesn't exist`），
// 而那句话指向的是完全另一个原因。⇒ **说不出被测对象在哪就当场停，别猜。**
const APP = process.env.APP_EXE;
if (!APP) {
  throw new Error(
    "APP_EXE 没给 —— 这一档必须被告知被测的 cc-monitor.exe 在哪（不猜默认值）。\n" +
      "  ⚠ 它还要求同目录下有 WebView2Loader.dll：那是**普通导入**不是 delay-load，\n" +
      "    少了它进程会立刻自退，且 stdout/stderr/日志三处全空（2026-09-21 真机现打）。",
  );
}

// `cargo install tauri-driver` 默认落 %USERPROFILE%\.cargo\bin\tauri-driver.exe
const TAURI_DRIVER =
  process.env.TAURI_DRIVER ||
  path.resolve(homedir(), ".cargo", "bin", "tauri-driver.exe");

// msedgedriver（版本需匹配 WebView2 Runtime）。留空则让 tauri-driver 自寻 PATH。
const MSEDGEDRIVER = process.env.MSEDGEDRIVER || "";

export const config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  path: "/",
  specs: ["./test/shell-smoke.spec.mjs"],
  maxInstances: 1,
  capabilities: [
    {
      "tauri:options": { application: APP },
    },
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 120000 },
  logLevel: "info",
  connectionRetryTimeout: 90000,
  connectionRetryCount: 1,
  beforeSession: () => {
    const args = MSEDGEDRIVER ? ["--native-driver", MSEDGEDRIVER] : [];
    tauriDriver = spawn(TAURI_DRIVER, args, {
      stdio: [null, process.stdout, process.stderr],
    });
  },
  afterSession: () => {
    if (tauriDriver) tauriDriver.kill();
  },
};
