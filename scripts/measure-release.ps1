# MoeFrame release 资源测量（v1-exe 工单 22）
#
# 只做测量，不改程序行为：每次运行都用独立的临时 APPDATA，不碰用户真实的
# %APPDATA%\MoeFrame\config.toml 与日志，也不触发自启对账（临时配置 autostart=false，
# 且本机 Run 项为空时无动作）。
#
# 三段测量：
#   A 启动 + 静止：冷启动到首张图显示的耗时、静止稳态工作集、静止平均单核 CPU
#   B 轮播 soak：按 -SlideSecs 连续切图，采样工作集趋势与进程峰值
#   C 一万张图库：只枚举路径的启动表现（可选）
#
# 用法（在仓库根目录）：
#   powershell -NoProfile -File scripts/measure-release.ps1 -GenerateSamples
#   powershell -NoProfile -File scripts/measure-release.ps1 -Gallery D:\pics -SoakMinutes 60
#
# 输出：<OutDir>\measure-release.json 与 measure-release.md（可直接粘进工单）。

[CmdletBinding()]
param(
    [string]$Exe = "target\release\MoeFrame.exe",
    [string]$Gallery = "",
    [switch]$GenerateSamples,
    [int]$SlideSecs = 3,
    [int]$IdleSeconds = 60,
    [int]$FirstImageTimeoutSec = 120,
    [int]$SoakMinutes = 60,
    [switch]$SkipSoak,
    [switch]$WithTenThousand,
    [string]$OutDir = "target\release-measure"
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repo

if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $repo $OutDir }
function Write-Step($text) { Write-Host "== $text" -ForegroundColor Cyan }

$exe = (Resolve-Path $Exe).Path
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }
$outJson = Join-Path $OutDir "measure-release.json"
$outMd = Join-Path $OutDir "measure-release.md"

# ---- 图库准备 ----
if (-not $Gallery) {
    if (-not $GenerateSamples) {
        throw "need -Gallery <dir> or -GenerateSamples"
    }
    $samples = Join-Path $OutDir "samples"
    if (-not (Test-Path $samples)) {
        Write-Step "generate samples"
        & cargo run --release --quiet --locked --example generate_decode_validation_samples -- $samples
        if ($LASTEXITCODE -ne 0) { throw "sample generation failed" }
    }
    $Gallery = $samples
}
$Gallery = (Resolve-Path $Gallery).Path

function New-AppData([string]$name, [string]$folder, [int]$intervalSecs) {
    $dir = Join-Path $OutDir "appdata-$name"
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
New-Item -ItemType Directory -Path $dir | Out-Null
    # 程序只在保存配置时建目录；日志标记要落在 <appdata>\MoeFrame 下，这里预先建好
    New-Item -ItemType Directory -Path (Join-Path $dir "MoeFrame") -Force | Out-Null
    $folderToml = $folder.Replace("\", "/")
    $config = @"
# measure-release.ps1 生成的临时配置
recursive = false
image_folder = "$folderToml"

[window]
width = 480
height = 320

[playback]
mode = "sequential"
interval_secs = $intervalSecs
pause_on_hover = false
animation = "none"

[display]
show_filename = false

[general]
autostart = false
"@
    Set-Content -Path (Join-Path $dir "MoeFrame\config.toml") -Value $config -Encoding UTF8
    return $dir
}

function Start-App([string]$appdata) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = Split-Path $exe
    $psi.EnvironmentVariables["APPDATA"] = $appdata
    $proc = [System.Diagnostics.Process]::Start($psi)
    return $proc
}

function Get-LogPath([string]$appdata) { Join-Path $appdata "MoeFrame\moe_frame.log" }

function Wait-FirstImage([string]$log, $proc, [int]$timeoutSec = 30) {
    $deadline = (Get-Date).AddSeconds($timeoutSec)
    while ((Get-Date) -lt $deadline) {
        if ($proc.HasExited) { throw "exited before first image, exit=$($proc.ExitCode)" }
        if (Test-Path $log) {
            $line = Select-String -Path $log -Pattern "event=first-image" -SimpleMatch -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($line) {
                $text = $line.Line
                if ($text -match "unix_ms=(\d+)") {
                    $markMs = [int64]$Matches[1]
                    $startMs = ([DateTimeOffset]$proc.StartTime).ToUnixTimeMilliseconds()
                    return ($markMs - $startMs)
                }
            }
        }
        Start-Sleep -Milliseconds 50
    }
    throw "timed out waiting for first-image marker (${timeoutSec}s)"
}

function Sample-Process($proc, [int]$seconds, [int]$stepMs = 1000) {
    $rows = New-Object System.Collections.Generic.List[object]
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        if ($proc.HasExited) { break }
        try { $proc.Refresh() } catch { break }
        $rows.Add([pscustomobject]@{
            t_ms      = [int]((Get-Date) - $script:runStart).TotalMilliseconds
            ws_mb     = [math]::Round($proc.WorkingSet64 / 1MB, 2)
            private_mb= [math]::Round($proc.PrivateMemorySize64 / 1MB, 2)
            cpu_s     = [math]::Round($proc.TotalProcessorTime.TotalSeconds, 3)
        })
        Start-Sleep -Milliseconds $stepMs
    }
    return $rows
}

function Stop-App($proc) {
    if ($proc -and -not $proc.HasExited) {
        $proc.CloseMainWindow() | Out-Null
        if (-not $proc.WaitForExit(3000)) { $proc.Kill() }
    }
}

function Stats($values) {
    $arr = @($values)
    if ($arr.Count -eq 0) { return $null }
    $sorted = $arr | Sort-Object
    [pscustomobject]@{
        min  = $sorted[0]
        mean = [math]::Round(($arr | Measure-Object -Average).Average, 2)
        max  = $sorted[$sorted.Count - 1]
    }
}

function Measure-IdleRun([string]$folder, [int]$intervalSecs, [string]$label) {
    Write-Step "startup + idle: $label"
    $appdata = New-AppData $label $folder $intervalSecs
    $log = Get-LogPath $appdata
    $script:runStart = Get-Date
    $proc = Start-App $appdata
    try {
        $startupMs = Wait-FirstImage $log $proc $FirstImageTimeoutSec
        # 等预加载与首帧绘制稳定，再进入静止采样
        Start-Sleep -Seconds 5
        $settle = Sample-Process $proc 30 1000
        $steady = Stats ($settle | Select-Object -Skip 10 | ForEach-Object { $_.ws_mb })
        $cpuDelta = $settle[-1].cpu_s - $settle[0].cpu_s
        $elapsed = ($settle[-1].t_ms - $settle[0].t_ms) / 1000.0
        $idleCpu = if ($elapsed -gt 0) { [math]::Round($cpuDelta / $elapsed * 100, 2) } else { $null }
        return [pscustomobject]@{
            label       = $label
            startup_ms  = $startupMs
            idle_cpu_pct= $idleCpu
            steady_ws_mb= $steady
            peak_ws_mb  = [math]::Round($proc.PeakWorkingSet64 / 1MB, 2)
        }
    } finally {
        Stop-App $proc
    }
}

$results = [ordered]@{}
$results.exe = $exe
$results.exe_mb = [math]::Round((Get-Item $exe).Length / 1MB, 2)
$results.os = (Get-CimInstance Win32_OperatingSystem).Caption + " " + (Get-CimInstance Win32_OperatingSystem).Version
$results.cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
$results.cores = (Get-CimInstance Win32_Processor | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum
$results.memory_gb = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1)
$results.gallery = $Gallery
Add-Type -AssemblyName System.Windows.Forms -ErrorAction SilentlyContinue
$screen = [System.Windows.Forms.Screen]::PrimaryScreen
if (-not ("W.Dpi" -as [type])) {
    Add-Type -Namespace W -Name Dpi -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetDpiForSystem();' | Out-Null
}
$dpi = [W.Dpi]::GetDpiForSystem()
$results.display = "$($screen.Bounds.Width)x$($screen.Bounds.Height) 主屏；系统 DPI ${dpi}（$([math]::Round($dpi / 96 * 100))%）"
$results.started_at = (Get-Date).ToString("s")

# ---- A：启动 + 静止（间隔取最大值，保证采样窗口内不切图）----
$results.idle = Measure-IdleRun $Gallery 99999999 "idle"

# ---- B：轮播 soak ----
if (-not $SkipSoak) {
    Write-Step "soak: $SoakMinutes min, slide every ${SlideSecs}s"
    $appdata = New-AppData "soak" $Gallery $SlideSecs
    $log = Get-LogPath $appdata
    $script:runStart = Get-Date
    $proc = Start-App $appdata
    try {
        $startupMs = Wait-FirstImage $log $proc $FirstImageTimeoutSec
        $rows = Sample-Process $proc ($SoakMinutes * 60) 1000
        $ws = @($rows | ForEach-Object { $_.ws_mb })
        $half = [int]($ws.Count / 2)
        $results.soak = [pscustomobject]@{
            minutes      = $SoakMinutes
            slide_secs   = $SlideSecs
            startup_ms   = $startupMs
            samples      = $ws.Count
            ws_first_mb  = if ($ws.Count) { $ws[0] } else { $null }
            ws_last_mb   = if ($ws.Count) { $ws[-1] } else { $null }
            ws_min_mb    = (Stats $ws).min
            ws_max_mb    = (Stats $ws).max
            ws_mean_mb   = (Stats $ws).mean
            ws_first_half_mean = [math]::Round((($ws[0..($half - 1)]) | Measure-Object -Average).Average, 2)
            ws_second_half_mean= [math]::Round((($ws[$half..($ws.Count - 1)]) | Measure-Object -Average).Average, 2)
            peak_ws_mb   = [math]::Round($proc.PeakWorkingSet64 / 1MB, 2)
        }
        $rows | Export-Csv -Path (Join-Path $OutDir "soak-samples.csv") -NoTypeInformation -Encoding UTF8
    } finally {
        Stop-App $proc
    }
}

# ---- C：一万张图库 ----
if ($WithTenThousand) {
    Write-Step "10k gallery: enumerate only"
    $big = Join-Path $OutDir "gallery-10000"
    if (-not (Test-Path $big)) {
        New-Item -ItemType Directory -Path $big | Out-Null
# 取最小的一张做种子：一万份拷贝的体积要可控，且本段只测路径枚举与启动。
        $seed = Get-ChildItem -Path $Gallery -File -Recurse | Sort-Object Length | Select-Object -First 1
        if (-not $seed) { throw "no usable image to seed the 10k gallery" }
        for ($i = 0; $i -lt 10000; $i++) {
            $name = "copy-{0:D5}$($seed.Extension)" -f $i
            $dest = Join-Path $big $name
            # 硬链接省空间，但 exFAT/部分网络盘不支持；不支持时退回拷贝，不能因为链接失败中断测量。
            try { New-Item -ItemType HardLink -Path $dest -Target $seed.FullName -ErrorAction Stop | Out-Null } catch { }
            if (-not (Test-Path $dest)) { Copy-Item $seed.FullName $dest }
        }
    }
    $results.ten_thousand = Measure-IdleRun $big 99999999 "10k"
}

$results | ConvertTo-Json -Depth 6 | Set-Content -Path $outJson -Encoding UTF8

# ---- 汇总 ----
$md = New-Object System.Collections.Generic.List[string]
$md.Add("# MoeFrame release 资源测量")
$md.Add("")
$md.Add("- 时间：$($results.started_at)")
$md.Add("- 构建：$($results.exe)（$($results.exe_mb) MB）")
$md.Add("- 系统：$($results.os)；CPU：$($results.cpu)（$($results.cores) 逻辑核）；内存：$($results.memory_gb) GB")
$md.Add("- 显示器：$($results.display)")
$md.Add("- 图库：$($results.gallery)")
$md.Add("")
$md.Add("| 项 | 值 | 口径 |")
$md.Add("|---|---|---|")
$md.Add("| 体积 | $($results.exe_mb) MB | 交付的 release exe（strip + LTO） |")
$md.Add("| 冷启动到首张图 | $($results.idle.startup_ms) ms | 进程启动 → `event=first-image` 标记（含扫描与首张解码） |")
$md.Add("| 静止稳态工作集 | $($results.idle.steady_ws_mb.mean) MB（$($results.idle.steady_ws_mb.min)–$($results.idle.steady_ws_mb.max)） | 首张图后 5s 起、30s 窗口的中段均值 |")
$md.Add("| 静止平均单核 CPU | $($results.idle.idle_cpu_pct)% | 同 30s 窗口内 CPU 时间增量 / 墙钟 |")
if ($results.soak) {
    $s = $results.soak
    $md.Add("| 轮播 ${($s.minutes)} 分钟工作集 | 首 $($s.ws_first_mb) MB / 末 $($s.ws_last_mb) MB，峰值 $($s.peak_ws_mb) MB | 每秒采样，$($s.samples) 个样本 |")
    $md.Add("| 前后半程均值 | $($s.ws_first_half_mean) MB → $($s.ws_second_half_mean) MB | 判断是否持续增长 |")
}
if ($results.ten_thousand) {
    $t = $results.ten_thousand
    $md.Add("| 一万张图库启动 | $($t.startup_ms) ms，稳态 $($t.steady_ws_mb.mean) MB | 同上口径 |")
}
$md | Set-Content -Path $outMd -Encoding UTF8

Write-Host ""
Get-Content $outMd | Write-Host
Write-Host "`nJSON: $outJson"