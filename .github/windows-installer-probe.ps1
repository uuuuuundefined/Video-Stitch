$ErrorActionPreference = "Stop"
if ($env:GITHUB_ACTIONS -ne "true") { throw "Disposable CI runner required." }
New-Item -ItemType Directory -Path diagnostics | Out-Null
$installer = @(Get-ChildItem "installer/*-setup.exe")
if ($installer.Count -ne 1) { throw "Expected one installer." }
$installation = Join-Path $env:RUNNER_TEMP "Video Stitch 安装诊断"
$runtime = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.videostitch.desktop/Runtime-v2"
$installed = Start-Process $installer[0].FullName -ArgumentList "/S /D=$installation" -Wait -PassThru
if ($installed.ExitCode -ne 0) { throw "Installation failed." }
$app = Start-Process (Join-Path $installation "video-stitch.exe") -PassThru
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Drawing

# 只记录本次安装应用的窗口和运行目录，不导出进程内存或环境变量。
function Show-AppState($stage) {
  $app.Refresh()
  Write-Output "STATE: $stage; pid=$($app.Id); exited=$($app.HasExited)"
  if ($app.HasExited) { return }
  Write-Output "title=$($app.MainWindowTitle); handle=$($app.MainWindowHandle); responding=$($app.Responding); cpu=$($app.TotalProcessorTime)"
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $app.Id)
  $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
    [System.Windows.Automation.TreeScope]::Children, $condition)
  foreach ($window in $windows) {
    Write-Output "Window: $($window.Current.Name); class=$($window.Current.ClassName)"
    $elements = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.Condition]::TrueCondition)
    $elements | Select-Object -First 100 | ForEach-Object { Write-Output "UI: $($_.Current.Name)" }
    if ($stage -eq "before-close" -and $window.Current.Name -like "Video Stitch*视频拼接工作台") {
      $bounds = $window.Current.BoundingRectangle
      if ($bounds.Width -gt 0 -and $bounds.Height -gt 0) {
        $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
          $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
          $bitmap.Save((Join-Path $PWD "diagnostics/startup.png"))
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
      }
    }
  }
  Write-Output "Runtime exists: $(Test-Path $runtime)"
  if (Test-Path $runtime) {
    Get-ChildItem $runtime -Force -Recurse | Select-Object -First 40 FullName, Length | Format-Table -AutoSize
  }
  Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $app.Id } |
    Select-Object ProcessId, ParentProcessId, Name | Format-Table -AutoSize
}

try {
  Start-Sleep -Seconds 8
  Show-AppState "before-close"
  Write-Output "CloseMainWindow result: $($app.CloseMainWindow())"
  foreach ($stage in @("after-5s", "after-10s", "after-15s")) {
    Start-Sleep -Seconds 5
    Show-AppState $stage
    if ($app.HasExited) { break }
  }
  if (!$app.HasExited) { throw "Installed app still running after close." }
} finally {
  if (!$app.HasExited) { Stop-Process -Id $app.Id -Force }
}
