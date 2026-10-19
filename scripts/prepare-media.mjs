import {
  copyFileSync,
  mkdirSync,
  chmodSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

// 分离本机开发组件与按校验和提供的发布组件，禁止把 Homebrew 动态依赖冒充独立包。
const args = process.argv.slice(2);
const option = (key) => args[args.indexOf(key) + 1];
const dev = args.includes("--dev");
const target = args.includes("--target")
  ? option("--target")
  : process.platform === "darwin" && process.arch === "arm64"
    ? "aarch64-apple-darwin"
    : process.platform === "win32" && process.arch === "x64"
      ? "x86_64-pc-windows-msvc"
      : null;
if (
  !target ||
  !["aarch64-apple-darwin", "x86_64-pc-windows-msvc"].includes(target)
)
  throw new Error("Unsupported target");
const root = resolve(import.meta.dirname, "..");
mkdirSync(resolve(root, "src-tauri/binaries"), { recursive: true });
const manifest = { target, developmentOnly: dev, binaries: {} };
for (const name of ["ffmpeg", "ffprobe"]) {
  const source = args.includes(`--${name}`)
    ? option(`--${name}`)
    : dev
      ? execFileSync(process.platform === "win32" ? "where" : "which", [name], {
          encoding: "utf8",
        })
          .trim()
          .split("\n")[0]
      : null;
  if (!source) throw new Error(`Provide --${name}`);
  const digest = createHash("sha256")
    .update(readFileSync(source))
    .digest("hex");
  if (
    !dev &&
    (!args.includes(`--sha256-${name}`) ||
      option(`--sha256-${name}`) !== digest)
  )
    throw new Error(`Missing or incorrect SHA-256 for ${name}`);
  const destination = resolve(
    root,
    `src-tauri/binaries/${name}-${target}${target.includes("windows") ? ".exe" : ""}`,
  );
  copyFileSync(source, destination);
  chmodSync(destination, 0o755);
  manifest.binaries[name] = {
    sha256: digest,
    version: execFileSync(source, ["-version"], { encoding: "utf8" })
      .split("\n")
      .slice(0, 3)
      .join("\n"),
  };
}
writeFileSync(
  resolve(root, "src-tauri/binaries/manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
);
process.stdout.write(
  `Staged ${target} media components (${dev ? "development only" : "verified artifacts"}).\n`,
);
