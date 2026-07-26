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

// 被测 app 产物 exe（KVM_cc build 出的 debug monitor.exe）。可用 APP_EXE 覆盖。
const APP =
  process.env.APP_EXE ||
  "C:/Users/vm260726/cc-monitor/src-tauri/target/debug/monitor.exe";

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
