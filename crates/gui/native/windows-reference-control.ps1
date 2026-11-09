# Same legacy client/runtime/MTA and actual keys against a stock Microsoft EDIT.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ReferenceInputWindow {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
'@
$uia = [Windows.Automation.AutomationElement]
$root = $uia::RootElement
$scope = [Windows.Automation.TreeScope]::Descendants
Write-Host "Stock reference client: PowerShell=$($PSVersionTable.PSVersion) Runtime=$([Environment]::Version) Apartment=$([Threading.Thread]::CurrentThread.ApartmentState) Bitness=$([IntPtr]::Size * 8)"
Write-Host "Stock reference assembly: $($uia.Assembly.FullName) Location=$($uia.Assembly.Location)"
$core = [Diagnostics.Process]::GetCurrentProcess().Modules | Where-Object ModuleName -eq 'UIAutomationCore.dll' | Select-Object -First 1
Write-Host "Stock reference UIAutomationCore: $($core.FileName) Version=$($core.FileVersionInfo.FileVersion)"
function Wait-Reference([string]$Description, [scriptblock]$Probe) {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $value = & $Probe
        if ($null -ne $value -and $value -ne $false) { return $value }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Stock reference timed out: $Description"
}
function Reference-Keys([string]$Keys) {
    $foregroundPid = [uint32]0
    [ReferenceInputWindow]::GetWindowThreadProcessId([ReferenceInputWindow]::GetForegroundWindow(),[ref]$foregroundPid) | Out-Null
    if ($foregroundPid -ne $referenceProcess.Id) { throw 'Stock reference native keyboard focus is outside its owned process' }
    [Windows.Forms.SendKeys]::SendWait($Keys)
}
$hostScript = Join-Path $PSScriptRoot 'windows-reference-host.ps1'
$referenceProcess = Start-Process -FilePath "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe" -ArgumentList @('-STA','-NoProfile','-File',"`"$hostScript`"") -PassThru
try {
    $condition = [Windows.Automation.PropertyCondition]::new($uia::ProcessIdProperty,$referenceProcess.Id)
    $window = Wait-Reference 'visible owned stock window' {
        foreach ($candidate in $root.FindAll([Windows.Automation.TreeScope]::Children,$condition)) {
            if ($candidate.Current.Name -eq 'Native UIA stock reference' -and -not $candidate.Current.IsOffscreen) { return $candidate }
        }
        return $null
    }
    [ReferenceInputWindow]::SetForegroundWindow([IntPtr]$window.Current.NativeWindowHandle) | Out-Null
    foreach ($child in ($window.FindAll($scope,[Windows.Automation.Condition]::TrueCondition) | Select-Object -First 32)) {
        Write-Host "Stock reference observed provider: type=$($child.Current.ControlType.ProgrammaticName) class=$($child.Current.ClassName) name=$($child.Current.Name) offscreen=$($child.Current.IsOffscreen)"
    }
    # This independent host owns exactly one real WinForms/Win32 EDIT. Its
    # AccessibleName need not be surfaced by the OS's legacy Win32 proxy.
    $editType = [Windows.Automation.PropertyCondition]::new($uia::ControlTypeProperty,[Windows.Automation.ControlType]::Edit)
    $notes = Wait-Reference 'single stock EDIT provider' {
        $edits = $window.FindAll($scope,$editType)
        if ($edits.Count -eq 1 -and -not $edits[0].Current.IsOffscreen) { return $edits[0] }
        return $null
    }
    Write-Host "Stock reference control class=$($notes.Current.ClassName)"
    $notes.SetFocus()
    Wait-Reference 'actual stock keyboard focus' { $notes.Current.HasKeyboardFocus } | Out-Null
    foreach ($character in "first native line`nsecond native line".ToCharArray()) {
        if ($character -eq "`n") { Reference-Keys '{ENTER}' } else { Reference-Keys ([string]$character) }
    }
    $textPattern = $notes.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    Wait-Reference 'stock document exact text' {
        $textPattern.DocumentRange.GetText(-1).Replace("`r",'').TrimEnd("`n") -eq "first native line`nsecond native line"
    } | Out-Null
    Write-Host 'Stock reference document GetText verified'
    $caret = $textPattern.GetSelection()
    Write-Host "Stock reference caret range_count=$($caret.Length)"
    if ($caret.Length -ne 1) { throw 'Stock reference caret did not return one range' }
    $document = $textPattern.DocumentRange
    $startAtEnd = $caret[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,$document,[Windows.Automation.Text.TextPatternRangeEndpoint]::End)
    $endAtEnd = $caret[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::End,$document,[Windows.Automation.Text.TextPatternRangeEndpoint]::End)
    Write-Host "Stock reference caret start_vs_doc_end=$startAtEnd end_vs_doc_end=$endAtEnd"
    if ($startAtEnd -ne 0 -or $endAtEnd -ne 0 -or $caret[0].GetText(-1) -ne '') { throw 'Stock reference caret contract failed' }
    Write-Host 'Stock reference caret endpoints and empty GetText verified'
    Reference-Keys '^a'
    Wait-Reference 'stock full selection exact text and endpoints' {
        $selection = $textPattern.GetSelection()
        if ($selection.Length -ne 1) { return $false }
        $document = $textPattern.DocumentRange
        $start = $selection[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,$document,[Windows.Automation.Text.TextPatternRangeEndpoint]::Start)
        $end = $selection[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::End,$document,[Windows.Automation.Text.TextPatternRangeEndpoint]::End)
        Write-Host "Stock reference selection count=$($selection.Length) start_vs_doc_start=$start end_vs_doc_end=$end"
        $start -eq 0 -and $end -eq 0 -and $selection[0].GetText(-1).Replace("`r",'').TrimEnd("`n") -eq "first native line`nsecond native line"
    } | Out-Null
    Write-Host 'Stock reference native Ctrl+A/endpoints/GetText verified'
} finally {
    if (-not $referenceProcess.HasExited) {
        $referenceProcess.CloseMainWindow() | Out-Null
        if (-not $referenceProcess.WaitForExit(3000)) { $referenceProcess.Kill(); $referenceProcess.WaitForExit(3000) | Out-Null }
    }
}
