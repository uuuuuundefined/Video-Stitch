import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { rustEnvironment } from "./rust.mjs";

// 优先验收将随包分发的组件，不把本机另一个 FFmpeg 的结果冒充包内验收。
const root = resolve(import.meta.dirname, "..");
const target =
  process.platform === "darwin" && process.arch === "arm64"
    ? "aarch64-apple-darwin"
    : process.platform === "win32" && process.arch === "x64"
      ? "x86_64-pc-windows-msvc"
      : null;
if (!target) throw new Error("Unsupported media test host");
const env = { ...rustEnvironment() };
for (const name of ["ffmpeg", "ffprobe"]) {
  const path = resolve(
    root,
    `src-tauri/binaries/${name}-${target}${process.platform === "win32" ? ".exe" : ""}`,
  );
  if (!existsSync(path))
    throw new Error(
      `Missing bundled ${name}; stage the target media components first`,
    );
  env[`VIDEO_STITCH_${name.toUpperCase()}`] = path;
}
const child = spawn(
  "cargo",
  [
    "test",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--no-default-features",
    "--test",
    "media",
    "--",
    "--ignored",
    "--test-threads=1",
    ...process.argv.slice(2),
  ],
  { env, stdio: "inherit" },
);
child.on("error", (error) => {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
