import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mediaTarget, validateMediaBinary } from "./media-components.mjs";

// 发布构建拒绝本机开发组件和缺失许可资料，避免生成依赖开发环境的安装包。
const root = resolve(import.meta.dirname, "..");
const manifest = JSON.parse(
  readFileSync(resolve(root, "src-tauri/binaries/manifest.json"), "utf8"),
);
const args = process.argv.slice(2);
const targetIndex = args.indexOf("--target");
const target = mediaTarget(
  targetIndex >= 0
    ? args[targetIndex + 1]
    : process.env.TAURI_ENV_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET,
);
if (manifest.target !== target)
  throw new Error(
    `Media target mismatch: expected ${target}, staged ${manifest.target}. Build and stage the target media components first.`,
  );
if (manifest.developmentOnly)
  throw new Error(
    "Development media components cannot be packaged for release.",
  );
for (const name of ["ffmpeg", "ffprobe"]) {
  const binary = resolve(
    root,
    `src-tauri/binaries/${name}-${manifest.target}${manifest.target.includes("windows") ? ".exe" : ""}`,
  );
  const bytes = readFileSync(binary);
  validateMediaBinary(bytes, target);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== manifest.binaries[name]?.sha256)
    throw new Error(`Unverified ${name} binary`);
  if (target === "aarch64-apple-darwin") {
    const dependencies = execFileSync("/usr/bin/otool", ["-L", binary], {
      encoding: "utf8",
    })
      .split("\n")
      .slice(1)
      .map((line) => line.trim().split(" ")[0])
      .filter(Boolean);
    if (
      dependencies.some(
        (path) =>
          !path.startsWith("/usr/lib/") && !path.startsWith("/System/Library/"),
      )
    )
      throw new Error(`${name} depends on non-system libraries`);
  }
}
const bundled = JSON.parse(
  readFileSync(
    resolve(root, "src-tauri/resources/media/manifest.json"),
    "utf8",
  ),
);
if (JSON.stringify(bundled) !== JSON.stringify(manifest))
  throw new Error("Bundled media manifest differs from binaries");
readFileSync(resolve(root, "src-tauri/resources/media/NOTICE.md"));
if (target === "x86_64-pc-windows-msvc") {
  for (const file of [
    "FFmpeg-COPYING.GPLv2",
    "x264-COPYING",
    "ffmpeg-8.1.2.tar.xz",
    "x264-b35605a.tar.gz",
    "ffmpeg-config.h",
    "build-ffmpeg-windows.sh",
  ])
    readFileSync(resolve(root, "src-tauri/resources/media", file));
}
process.stdout.write("Release media components verified.\n");
