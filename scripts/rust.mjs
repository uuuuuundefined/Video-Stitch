import { existsSync } from "node:fs";
import { resolve, delimiter } from "node:path";
import { spawn } from "node:child_process";

// 优先使用项目工具链，不修改用户 PATH 或全局 Rust 配置。
export function rustEnvironment() {
  const root = resolve(import.meta.dirname, "..");
  const local = resolve(root, ".tools/cargo/bin");
  return existsSync(local)
    ? {
        ...process.env,
        CARGO_HOME: resolve(root, ".tools/cargo"),
        RUSTUP_HOME: resolve(root, ".tools/rustup"),
        PATH: local + delimiter + process.env.PATH,
      }
    : process.env;
}

if (process.argv[1] === import.meta.filename) {
  const child = spawn("cargo", process.argv.slice(2), {
    env: rustEnvironment(),
    stdio: "inherit",
  });
  child.on("error", (error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
  child.on("exit", (code) => {
    process.exitCode = code ?? 1;
  });
}
