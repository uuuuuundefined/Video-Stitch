import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";

// 只复用主分支上已通过两平台检查、且产品源码未变化的发布附件。
function api(path) {
  return JSON.parse(
    execFileSync("gh", ["api", `repos/{owner}/{repo}/${path}`], {
      encoding: "utf8",
    }),
  );
}

const version = JSON.parse(readFileSync("package.json", "utf8")).version;
if (!/^\d+\.\d+\.\d+$/.test(version))
  throw new Error("Invalid release version");
const tag = `v${version}`;
const existing = api("releases?per_page=100").find((r) => r.tag_name === tag);
if (!existing?.draft || !/^[a-f0-9]{40}$/.test(existing.target_commitish))
  throw new Error("Expected an existing draft tied to an exact source commit");
const sha = existing.target_commitish;
const requested = process.env.RELEASE_RUN_ID || "";
if (requested && !/^\d+$/.test(requested))
  throw new Error("Invalid workflow run ID");
const candidates = requested
  ? [api(`actions/runs/${requested}`)]
  : api(`actions/workflows/release.yml/runs?head_sha=${sha}&per_page=100`)
      .workflow_runs;
let selected;
for (const run of candidates) {
  if (
    run.head_sha !== sha ||
    run.head_branch !== "main" ||
    run.status !== "completed" ||
    run.path !== ".github/workflows/release.yml"
  )
    continue;
  const jobs = api(`actions/runs/${run.id}/jobs?per_page=100`).jobs;
  if (
    ["macos", "windows / windows"].every((name) =>
      jobs.some((job) => job.name === name && job.conclusion === "success"),
    )
  ) {
    selected = run;
    break;
  }
}
if (!selected)
  throw new Error("No matching run with both platform checks passed");
const artifacts = api(`actions/runs/${selected.id}/artifacts`).artifacts;
if (
  artifacts.length !== 2 ||
  ["video-stitch-macos-arm64", "video-stitch-windows-x64"].some(
    (name) => !artifacts.some((a) => a.name === name && !a.expired),
  )
)
  throw new Error("Expected both unexpired platform artifacts");
const comparison = api(`compare/${sha}...${process.env.GITHUB_SHA}`);
const allowed = new Set([
  "scripts/publish-release.mjs",
  "scripts/resolve-release-run.mjs",
  ".github/workflows/publish-checked-artifacts.yml",
]);
if (
  !["ahead", "identical"].includes(comparison.status) ||
  comparison.files.some((file) => !allowed.has(file.filename))
)
  throw new Error("Product source changed; build fresh installers instead");
appendFileSync(
  process.env.GITHUB_OUTPUT,
  `run_id=${selected.id}\nsource_sha=${sha}\ntag=${tag}\n`,
);
console.log(
  `Verified platform artifacts from run ${selected.id}, source ${sha}`,
);
