# Private CI prerequisite: authenticated Mesa WGL runtime in target/debug only.
# Pinned package/source/license data live in windows-mesa.json. No system driver
# installation, product archive changes, or Slint renderer substitution occurs.
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$runtime = Join-Path $repo 'target/native-driver-runtime'
$binaryDirectory = Join-Path $repo 'target/debug'
$evidenceDirectory = Join-Path $repo 'target/native-windows'
$sevenZip = Join-Path $env:ProgramFiles '7-Zip/7z.exe'
if (-not (Test-Path $sevenZip)) { throw 'Native Mesa prerequisite requires the hosted image 7-Zip installation' }
if (Test-Path $runtime) { throw 'Native driver scratch already exists; preserve or clean its earlier evidence before another preparation' }
[IO.Directory]::CreateDirectory($runtime) | Out-Null
[IO.Directory]::CreateDirectory($binaryDirectory) | Out-Null
[IO.Directory]::CreateDirectory($evidenceDirectory) | Out-Null
$manifest = Get-Content -Raw (Join-Path $PSScriptRoot 'windows-mesa.json') | ConvertFrom-Json
@{state='preparing'; purpose=$manifest.purpose; packages=$manifest.packages} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDirectory 'driver-provenance.json')
$delivered = [Collections.Generic.List[object]]::new()
foreach ($package in $manifest.packages) {
    $uri = [Uri]$package.url
    if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'repo.msys2.org' -or $package.sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Invalid pinned native-driver source' }
    $archive = Join-Path $runtime ([IO.Path]::GetFileName($uri.LocalPath))
    Invoke-WebRequest -Uri $uri -OutFile $archive -MaximumRedirection 0
    if ((Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant() -ne $package.sha256) { throw "Native driver package digest mismatch: $($package.package)" }
    $directory = Join-Path $runtime $package.package
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    & $sevenZip x -y "-o$directory" $archive | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Cannot decompress pinned native driver $($package.package)" }
    $tar = Join-Path $directory ([IO.Path]::GetFileNameWithoutExtension($archive))
    & $sevenZip x -y "-o$directory" $tar | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Cannot extract pinned native driver $($package.package)" }
    foreach ($name in $package.files) {
        $source = Join-Path $directory "mingw64/bin/$name"
        $destination = Join-Path $binaryDirectory $name
        if (Test-Path $destination) { throw "Native driver would replace existing build output: $destination" }
        Copy-Item -LiteralPath $source -Destination $destination
        $sourceHash = (Get-FileHash -Algorithm SHA256 $source).Hash.ToLowerInvariant()
        if ((Get-FileHash -Algorithm SHA256 $destination).Hash.ToLowerInvariant() -ne $sourceHash) { throw "Native driver copy differs: $name" }
        $delivered.Add(@{name=$name; sha256=$sourceHash; package=$package.package; version=$package.version; package_sha256=$package.sha256})
    }
}
@{state='prepared'; purpose=$manifest.purpose; renderer='Mesa WGL llvmpipe'; runner_image=$env:ImageVersion;
  packages=$manifest.packages; delivered=$delivered} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDirectory 'driver-provenance.json')
Write-Output "Prepared $($delivered.Count) authenticated native Mesa DLLs in target/debug; no product archive inputs changed."
