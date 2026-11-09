# Production qualification through UI Automation and native Windows keys.
param([Parameter(Mandatory=$true)][string]$Binary,
      [Parameter(Mandatory=$true)][string]$Fixture,
      [Parameter(Mandatory=$true)][string]$Scratch)
$ErrorActionPreference = 'Stop'
# Native GL driver choice belongs only to this disposable qualification process.
$env:GALLIUM_DRIVER = 'llvmpipe'
$env:LP_NUM_THREADS = '2'
$qualificationDeadline = [DateTime]::UtcNow.AddSeconds(180)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeQaWindow {
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd, uint cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, System.Text.StringBuilder text, int maximum);
    private delegate bool EnumWindow(IntPtr hwnd, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    public static IntPtr[] HandlesFor(uint processId) {
        var handles = new System.Collections.Generic.List<IntPtr>();
        EnumWindows((window, parameter) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == processId) handles.Add(window);
            return true;
        }, IntPtr.Zero);
        return handles.ToArray();
    }
    public static string Title(IntPtr hwnd) {
        var title = new System.Text.StringBuilder(2048);
        GetWindowText(hwnd, title, title.Capacity);
        return title.ToString();
    }
}
'@
if (-not [Environment]::UserInteractive) { throw 'Required interactive Windows desktop is unavailable' }
$Binary = (Resolve-Path $Binary).Path
$Fixture = (Resolve-Path $Fixture).Path
$Scratch = [IO.Path]::GetFullPath($Scratch)
[IO.Directory]::CreateDirectory($Scratch) | Out-Null
$env:AUDITORYLAB_LIBRARY_DIR = Join-Path $Scratch 'library'
$env:AUDITORYLAB_NATIVE_QA_DIRECTORY = $Scratch
$root = [System.Windows.Automation.AutomationElement]::RootElement
$scope = [System.Windows.Automation.TreeScope]::Descendants
$uia = [System.Windows.Automation.AutomationElement]
function Wait-Native([string]$Description, [scriptblock]$Probe) {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    if ($deadline -gt $qualificationDeadline) { $deadline = $qualificationDeadline }
    do {
        $value = & $Probe
        if ($null -ne $value -and $value -ne $false) { return $value }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out: $Description"
}
function Find-Named($Parent, [string]$Name) {
    $condition = [System.Windows.Automation.PropertyCondition]::new($uia::NameProperty,$Name)
    foreach ($item in $Parent.FindAll($scope,$condition)) {
        if ($item.Current.IsEnabled -and -not $item.Current.IsOffscreen) { return $item }
    }
    return $null
}
function Invoke-Native($Item) {
    $pattern = $null
    if ($Item.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { $pattern.Invoke(); return }
    if ($Item.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$pattern)) { $pattern.Select(); return }
    throw "OS-provider action unavailable: $($Item.Current.Name)"
}
function Read-NativeText($Item) {
    $pattern = $null
    if ($Item.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$pattern)) { return $pattern.Current.Value }
    if ($Item.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$pattern)) { return $pattern.DocumentRange.GetText(-1).TrimEnd("`r","`n") }
    throw 'Multiline field exposes neither Value nor Text UIA pattern'
}
function Native-Keys([string]$Keys) {
    $foregroundPid = [uint32]0
    [NativeQaWindow]::GetWindowThreadProcessId([NativeQaWindow]::GetForegroundWindow(),[ref]$foregroundPid) | Out-Null
    if ($foregroundPid -ne $process.Id) { throw 'Native keyboard focus is outside the isolated application' }
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
}
function Native-Text([string]$Text) {
    foreach ($character in $Text.ToCharArray()) {
        if ($character -eq "`n") { Native-Keys '{ENTER}' }
        elseif ('+^%~(){}[]'.Contains($character)) { Native-Keys "{$character}" }
        else { Native-Keys ([string]$character) }
    }
}
$process = Start-Process -FilePath $Binary -PassThru -WorkingDirectory $Scratch -RedirectStandardError (Join-Path $Scratch 'application.log')
try {
    $condition = [System.Windows.Automation.PropertyCondition]::new($uia::ProcessIdProperty,$process.Id)
    $window = Wait-Native 'visible production main UIA window' {
        foreach ($candidate in $root.FindAll([System.Windows.Automation.TreeScope]::Children,$condition)) {
            if ($candidate.Current.Name -eq 'Auditory Lab' -and -not $candidate.Current.IsOffscreen -and $candidate.Current.BoundingRectangle.Width -gt 100) { return $candidate }
        }
        return $null
    }
    $browse = Wait-Native 'first-frame Browse Library provider action' { Find-Named $window 'Browse Library' }
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    if ($handle -eq [IntPtr]::Zero) { throw 'Production UIA provider has no native window handle' }
    [NativeQaWindow]::SetForegroundWindow($handle) | Out-Null
    $bounds = $window.Current.BoundingRectangle
    if ($bounds.Width -le 100 -or $bounds.Height -le 100 -or $window.Current.IsOffscreen) { throw 'Native first-frame geometry is unusable' }
    $bitmap = [System.Drawing.Bitmap]::new([int]$bounds.Width,[int]$bounds.Height)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try { $graphics.CopyFromScreen([int]$bounds.X,[int]$bounds.Y,0,0,$bitmap.Size) } finally { $graphics.Dispose() }
        $colors = [Collections.Generic.HashSet[int]]::new()
        for ($x=0; $x -lt $bitmap.Width; $x+=10) { for ($y=0; $y -lt $bitmap.Height; $y+=10) { $colors.Add($bitmap.GetPixel($x,$y).ToArgb()) | Out-Null } }
        if ($colors.Count -lt 5) { $bitmap.Save((Join-Path $Scratch 'failure-first-frame.png')); throw 'No rendered production first-frame pixels' }
    } finally { $bitmap.Dispose() }
    $driver = @($process.Modules | Where-Object ModuleName -eq 'opengl32.dll')
    if ($driver.Count -ne 1) { throw 'Production renderer did not load a discoverable WGL driver' }
    if (Test-Path (Join-Path $Scratch 'driver-provenance.json')) {
        $expectedDriver = Join-Path ([IO.Path]::GetDirectoryName($Binary)) 'opengl32.dll'
        if ($driver[0].FileName -ne $expectedDriver) { throw 'Production renderer did not load the authenticated app-local Mesa driver' }
    }
    Invoke-Native $browse
    foreach ($name in @('Open recipe…','Workbench')) { Wait-Native "operation provider $name" { Find-Named $window $name } | Out-Null }
    Invoke-Native (Find-Named $window 'Workbench')
    Wait-Native 'Workbench New sound provider' { Find-Named $window 'New sound' } | Out-Null
    $notesCondition = [System.Windows.Automation.PropertyCondition]::new($uia::NameProperty,'Custom recipe notes')
    $notes = Wait-Native 'multiline notes UIA provider' { $window.FindFirst($scope,$notesCondition) }
    $title = Wait-Native 'visible title keyboard starting point' { Find-Named $window 'Sound title' }
    $title.SetFocus()
    Wait-Native 'actual title keyboard focus' { $title.Current.HasKeyboardFocus } | Out-Null
    # Use native Tab traversal, whose production focus path scrolls clipped fields.
    # Slint does not advertise a ScrollItem UIA pattern for these fields.
    for ($attempt=0; $attempt -lt 80; $attempt++) {
        Native-Keys '{TAB}'
        Start-Sleep -Milliseconds 50
        if ($notes.Current.HasKeyboardFocus) { break }
    }
    Wait-Native 'actual visible multiline keyboard focus' { $notes.Current.HasKeyboardFocus -and -not $notes.Current.IsOffscreen } | Out-Null
    Native-Text "first native line`nsecond native line"
    Wait-Native 'native multiline text input' { (Read-NativeText $notes).Replace("`r",'') -eq "first native line`nsecond native line" } | Out-Null
    Native-Keys '^a'
    $textPattern = $notes.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
    Wait-Native 'native modifier selection' { $textPattern.GetSelection()[0].GetText(-1).Replace("`r",'').TrimEnd("`n") -eq "first native line`nsecond native line" } | Out-Null
    Native-Keys '{BACKSPACE}'
    Wait-Native 'cleared multiline field' { (Read-NativeText $notes) -eq '' } | Out-Null
    Invoke-Native (Find-Named $window 'Library')
    $dialogOwners = @()
    for ($attempt=0; $attempt -lt 3; $attempt++) {
        Invoke-Native (Wait-Native 'enabled Open recipe' { Find-Named $window 'Open recipe…' })
        $dialog = Wait-Native 'native common file dialog' {
            foreach ($candidate in $root.FindAll([System.Windows.Automation.TreeScope]::Children,$condition)) {
                if ($candidate.Current.ClassName -eq '#32770' -and $null -ne (Find-Named $candidate 'Cancel')) { return $candidate }
            }
            return $null
        }
        $owner = [NativeQaWindow]::GetWindow([IntPtr]$dialog.Current.NativeWindowHandle,4)
        if ($owner -ne $handle) { throw "Native file dialog has wrong owner $owner (expected $handle)" }
        $dialogOwners += $owner.ToInt64()
        if ($attempt -lt 2) { Invoke-Native (Find-Named $dialog 'Cancel') }
        else { $dialog.SetFocus(); Native-Keys '^l'; Native-Text $Fixture; Native-Keys '{ENTER}' }
        Wait-Native 'native chooser closed and application recovered' {
            try { return -not $dialog.Current.IsEnabled -or $dialog.Current.IsOffscreen } catch [System.Windows.Automation.ElementNotAvailableException] { return $true }
        } | Out-Null
    }
    Wait-Native 'opened fixture selected in production provider' { Find-Named $window 'My 3 Hz pair' } | Out-Null
    if ($process.HasExited) { throw 'Production application exited during native qualification' }
    @{session='windows'; provider='UI Automation'; production='Winit/FemtoVG'; first_frame_colors=$colors.Count; native_gl_driver=$driver[0].FileName;
      multiline_native_keys=$true; modifier_selection=$true; native_dialog_cancel_reopen=$true;
      native_dialog_open='My 3 Hz pair'; native_dialog_owners=$dialogOwners; audio_device_opened=$false} | ConvertTo-Json -Compress
} catch {
    # Startup can fail before a production main provider exists. Retain all
    # PID-owned native HWNDs and UIA error-dialog text, not just the absent main.
    $details = [Collections.Generic.List[string]]::new()
    $details.Add("PID=$($process.Id) Session=$($process.SessionId) UserInteractive=$([Environment]::UserInteractive) Exited=$($process.HasExited)")
    if ($process.HasExited) { $details.Add("ExitCode=$($process.ExitCode)") }
    foreach ($hwnd in [NativeQaWindow]::HandlesFor([uint32]$process.Id)) {
        $details.Add("HWND=$hwnd Visible=$([NativeQaWindow]::IsWindowVisible($hwnd)) Title=$([NativeQaWindow]::Title($hwnd))")
        try {
            $provider = $uia::FromHandle($hwnd)
            $details.Add("UIA=$($provider.Current.Name) Offscreen=$($provider.Current.IsOffscreen) Bounds=$($provider.Current.BoundingRectangle)")
            foreach ($child in ($provider.FindAll($scope,[System.Windows.Automation.Condition]::TrueCondition) | Select-Object -First 100)) {
                $details.Add("$($child.Current.ControlType.ProgrammaticName): $($child.Current.Name)")
            }
        } catch { $details.Add("Provider inspection failed: $_") }
    }
    [IO.File]::WriteAllLines((Join-Path $Scratch 'failure-provider.txt'),$details)
    throw
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit(10000) | Out-Null }
}
