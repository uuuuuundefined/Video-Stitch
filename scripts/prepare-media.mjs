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
import { mediaTarget, validateMediaBinary } from "./media-components.mjs";

// 分离本机开发组件与按校验和提供的发布组件，禁止把 Homebrew 动态依赖冒充独立包。
const args = process.argv.slice(2);
const option = (key) => args[args.indexOf(key) + 1];
const dev = args.includes("--dev");
const target = mediaTarget(
  args.includes("--target") ? option("--target") : null,
);
if (target !== mediaTarget())
  throw new Error(
    "Stage media on the matching host so executable versions can be verified",
  );
const root = resolve(import.meta.dirname, "..");
const manifest = { target, developmentOnly: dev, binaries: {} };
const staged = [];
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
  const bytes = readFileSync(source);
  validateMediaBinary(bytes, target);
  const digest = createHash("sha256").update(bytes).digest("hex");
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
  staged.push({ source, destination });
  manifest.binaries[name] = {
    sha256: digest,
    version: execFileSync(source, ["-version"], { encoding: "utf8" })
      .split("\n")
      .slice(0, 3)
      .join("\n"),
  };
}
// 两个组件全部校验通过后才更新暂存区及配套 manifest。
mkdirSync(resolve(root, "src-tauri/binaries"), { recursive: true });
mkdirSync(resolve(root, "src-tauri/resources/media"), { recursive: true });
for (const { source, destination } of staged) {
  if (resolve(source) !== destination) copyFileSync(source, destination);
  chmodSync(destination, 0o755);
}
writeFileSync(
  resolve(root, "src-tauri/binaries/manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
);
writeFileSync(
  resolve(root, "src-tauri/resources/media/manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
);
process.stdout.write(
  `Staged ${target} media components (${dev ? "development only" : "verified artifacts"}).\n`,
);
