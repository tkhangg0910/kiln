param([string]$exe, [string]$label, [int]$panes = 5)
# 1) A 10 MB `type` in one pane while the UI thread is probed every 50 ms (WM_NULL with timeout).
# 2) $panes panes (via the single-instance handoff), each having printed the 10 MB file (full
#    scrollback), then idle: private working set of the kilno process.
$S = if ($env:KILNO_BENCH_DIR) { $env:KILNO_BENCH_DIR } else { Join-Path $env:TEMP "kilno-bench" }
foreach ($x in "run\work","run\claude","run\codex","run\lad\Kilno") { New-Item -ItemType Directory -Force (Join-Path $S $x) | Out-Null }
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P {
  [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
  [StructLayout(LayoutKind.Sequential)] public struct PMC { public uint cb, PageFaultCount; public UIntPtr Peak, WS, QPP, QP, QPNP, QNP, Page, PeakPage, Private, PrivateWS; public ulong Shared; }
  [DllImport("psapi.dll")] public static extern bool GetProcessMemoryInfo(IntPtr h, ref PMC c, uint cb);
}
"@
function PrivWS($proc) { $c = New-Object P+PMC; $c.cb = [uint32][Runtime.InteropServices.Marshal]::SizeOf($c); [P]::GetProcessMemoryInfo($proc.Handle, [ref]$c, $c.cb) | Out-Null; [math]::Round($c.PrivateWS.ToUInt64()/1MB,1) }
$work = "$S\run\work"
$env:CLAUDE_CONFIG_DIR = "$S\run\claude"; $env:LOCALAPPDATA = "$S\run\lad"; $env:CODEX_HOME = "$S\run\codex"
$env:KILNO_BENCH_AGENT_CMD = "cmd /c type `"$S\big10mb.txt`""
Remove-Item Env:KILNO_BENCH_TYPE -ErrorAction SilentlyContinue

$p = Start-Process -FilePath $exe -ArgumentList "`"$work`"" -WorkingDirectory $work -PassThru
$t = [Diagnostics.Stopwatch]::StartNew()
while ($p.MainWindowHandle -eq 0 -and $t.ElapsedMilliseconds -lt 30000) { Start-Sleep -Milliseconds 20; $p.Refresh() }
$h = $p.MainWindowHandle
$worst = 0; $hung = 0; $n = 0; $r = [IntPtr]::Zero
$t.Restart()
while ($t.ElapsedMilliseconds -lt 25000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $ok = [P]::SendMessageTimeout($h, 0, [IntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$r)
  $ms = $sw.Elapsed.TotalMilliseconds; $n++
  if ($ok -eq [IntPtr]::Zero) { $hung++ }
  if ($ms -gt $worst) { $worst = $ms }
  Start-Sleep -Milliseconds 50
}
"[$label] 10 MB type: $n UI probes over 25 s, worst reply {0:N1} ms, timeouts(>1 s) $hung" -f $worst
for ($i = 1; $i -lt $panes; $i++) { Start-Process -FilePath $exe -ArgumentList "`"$work`"" -WorkingDirectory $work | Out-Null; Start-Sleep -Seconds 2 }
Start-Sleep -Seconds 40
$p.Refresh()
"[$label] $panes panes, full scrollback, idle: private working set $(PrivWS $p) MB"
taskkill /T /F /PID $p.Id | Out-Null
