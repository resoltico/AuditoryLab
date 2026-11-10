# Persisted keyboard witness. The ordinary CLI validates the actual TOML file;
# the fixed canonical notes block below is a literal fixture oracle, not a parser.
function Read-SavedRecord([string]$Cli, [string]$Library, [string]$ExpectedNotes, $Previous) {
    $output = & $Cli --library-dir $Library list --collection user --format json
    if ($LASTEXITCODE -eq 4) { return $null }
    if ($LASTEXITCODE -ne 0) { throw 'Native save library listing failed' }
    $listed = ($output -join "`n") | ConvertFrom-Json
    if (-not $listed.complete) { throw 'Native save library listing is incomplete' }
    $entries = @($listed.entries)
    if ($entries.Count -eq 0 -and $null -eq $Previous) { return $null }
    if ($entries.Count -ne 1) { throw 'Native save must own exactly one user record' }
    $entry = $entries[0]
    if ($null -ne $Previous -and $entry.version_token -eq $Previous.version_token) { return $null }
    if ($entry.collection -ne 'user' -or $entry.trashed -or -not $entry.version_token) { throw 'Native save did not publish an active owned record' }
    if ($null -ne $Previous -and ($entry.key -ne $Previous.key -or $entry.document_id -ne $Previous.document_id)) { throw 'Native save created another record instead of updating its binding' }
    $path = Join-Path (Join-Path $Library 'recipes') ($entry.document_id + '.toml')
    $output = & $Cli --library-dir $Library describe --file $path --format json
    if ($LASTEXITCODE -eq 4) { return $null }
    if ($LASTEXITCODE -ne 0) { throw 'Native save persisted an invalid recipe file' }
    $described = ($output -join "`n") | ConvertFrom-Json
    if (-not $described.complete -or @($described.entries).Count -ne 1 -or $described.entries[0].document_id -ne $entry.document_id) { throw 'Native save persisted wrong identity' }
    $canonical = [string]$described.entries[0].recipe_toml
    if ([IO.File]::ReadAllText($path) -cne $canonical) { throw 'Native save file differs from ordinary CLI canonical recipe' }
    if ($ExpectedNotes -ceq "first native line`nsecond native line") {
        $literal = 'notes = """' + "`nfirst native line`nsecond native line" + '"""' + "`n`n[timing]"
    } elseif ($ExpectedNotes -ceq '') {
        $literal = 'notes = ""' + "`n`n[timing]"
    } else { throw 'Unexpected native notes fixture' }
    if (-not $canonical.Contains($literal) -or ([regex]::Matches($canonical,'(?m)^notes = ')).Count -ne 1) { throw 'Native save persisted wrong literal notes' }
    return @{key=$entry.key; document_id=$entry.document_id; version_token=$entry.version_token;
             sha256=(Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant(); owned_records=1; notes=$ExpectedNotes}
}
