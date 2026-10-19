$ErrorActionPreference = "Stop"
if ($env:GITHUB_ACTIONS -ne "true" -or !$env:RUNNER_TEMP) {
  throw "This smoke check must run on a disposable GitHub Actions Windows runner."
}

$installer = @(Get-ChildItem "src-tauri/target/release/bundle/nsis/*-setup.exe")
if ($installer.Count -ne 1) { throw "Expected exactly one NSIS installer." }
$installation = Join-Path $env:RUNNER_TEMP "Video Stitch 安装测试"
$privateRoot = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.videostitch.desktop"
if ((Test-Path $installation) -or (Test-Path $privateRoot)) {
  throw "Smoke check requires empty installation and application-data locations."
}
$runtime = Join-Path $privateRoot "Runtime-v2"
$legacy = Join-Path $privateRoot "WebView"
$exports = Join-Path $env:RUNNER_TEMP "Video Stitch 导出测试"
New-Item -ItemType Directory -Path $legacy, $exports | Out-Null
$sentinels = @(
  (Join-Path $privateRoot "成果.mp4"),
  (Join-Path $legacy "保留.mp4"),
  (Join-Path $exports "成果.mp4"),
  (Join-Path $exports "unfinished-tasks.smoke.json")
)
foreach ($path in $sentinels) { [IO.File]::WriteAllText($path, "preserve-export-data") }

# 静默安装到含中文和空格的路径，验证实际随包组件可直接运行。
function Install-SmokeApp {
  $process = Start-Process -FilePath $installer[0].FullName -ArgumentList "/S /D=$installation" -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
  foreach ($name in @("video-stitch.exe", "ffmpeg.exe", "ffprobe.exe")) {
    if (!(Test-Path (Join-Path $installation $name))) { throw "Missing installed $name" }
  }
  foreach ($name in @("ffmpeg.exe", "ffprobe.exe")) {
    & (Join-Path $installation $name) -version
    if ($LASTEXITCODE -ne 0) { throw "Installed $name cannot run." }
  }
}

# _?= 禁止卸载器另起临时副本，确保等待到 hook 真正完成。
function Uninstall-SmokeApp {
  $uninstaller = @(Get-ChildItem $installation -Filter "*uninstall*.exe")
  if ($uninstaller.Count -ne 1) { throw "Expected one installed uninstaller." }
  $process = Start-Process -FilePath $uninstaller[0].FullName -ArgumentList "/S _?=$installation" -Wait -PassThru
  if ($process.ExitCode -ne 0) { throw "Uninstaller failed: $($process.ExitCode)" }
  if (Test-Path (Join-Path $installation "video-stitch.exe")) { throw "Application was not uninstalled." }
  foreach ($path in $sentinels) {
    if (!(Test-Path $path) -or [IO.File]::ReadAllText($path) -ne "preserve-export-data") {
      throw "Uninstall changed export data: $path"
    }
  }
}

Install-SmokeApp
# 无任务时关闭原生主窗口应经前端 IPC 正常退出，并释放、清理 WebView 数据。
$app = Start-Process -FilePath (Join-Path $installation "video-stitch.exe") -PassThru
try {
  $opened = $false
  for ($attempt = 0; $attempt -lt 45; $attempt++) {
    Start-Sleep -Seconds 1
    $app.Refresh()
    if ($app.HasExited) { throw "Installed application exited before opening its main window." }
    if ($app.MainWindowTitle -like "Video Stitch*视频拼接工作台") {
      $opened = $true
      break
    }
  }
  if (!$opened) { throw "Installed application did not open its expected main window." }
  Start-Sleep -Seconds 3
  if (!$app.CloseMainWindow() -or !$app.WaitForExit(40000)) {
    throw "Installed application did not close normally through the frontend."
  }
  if ($app.ExitCode -ne 0) { throw "Application exited with code $($app.ExitCode)." }
  if (Test-Path $runtime) { throw "WebView runtime remains after normal exit." }
} finally {
  if (!$app.HasExited) { Stop-Process -Id $app.Id -Force }
}
New-Item -ItemType Directory -Path $runtime | Out-Null
[IO.File]::WriteAllText((Join-Path $runtime "unknown.mp4"), "unknown-owner")
Uninstall-SmokeApp
if ([IO.File]::ReadAllText((Join-Path $runtime "unknown.mp4")) -ne "unknown-owner") {
  throw "Uninstall changed an unowned runtime directory."
}

Install-SmokeApp
[IO.File]::WriteAllText((Join-Path $runtime ".video-stitch-owner"), "video-stitch-runtime-v2")
Uninstall-SmokeApp
if (Test-Path $runtime) { throw "Owned runtime directory remains after uninstall." }
Write-Output "NSIS install/uninstall and export-data preservation checks passed."
