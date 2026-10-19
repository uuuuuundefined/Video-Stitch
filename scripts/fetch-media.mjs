import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

// 仅开发构建下载固定源码，运行中的桌面应用不联网或创建这些文件。
const directory = resolve(import.meta.dirname, "../.tools/media-source");
mkdirSync(directory, { recursive: true });
const sources = [
  {
    name: "ffmpeg-8.1.2.tar.xz",
    url: "https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz",
    sha256: "464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c",
  },
  {
    name: "x264-b35605a.tar.gz",
    url: "https://code.videolan.org/videolan/x264/-/archive/b35605ace3ddf7c1a5d67a2eb553f034aef41d55/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55.tar.gz",
    sha256: "cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9",
  },
];
for (const source of sources) {
  const path = resolve(directory, source.name);
  const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
  if (existsSync(path) && digest(readFileSync(path)) === source.sha256)
    continue;
  const response = await fetch(source.url, {
    signal: AbortSignal.timeout(120000),
  });
  if (!response.ok) throw new Error(`${source.name}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (digest(bytes) !== source.sha256)
    throw new Error(`Source checksum mismatch: ${source.name}`);
  writeFileSync(path, bytes, { flag: existsSync(path) ? "w" : "wx" });
}
process.stdout.write("Pinned media sources verified.\n");
