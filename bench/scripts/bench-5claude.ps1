param([string]$exe, [string]$label)
# 5 panes running the real claude (temp config) via the single-instance handoff; then idle.
$S = if ($env:KILNO_BENCH_DIR) { $env:KILNO_BENCH_DIR } else { Join-Path $env:TEMP "kilno-bench" }
foreach ($x in "run\work","run\claude","run\codex","run\lad\Kilno") { New-Item -ItemType Directory -Force (Join-Path $S $x) | Out-Null }
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Q {
  [StructLayout(LayoutKind.Sequential)] public struct PMC { public uint cb, PageFaultCount; public UIntPtr Peak, WS, QPP, QP, QPNP, QNP, Page, PeakPage, Private, PrivateWS; public ulong Shared; }
  [DllImport("psapi.dll")] public static extern bool GetProcessMemoryInfo(IntPtr h, ref PMC c, uint cb);
}
"@
function PrivWS($id) { $pr = Get-Process -Id $id -ErrorAction SilentlyContinue; if (-not $pr) { return 0 }; $c = New-Object Q+PMC; $c.cb = [uint32][Runtime.InteropServices.Marshal]::SizeOf($c); [Q]::GetProcessMemoryInfo($pr.Handle, [ref]$c, $c.cb) | Out-Null; $c.PrivateWS.ToUInt64()/1MB }
function Kids($id) { Get-CimInstance Win32_Process -Filter "ParentProcessId=$id" | ForEach-Object { $_; Kids $_.ProcessId } }
$work = "$S\run\work"
$env:CLAUDE_CONFIG_DIR = "$S\run\claude"; $env:LOCALAPPDATA = "$S\run\lad"; $env:CODEX_HOME = "$S\run\codex"
Remove-Item Env:KILNO_BENCH_AGENT_CMD, Env:KILNO_BENCH_TYPE -ErrorAction SilentlyContinue
$p = Start-Process -FilePath $exe -ArgumentList "`"$work`"" -WorkingDirectory $work -PassThru
Start-Sleep -Seconds 8
for ($i = 1; $i -lt 5; $i++) { Start-Process -FilePath $exe -ArgumentList "`"$work`"" -WorkingDirectory $work | Out-Null; Start-Sleep -Seconds 4 }
Start-Sleep -Seconds 30
$kids = Kids $p.Id
$claude = @($kids | Where-Object Name -eq 'claude.exe').Count
$wv = $kids | Where-Object Name -eq 'msedgewebview2.exe'
$wvMB = ($wv | ForEach-Object { PrivWS $_.ProcessId } | Measure-Object -Sum).Sum
$cpu0 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalMilliseconds; Start-Sleep -Seconds 10; $cpu1 = (Get-Process -Id $p.Id).TotalProcessorTime.TotalMilliseconds
"[{0}] claude panes {1}; kilno private WS {2:N1} MB; WebView2 helpers {3} = {4:N1} MB; kilno CPU over 10 s idle {5:N0} ms" -f $label, $claude, (PrivWS $p.Id), @($wv).Count, $wvMB, ($cpu1-$cpu0)
taskkill /T /F /PID $p.Id 2>$null | Out-Null
Start-Sleep -Seconds 2
