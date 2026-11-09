# Compile only the narrow official-SDK UIA reference client, never product files.
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$scratch = Join-Path $repo 'target/native-ui-control'
[IO.Directory]::CreateDirectory($scratch) | Out-Null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Native UIA control requires the existing MSVC/Windows SDK installation' }
$vcvars = Join-Path $installation 'VC/Auxiliary/Build/vcvars64.bat'
$source = Join-Path $PSScriptRoot 'text-range-control.cpp'
$object = Join-Path $scratch 'text-range-control.obj'
$executable = Join-Path $scratch 'text-range-control.exe'
$commands = @(
    '@echo off',
    "call `"$vcvars`" >nul",
    'if errorlevel 1 exit /b %errorlevel%',
    "cl.exe /nologo /EHsc /W4 /WX /std:c++17 /O2 /DUNICODE /D_UNICODE `"$source`" /Fo:`"$object`" /Fe:`"$executable`" /link ole32.lib oleaut32.lib uiautomationcore.lib uuid.lib",
    'exit /b %errorlevel%'
)
[IO.File]::WriteAllLines((Join-Path $scratch 'build-control.cmd'),$commands,[Text.Encoding]::ASCII)
Push-Location $scratch
try {
    & $env:ComSpec /d /c build-control.cmd
    if ($LASTEXITCODE -ne 0) { throw 'Official-SDK native text-range control did not compile' }
} finally { Pop-Location }
