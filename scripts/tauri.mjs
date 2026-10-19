import { spawn } from "node:child_process";
import { rustEnvironment } from "./rust.mjs";

// 统一桌面开发与打包的工具链查找方式。
const child = spawn(
  process.platform === "win32" ? "tauri.cmd" : "tauri",
  process.argv.slice(2),
  {
    env: rustEnvironment(),
    stdio: "inherit",
    shell: process.platform === "win32",
  },
);
child.on("error", (error) => {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
