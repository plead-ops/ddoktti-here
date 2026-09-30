param(
    [Parameter(Mandatory=$true)][string]$ExePath,
    [ValidateRange(1,10)][int]$Repeat = 3,
    [switch]$TestInput
)
# Use the isolated debug build documented in docs/native-rendering.md.
# -TestInput moves the pointer: run on an otherwise idle test desktop/VM.
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path $ExePath).Path
$logs = Join-Path $env:TEMP ('ddoktti-native-check-' + [guid]::NewGuid())
New-Item -ItemType Directory $logs | Out-Null
if ($TestInput -and -not ('DdokttiTestMouse' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DdokttiTestMouse {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int L,T,R,B; }
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string c,string title);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point p);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 public static IntPtr FindPet(uint pid) {
   var h=FindWindow(null,"\ub611\ub760");uint owner;GetWindowThreadProcessId(h,out owner);
   if(h==IntPtr.Zero || owner!=pid) throw new Exception("Test pet window not found");return h;
 }
 public static void Move(IntPtr h,int dx=0,int dy=0) {
   Rect r;if(!GetWindowRect(h,out r)) throw new Exception("Test pet window disappeared");
   SetCursorPos((r.L+r.R)/2+dx,r.T+(r.B-r.T)*65/100+dy);
 }
 public static void Button(uint flag) { mouse_event(flag,0,0,0,UIntPtr.Zero); }
}
'@
}
function Wait-Stage([string]$Path, [string]$Stage, $Process) {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ((Test-Path $Path) -and (Get-Content $Path -Raw -Encoding UTF8) -match "SMOKE ${Stage}:") { return }
        if ($Process.HasExited) { throw "App exited before stage $Stage; see $Path" }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timeout waiting for $Stage; see $Path"
}
for ($i=1; $i -le $Repeat; $i++) {
    $err = Join-Path $logs "run-$i.stderr.log"
    $out = Join-Path $logs "run-$i.stdout.log"
    $p = Start-Process $exe -ArgumentList '--native-smoke' -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
    $handle = $p.Handle # Keep the process handle so ExitCode remains available after exit.
    if ($TestInput -and $i -eq $Repeat) {
        $dpi = [DdokttiTestMouse]::SetThreadDpiAwarenessContext([IntPtr](-4))
        $cursor = New-Object DdokttiTestMouse+Point
        [DdokttiTestMouse]::GetCursorPos([ref]$cursor) | Out-Null
        try {
            Wait-Stage $err 'run' $p # Sleepy starts immediately after this marker.
            $h = [DdokttiTestMouse]::FindPet($p.Id)
            Start-Sleep -Milliseconds 250
            [DdokttiTestMouse]::Move($h); Start-Sleep -Milliseconds 150
            [DdokttiTestMouse]::Button(2); Start-Sleep -Milliseconds 100; [DdokttiTestMouse]::Button(4)
            Wait-Stage $err 'sleepy' $p
            [DdokttiTestMouse]::Move($h); Start-Sleep -Milliseconds 150
            [DdokttiTestMouse]::Button(8); Start-Sleep -Milliseconds 100; [DdokttiTestMouse]::Button(16)
            Wait-Stage $err 'tickle' $p
            [DdokttiTestMouse]::Move($h); Start-Sleep -Milliseconds 150
            [DdokttiTestMouse]::Button(2); Start-Sleep -Milliseconds 100
            [DdokttiTestMouse]::Move($h,-180,-140)
            Wait-Stage $err 'slack' $p
        } finally {
            [DdokttiTestMouse]::Button(4); [DdokttiTestMouse]::Button(16)
            [DdokttiTestMouse]::SetCursorPos($cursor.X,$cursor.Y) | Out-Null
            [DdokttiTestMouse]::SetThreadDpiAwarenessContext($dpi) | Out-Null
        }
    }
    if (-not $p.WaitForExit(45000)) { throw "App did not exit; see $err" }
    $p.Refresh()
    $log = Get-Content $err -Raw -Encoding UTF8
    if ($p.ExitCode -ne 0 -or $log -notmatch 'SMOKE RESULT passed=true native_window=true webviews=2') { throw "Native check failed; see $err" }
    if ($TestInput -and $i -eq $Repeat) {
        foreach ($expected in @('SMOKE sleepy:.*mode=surprised','SMOKE tickle:.*menu=true','SMOKE slack:.*dragging=true')) {
            if ($log -notmatch $expected) { throw "Input check failed: $expected; see $err" }
        }
    }
    Write-Output "Run ${i}: PASS (exit $($p.ExitCode))"
}
Write-Output "Logs: $logs"
