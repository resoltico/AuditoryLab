# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
# Run only in a dedicated powershell.exe process. Never dot-source this script.
#requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Binary,
      [Parameter(Mandatory=$true)][string]$Scratch)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows console verification requires Windows.' }
if ($MyInvocation.InvocationName -eq '.') { throw 'Run this script in a dedicated powershell.exe process; do not dot-source it.' }
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
$scratchPath = [IO.Path]::GetFullPath($Scratch)
$targetPrefix = [IO.Path]::GetFullPath((Join-Path $repo 'target')) + [IO.Path]::DirectorySeparatorChar
if (-not $scratchPath.StartsWith($targetPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Scratch must be under this checkout target/.' }
# Reject reparse-point ancestors: lexical containment alone is insufficient.
$ancestor = $scratchPath
while ($ancestor.Length -ge $targetPrefix.TrimEnd('\').Length) {
    if (Test-Path -LiteralPath $ancestor) {
        if (((Get-Item -LiteralPath $ancestor).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse-point scratch ancestor: $ancestor" }
    }
    $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    if (-not $ancestor) { break }
}
[IO.Directory]::CreateDirectory($scratchPath) | Out-Null
$owned = Join-Path $scratchPath ('case-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $owned) { throw 'Owned case directory already exists.' }
[IO.Directory]::CreateDirectory($owned) | Out-Null
$oldTemp = $env:TEMP; $oldTmp = $env:TMP
$env:TEMP = $owned; $env:TMP = $owned
$results = New-Object 'System.Collections.Generic.List[string]'
$overall = 0
$binaryHash = 'UNAVAILABLE'; $sourceCommit = 'UNAVAILABLE'; $rustVersion = 'UNAVAILABLE'
try {
    $binaryHash = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash
    $sourceCommit = if ($env:GITHUB_SHA) { $env:GITHUB_SHA } elseif (Test-Path -LiteralPath (Join-Path $repo '.git')) { (& git -C $repo rev-parse HEAD) } else { 'UNAVAILABLE' }
    $rustVersion = if (Get-Command rustc -ErrorAction SilentlyContinue) { (& rustc --version) } else { 'UNAVAILABLE' }
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public sealed class InterruptChild : IDisposable {
    const uint GENERIC_READ=0x80000000, GENERIC_WRITE=0x40000000;
    const uint OPEN_EXISTING=3, WAIT_TIMEOUT=258, WAIT_OBJECT_0=0;
    static readonly IntPtr Invalid=new IntPtr(-1);
    [StructLayout(LayoutKind.Sequential)] struct SA { public int length; public IntPtr descriptor; [MarshalAs(UnmanagedType.Bool)] public bool inherit; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct SI {
        public uint cb; public string reserved, desktop, title;
        public uint x,y,xsize,ysize,xchars,ychars,fill,flags;
        public ushort show, reservedBytes; public IntPtr reservedPointer,input,output,error;
    }
    [StructLayout(LayoutKind.Sequential)] struct PI { public IntPtr process,thread; public uint pid,tid; }
    [UnmanagedFunctionPointer(CallingConvention.Winapi)] [return: MarshalAs(UnmanagedType.Bool)] delegate bool Handler(uint kind);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool AllocConsole();
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool AttachConsole(uint pid);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool SetConsoleCtrlHandler(Handler callback,[MarshalAs(UnmanagedType.Bool)] bool add);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool GenerateConsoleCtrlEvent(uint kind,uint group);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool CreateProcessW(string application,StringBuilder command,IntPtr processSA,IntPtr threadSA,[MarshalAs(UnmanagedType.Bool)] bool inherit,uint flags,IntPtr environment,string directory,ref SI startup,out PI info);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateFileW(string path,uint access,uint share,ref SA security,uint disposition,uint flags,IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool CreatePipe(out IntPtr read,out IntPtr write,ref SA security,uint size);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool SetHandleInformation(IntPtr handle,uint mask,uint flags);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool ReadFile(IntPtr handle,byte[] bytes,uint count,out uint read,IntPtr overlapped);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool WriteFile(IntPtr handle,byte[] bytes,uint count,out uint written,IntPtr overlapped);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool GetNamedPipeInfo(IntPtr handle,out uint flags,out uint outgoing,out uint incoming,out uint instances);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle,uint millis);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool GetExitCodeProcess(IntPtr handle,out uint code);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool TerminateProcess(IntPtr handle,uint code);
    [DllImport("kernel32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool CloseHandle(IntPtr handle);

    IntPtr process,thread,input,outputRead,inputRead,outputWrite,errorHandle;
    uint pid,inputCapacity; bool attached,blocked; Thread reader;
    Handler handler; Exception readerError; bool overflow;
    MemoryStream capture=new MemoryStream();
    static void Check(bool ok,string api) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error(),api); }
    static SA Security(bool inherit) { SA s=new SA(); s.length=Marshal.SizeOf(typeof(SA)); s.inherit=inherit; return s; }
    static void Close(ref IntPtr h) { if (h!=IntPtr.Zero && h!=Invalid) { IntPtr old=h; h=IntPtr.Zero; Check(CloseHandle(old),"CloseHandle"); } }
    static void Detach() { if (!FreeConsole()) { int error=Marshal.GetLastWin32Error(); if(error!=6) throw new Win32Exception(error,"FreeConsole"); } }
    public static string Quote(string value) {
        StringBuilder s=new StringBuilder("\""); int slashes=0;
        foreach(char ch in value) {
            if(ch=='\\') { slashes++; continue; }
            if(ch=='"') { s.Append('\\',slashes*2+1); s.Append(ch); }
            else { s.Append('\\',slashes); s.Append(ch); }
            slashes=0;
        }
        s.Append('\\',slashes*2); s.Append('"'); return s.ToString();
    }
    // At the readable (server) endpoint, incoming describes writer -> reader.
    static uint Capacity(IntPtr read) {
        uint flags,outgoing,incoming,instances;
        Check(GetNamedPipeInfo(read,out flags,out outgoing,out incoming,out instances),"GetNamedPipeInfo");
        uint capacity=(flags & 1)!=0 ? incoming : outgoing;
        if(capacity==0 || capacity>1048576) throw new Exception("Unsupported measured pipe capacity "+capacity);
        return capacity;
    }
    void StopOwned() {
        if(process!=IntPtr.Zero && IsRunning) { Check(TerminateProcess(process,1),"TerminateProcess"); if(!WaitForExit(20000)) throw new Exception("Owned child termination timeout"); }
    }
    static void WriteAll(IntPtr handle,byte[] bytes) {
        uint written; Check(WriteFile(handle,bytes,(uint)bytes.Length,out written,IntPtr.Zero),"WriteFile");
        if(written!=bytes.Length) throw new Exception("WriteFile incomplete: "+written+" / "+bytes.Length);
    }
    void BoundedWrite(IntPtr handle,byte[] bytes,bool prefill) {
        Exception failure=null;
        Thread writer=new Thread(delegate() { try { WriteAll(handle,bytes); } catch(Exception e) { failure=e; } });
        writer.Start();
        if(!writer.Join(prefill ? 5000 : 20000)) {
            if(prefill) Close(ref outputRead); else StopOwned();
            if(!writer.Join(20000)) throw new Exception("Owned writer did not release after timeout cleanup");
            throw new Exception(prefill ? "Stdout prefill timeout" : "Input write timeout");
        }
        if(failure!=null) throw new Exception("Bounded write failed",failure);
    }
    public static InterruptChild Start(string binary,string[] args,string directory,bool fillStdout,bool captureStdout) {
        InterruptChild child=new InterruptChild();
        try {
            Detach(); Check(AllocConsole(),"AllocConsole"); child.attached=true;
            Check(SetConsoleCtrlHandler(null,false),"SetConsoleCtrlHandler(clear inherited ignore)");
            SA inherited=Security(true);
            Check(CreatePipe(out child.inputRead,out child.input,ref inherited,4096),"CreatePipe(stdin)");
            child.inputCapacity=Capacity(child.inputRead);
            Check(SetHandleInformation(child.input,1,0),"SetHandleInformation(stdin parent)");
            child.blocked=fillStdout;
            if(fillStdout || captureStdout) {
                Check(CreatePipe(out child.outputRead,out child.outputWrite,ref inherited,4096),"CreatePipe(stdout)");
                Check(SetHandleInformation(child.outputRead,1,0),"SetHandleInformation(stdout parent)");
                if(fillStdout) child.BoundedWrite(child.outputWrite,new byte[Capacity(child.outputRead)],true);
            } else { child.outputWrite=CreateFileW("NUL",GENERIC_WRITE,3,ref inherited,OPEN_EXISTING,0,IntPtr.Zero); Check(child.outputWrite!=Invalid,"CreateFileW(stdout NUL)"); }
            child.errorHandle=CreateFileW("NUL",GENERIC_WRITE,3,ref inherited,OPEN_EXISTING,0,IntPtr.Zero); Check(child.errorHandle!=Invalid,"CreateFileW(stderr NUL)");
            SI startup=new SI(); startup.cb=(uint)Marshal.SizeOf(typeof(SI)); startup.flags=0x100|1; startup.show=0;
            startup.input=child.inputRead; startup.output=child.outputWrite; startup.error=child.errorHandle;
            StringBuilder command=new StringBuilder(Quote(binary)); foreach(string arg in args) command.Append(" "+Quote(arg));
            PI info; Check(CreateProcessW(binary,command,IntPtr.Zero,IntPtr.Zero,true,0x10,IntPtr.Zero,directory,ref startup,out info),"CreateProcessW(CREATE_NEW_CONSOLE)");
            child.process=info.process; child.thread=info.thread; child.pid=info.pid;
            Close(ref child.inputRead); Close(ref child.outputWrite); Close(ref child.errorHandle);
            if(captureStdout && !fillStdout) child.StartReader();
            Detach(); child.attached=false; Check(AttachConsole(child.pid),"AttachConsole(child)"); child.attached=true;
            child.handler=delegate(uint kind) { return kind==0 || kind==1; };
            Check(SetConsoleCtrlHandler(child.handler,true),"SetConsoleCtrlHandler(owned console handler)");
            return child;
        } catch { child.Dispose(); throw; }
    }
    void StartReader() {
        reader=new Thread(delegate() {
            try {
                byte[] bytes=new byte[4096]; uint count;
                while(true) {
                    if(!ReadFile(outputRead,bytes,(uint)bytes.Length,out count,IntPtr.Zero)) {
                        int error=Marshal.GetLastWin32Error(); if(error==109) break;
                        throw new Win32Exception(error,"ReadFile(stdout)");
                    }
                    if(count==0) break;
                    int room=65536-(int)capture.Length, keep=Math.Min(room,(int)count);
                    if(keep>0) capture.Write(bytes,0,keep);
                    if(keep<count) overflow=true; // Continue draining without backpressure.
                }
            } catch(Exception e) { readerError=e; }
        }); reader.Start();
    }
    public void WriteInput(byte[] bytes) { BoundedWrite(input,bytes,false); }
    public void CloseInput() { Close(ref input); }
    public uint GetInputCapacity() { return inputCapacity; }
    public void SendControlEvent(uint kind) {
        if(!attached || pid==0) throw new Exception("No isolated child console attached");
        Check(GenerateConsoleCtrlEvent(kind,0),"GenerateConsoleCtrlEvent(isolated group zero)");
    }
    public bool WaitForExit(int millis) {
        uint result=WaitForSingleObject(process,(uint)millis);
        if(result==WAIT_OBJECT_0) return true; if(result==WAIT_TIMEOUT) return false;
        throw new Win32Exception(Marshal.GetLastWin32Error(),"WaitForSingleObject");
    }
    public bool IsRunning { get { return !WaitForExit(0); } }
    public uint ExitCode { get { if(IsRunning) throw new Exception("Child still running"); uint code; Check(GetExitCodeProcess(process,out code),"GetExitCodeProcess"); return code; } }
    public string ReadStdoutAfterExit() {
        if(IsRunning || blocked || reader==null) throw new Exception("Capture requires exited ordinary-capture child");
        if(!reader.Join(20000)) throw new Exception("Capture EOF timeout");
        if(readerError!=null) throw new Exception("Capture reader failed",readerError);
        if(overflow) throw new Exception("Captured stdout exceeded 64 KiB");
        return Encoding.UTF8.GetString(capture.ToArray());
    }
    public byte[] WaitForClosedOutput(string path) {
        Stopwatch watch=Stopwatch.StartNew(); SA security=Security(false);
        while(watch.ElapsedMilliseconds<20000) {
            if(!IsRunning) throw new Exception("Child exited before closed-output barrier: "+ExitCode);
            IntPtr handle=CreateFileW(path,GENERIC_READ,0,ref security,OPEN_EXISTING,0,IntPtr.Zero);
            if(handle!=Invalid) {
                try {
                    byte[] bytes=new byte[4096]; uint count;
                    Check(ReadFile(handle,bytes,(uint)bytes.Length,out count,IntPtr.Zero),"ReadFile(exclusive WAV)");
                    if(count!=66) throw new Exception("One-frame WAV length must be 66, got "+count);
                    byte[] actual=new byte[count]; Array.Copy(bytes,actual,count);
                    ValidateWave(actual); return actual;
                } finally { Close(ref handle); }
            }
            int error=Marshal.GetLastWin32Error(); if(error!=2 && error!=32) throw new Win32Exception(error,"CreateFileW(exclusive output barrier)");
            Thread.Sleep(5);
        }
        throw new Exception("Exclusive output completion barrier timeout");
    }
    public static void ValidateWave(byte[] bytes) {
        if(bytes.Length!=66 || Encoding.ASCII.GetString(bytes,0,4)!="RIFF" || BitConverter.ToUInt32(bytes,4)!=58 || Encoding.ASCII.GetString(bytes,8,4)!="WAVE") throw new Exception("Invalid independent RIFF header");
        int at=12; bool format=false,fact=false,data=false;
        while(at+8<=bytes.Length) {
            string id=Encoding.ASCII.GetString(bytes,at,4); uint size=BitConverter.ToUInt32(bytes,at+4); int body=at+8;
            if(size>bytes.Length-body) throw new Exception("WAV chunk exceeds file");
            if(id=="fmt ") {
                if(format || size!=18 || BitConverter.ToUInt16(bytes,body)!=3 || BitConverter.ToUInt16(bytes,body+2)!=2 || BitConverter.ToUInt32(bytes,body+4)!=48000 || BitConverter.ToUInt32(bytes,body+8)!=384000 || BitConverter.ToUInt16(bytes,body+12)!=8 || BitConverter.ToUInt16(bytes,body+14)!=32 || BitConverter.ToUInt16(bytes,body+16)!=0) throw new Exception("Invalid float stereo format");
                format=true;
            } else if(id=="fact") { if(fact || size!=4 || BitConverter.ToUInt32(bytes,body)!=1) throw new Exception("Invalid fact frame count"); fact=true; }
            else if(id=="data") { if(data || size!=8 || BitConverter.ToUInt32(bytes,body)!=0 || BitConverter.ToUInt32(bytes,body+4)!=0) throw new Exception("Expected one silent stereo frame"); data=true; }
            else throw new Exception("Unexpected WAV chunk "+id);
            at=checked(body+(int)size+(int)(size%2));
        }
        if(at!=bytes.Length || !format || !fact || !data) throw new Exception("Missing independent WAV fields");
    }
    public void Dispose() {
        try { StopOwned(); }
        finally {
            Close(ref input); Close(ref inputRead); Close(ref outputWrite); Close(ref errorHandle);
            if(reader!=null && !reader.Join(20000)) {
                Close(ref outputRead);
                if(!reader.Join(20000)) throw new Exception("Owned capture thread cleanup timeout");
            }
            Close(ref outputRead); Close(ref thread); Close(ref process);
            if(attached) { Detach(); attached=false; } handler=null; capture.Dispose();
        }
    }
}
'@
    # Independent quote expectations, including trailing slashes and Unicode.
    $quotes = @(
        @('plain', '"plain"'), @('two words', '"two words"'),
        @('a"b', '"a\"b"'), @('a\', '"a\\"'), @('a\"b', '"a\\\"b"'),
        @('tōne', '"tōne"'), @('', '""')
    )
    foreach ($pair in $quotes) { if ([InterruptChild]::Quote($pair[0]) -cne $pair[1]) { throw "Windows argument quote control failed: $($pair[0])" } }
    $long = "schema_version = 1`nduration_seconds = 600.0`nfade_seconds = 0.0`n"
    $one = $long.Replace('600.0', '0.000020833333333333333')
    $longPath = Join-Path $owned 'long stimulus.toml'; $onePath = Join-Path $owned 'one sīlence.toml'
    [IO.File]::WriteAllText($longPath, $long, (New-Object Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText($onePath, $one, (New-Object Text.UTF8Encoding($false)))
    foreach ($event in @(0, 1)) {
        foreach ($case in @('first-export', 'second-report', 'pre-input')) {
            $child = $null; $phase = 'start'; $watch = [Diagnostics.Stopwatch]::StartNew()
            $destination = Join-Path $owned ("event-$event-$case.wav")
            try {
                $inputPath = if ($case -eq 'pre-input') { '-' } elseif ($case -eq 'second-report') { $onePath } else { $longPath }
                $child = [InterruptChild]::Start($binaryPath, @('export', $inputPath, '--output', $destination, '--sample-rate', '48000'), $owned, ($case -eq 'second-report'), ($case -ne 'second-report'))
                if ($case -eq 'first-export') {
                    $child.CloseInput(); $phase = 'creation barrier'
                    while (-not (Test-Path -LiteralPath $destination)) {
                        if (-not $child.IsRunning) { throw "Exited before creation: $($child.ExitCode)" }
                        if ($watch.ElapsedMilliseconds -gt 20000) { throw 'Creation barrier timeout' }
                        Start-Sleep -Milliseconds 5
                    }
                    if (-not $child.IsRunning) { throw 'Exited at creation barrier' }
                    $phase = 'first event'; $child.SendControlEvent($event)
                    if (-not $child.WaitForExit(20000)) { throw 'First-event exit timeout' }
                    if ($child.ExitCode -ne 130) { throw "Expected 130, got $($child.ExitCode)" }
                    if (Test-Path -LiteralPath $destination) { throw 'Cancelled output remains' }
                    if ($child.ReadStdoutAfterExit().Length -ne 0) { throw 'Cancelled export reported success' }
                } elseif ($case -eq 'second-report') {
                    $child.CloseInput(); $phase = 'closed completed output barrier'
                    $before = $child.WaitForClosedOutput($destination)
                    $phase = 'first event while reporting blocked'; $child.SendControlEvent($event)
                    if ($child.WaitForExit(250)) { throw "First event forced exit: $($child.ExitCode)" }
                    $phase = 'second event'; $child.SendControlEvent($event)
                    if (-not $child.WaitForExit(20000)) { throw 'Second-event exit timeout' }
                    if ($child.ExitCode -ne 130) { throw "Expected 130, got $($child.ExitCode)" }
                    $after = [IO.File]::ReadAllBytes($destination)
                    if ([Convert]::ToBase64String($before) -cne [Convert]::ToBase64String($after)) { throw 'Completed file changed after second event' }
                } else {
                    $phase = 'stdin consumption barrier'; $capacity = $child.GetInputCapacity()
                    if ($capacity -eq 0 -or $capacity -gt 32768) { throw "Unexpected stdin capacity $capacity" }
                    $prefix = "schema_version = 1`n#"
                    $padding = $prefix + ('x' * ([int]$capacity + 1 - $prefix.Length))
                    $child.WriteInput([Text.Encoding]::UTF8.GetBytes($padding))
                    if (-not $child.IsRunning -or (Test-Path -LiteralPath $destination)) { throw 'Invalid pre-input barrier state' }
                    $phase = 'native pre-input event'; $child.SendControlEvent($event)
                    if (-not $child.WaitForExit(20000)) { throw 'Pre-input native exit timeout' }
                    if ($child.ExitCode -ne [uint32]3221225786) { throw "Expected STATUS_CONTROL_C_EXIT 0xC000013A, got $($child.ExitCode)" }
                    if (Test-Path -LiteralPath $destination) { throw 'Pre-input export created output' }
                    if ($child.ReadStdoutAfterExit().Length -ne 0) { throw 'Pre-input child emitted stdout' }
                }
                $results.Add("PASS event=$event case=$case elapsed_ms=$($watch.ElapsedMilliseconds)")
            } catch {
                $overall = 1
                $code = if ($null -ne $child -and -not $child.IsRunning) { [string]$child.ExitCode } else { 'running/unstarted' }
                $results.Add("FAIL event=$event case=$case phase=$phase exit=$code file_exists=$(Test-Path -LiteralPath $destination) elapsed_ms=$($watch.ElapsedMilliseconds): $_")
            } finally {
                if ($null -ne $child) { $child.Dispose() }
            }
        }
    }
    if ($results.Count -ne 6) { throw "Expected six distinct native cases, got $($results.Count)" }
} catch {
    $overall = 1; $results.Add("HARNESS FAIL: $_")
} finally {
    $env:TEMP = $oldTemp; $env:TMP = $oldTmp
    Remove-Item -LiteralPath $owned -Recurse -Force
    $header = "source=$repo commit=$sourceCommit binary=$binaryPath binary_sha256=$binaryHash toolchain=$rustVersion host=$([Environment]::OSVersion) powershell=$($PSVersionTable.PSVersion) cases=$($results.Count) overall=$overall"
    [IO.File]::WriteAllLines((Join-Path $scratchPath 'report.txt'), @($header) + $results.ToArray())
    $results | Write-Output
}
exit $overall
