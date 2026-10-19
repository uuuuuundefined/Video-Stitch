import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

// 发布构建拒绝本机开发组件和缺失许可资料，避免生成依赖开发环境的安装包。
const root = resolve(import.meta.dirname, "..");
const manifest = JSON.parse(
  readFileSync(resolve(root, "src-tauri/binaries/manifest.json"), "utf8"),
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
  const digest = createHash("sha256")
    .update(readFileSync(binary))
    .digest("hex");
  if (digest !== manifest.binaries[name]?.sha256)
    throw new Error(`Unverified ${name} binary`);
  if (process.platform === "darwin") {
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
process.stdout.write("Release media components verified.\n");
