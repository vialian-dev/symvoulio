# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
param([Parameter(Mandatory=$true)][int]$TargetId)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DesktopActivation {
 [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
'@
$targetWindow = (Get-Process -Id $TargetId).MainWindowHandle
if ($targetWindow -eq [IntPtr]::Zero) { throw "Owned app $TargetId has no main window" }
$shell = New-Object -ComObject WScript.Shell
[void]$shell.AppActivate($TargetId)
Start-Sleep -Milliseconds 200
[uint32]$foregroundId = 0
$foregroundThread = [DesktopActivation]::GetWindowThreadProcessId([DesktopActivation]::GetForegroundWindow(), [ref]$foregroundId)
$callerThread = [DesktopActivation]::GetCurrentThreadId()
$attached = $false
try {
  if ($foregroundId -ne $TargetId -and $foregroundThread -ne 0) {
    $attached = [DesktopActivation]::AttachThreadInput($callerThread, $foregroundThread, $true)
  }
  $focused = [DesktopActivation]::SetForegroundWindow($targetWindow)
} finally {
  if ($attached) { [void][DesktopActivation]::AttachThreadInput($callerThread, $foregroundThread, $false) }
}
Start-Sleep -Milliseconds 200
[uint32]$actualId = 0
[void][DesktopActivation]::GetWindowThreadProcessId([DesktopActivation]::GetForegroundWindow(), [ref]$actualId)
Write-Output "desktop focus target=$TargetId actual=$actualId attached=$attached"
if ($actualId -ne $TargetId) { throw "Owned app did not become the foreground window: target=$TargetId actual=$actualId attached=$attached focused=$focused" }
