import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  createReadStream,
  existsSync,
  readFileSync,
  readdirSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";

// 只发布当前构建的完整附件；重试可恢复同一提交的草稿，拒绝覆盖既有正式版本。
const tag = process.env.RELEASE_TAG;
const sha = process.env.RELEASE_SOURCE_SHA || process.env.GITHUB_SHA;
if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "") || !/^[a-f0-9]{40}$/.test(sha ?? ""))
  throw new Error("A stable release tag and exact source commit are required");
const releases = JSON.parse(
  execFileSync(
    "gh",
    ["api", "--paginate", "--slurp", "repos/{owner}/{repo}/releases"],
    {
      encoding: "utf8",
    },
  ),
).flat();
const existing = releases.find((release) => release.tag_name === tag);
if (existing && (!existing.draft || existing.target_commitish !== sha))
  throw new Error("Release already exists or belongs to a different commit");
const refs = JSON.parse(
  execFileSync(
    "gh",
    ["api", `repos/{owner}/{repo}/git/matching-refs/tags/${tag}`],
    {
      encoding: "utf8",
    },
  ),
);
const ref = refs.find((item) => item.ref === `refs/tags/${tag}`);
if (ref && (ref.object.type !== "commit" || ref.object.sha !== sha))
  throw new Error("Existing release tag does not match the build commit");
let assets = readdirSync("release-assets");
if (
  assets.filter((name) => name.endsWith("-setup.exe")).length !== 1 ||
  assets.filter((name) => name.endsWith(".dmg")).length !== 1 ||
  !(
    assets.length === 2 ||
    (assets.length === 3 && assets.includes("SHA256SUMS.txt"))
  )
)
  throw new Error(
    "Expected one Windows installer, one macOS image and optional checksums",
  );
// GitHub 会把附件名的空格改为点号；上传前统一命名并同步校验文件。
for (const name of assets) {
  const canonical = name.replaceAll(" ", ".");
  if (canonical !== name) {
    if (existsSync(`release-assets/${canonical}`))
      throw new Error("Release asset names collide after normalization");
    renameSync(`release-assets/${name}`, `release-assets/${canonical}`);
  }
}
assets = readdirSync("release-assets").sort();
const checksums = [];
for (const name of assets.filter((name) => name !== "SHA256SUMS.txt")) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(`release-assets/${name}`))
    hash.update(chunk);
  checksums.push(`${hash.digest("hex")}  ${name}\n`);
}
writeFileSync("release-assets/SHA256SUMS.txt", checksums.join(""));
assets = readdirSync("release-assets").sort();
const notes = readFileSync("docs/RELEASE_NOTES.md", "utf8");
if (!notes.startsWith(`# Video Stitch ${tag}\n`))
  throw new Error("Release notes do not match the package version");
if (!existing)
  execFileSync(
    "gh",
    [
      "release",
      "create",
      tag,
      "--target",
      sha,
      "--title",
      `Video Stitch ${tag}`,
      "--draft",
      "--notes-file",
      "docs/RELEASE_NOTES.md",
    ],
    { stdio: "inherit" },
  );
execFileSync(
  "gh",
  [
    "release",
    "upload",
    tag,
    ...assets.map((name) => `release-assets/${name}`),
    "--clobber",
  ],
  { stdio: "inherit" },
);
const uploaded = JSON.parse(
  execFileSync("gh", ["release", "view", tag, "--json", "assets"], {
    encoding: "utf8",
  }),
).assets;
if (
  uploaded.length !== assets.length ||
  assets.some(
    (name) =>
      !uploaded.some(
        (asset) =>
          asset.name === name &&
          asset.size === statSync(`release-assets/${name}`).size,
      ),
  )
)
  throw new Error(
    "Uploaded release assets are incomplete; keep release as draft",
  );
execFileSync("gh", ["release", "edit", tag, "--draft=false", "--latest"], {
  stdio: "inherit",
});
