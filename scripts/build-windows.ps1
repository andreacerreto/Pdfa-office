param(
    [switch]$SkipDependencies,
    [string]$MakeNsis = '',
    [string]$SevenZip = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$project = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ($env:OS -ne 'Windows_NT') { throw 'Eseguire questo script su Windows x64.' }
$package = Join-Path $project 'build\package'
$dist = Join-Path $project 'dist'
New-Item -ItemType Directory -Force $package, $dist | Out-Null
$versionLine = Get-Content (Join-Path $project 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([^"]+)"' } | Select-Object -First 1
if (-not ($versionLine -match '^version\s*=\s*"([^"]+)"')) { throw 'Versione non trovata in Cargo.toml.' }
$version = $Matches[1]

Push-Location $project
try {
    cargo test --locked --no-default-features
    if ($LASTEXITCODE -ne 0) { throw 'Test Rust non riusciti.' }
    cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Compilazione Rust non riuscita.' }
    $metadataJson = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Impossibile individuare la cartella di compilazione Cargo.' }
    $targetDirectory = ($metadataJson | ConvertFrom-Json).target_directory
    if (-not $SkipDependencies) {
        & (Join-Path $PSScriptRoot 'prepare-dependencies.ps1') -PackageDirectory $package -SevenZip $SevenZip
    }
    foreach ($binary in @('pdfa-office.exe','pdfa-office-cli.exe')) {
        Copy-Item -LiteralPath (Join-Path $targetDirectory "release\$binary") -Destination $package -Force
    }
    foreach ($name in @('README.md','LICENSE','THIRD-PARTY-NOTICES.md','TESTING.md')) {
        if (Test-Path (Join-Path $project $name)) { Copy-Item -LiteralPath (Join-Path $project $name) -Destination $package -Force }
    }
    foreach ($name in @('docs','licenses')) {
        if (Test-Path (Join-Path $project $name)) { Copy-Item -LiteralPath (Join-Path $project $name) -Destination $package -Recurse -Force }
    }
    Copy-Item -LiteralPath (Join-Path $project 'assets\icon.ico') -Destination (Join-Path $package 'icon.ico') -Force
    'PDF/A Office managed installation' | Set-Content (Join-Path $package 'pdfa-office.install') -Encoding ASCII
    # Include the complete application source with the installer, preserving
    # hidden project files and excluding build products and downloaded binaries.
    $sourceZip = Join-Path $dist "PDF-A-Office-$version-Sorgenti.zip"
    if (Test-Path $sourceZip) { Remove-Item -LiteralPath $sourceZip -Force }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::Open($sourceZip, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($sourceItem in @('Cargo.toml','Cargo.lock','rust-toolchain.toml','.gitignore','README.md','LICENSE','THIRD-PARTY-NOTICES.md','TESTING.md','src','assets','installer','scripts','tests','docs','licenses','.github')) {
            $sourcePath = Join-Path $project $sourceItem
            if (-not (Test-Path -LiteralPath $sourcePath)) { continue }
            $sourceFiles = if (Test-Path -LiteralPath $sourcePath -PathType Container) {
                Get-ChildItem -LiteralPath $sourcePath -Recurse -File -Force
            } else { Get-Item -LiteralPath $sourcePath }
            foreach ($sourceFile in $sourceFiles) {
                $entry = 'pdfa-office/' + $sourceFile.FullName.Substring($project.Length).TrimStart('\','/').Replace('\','/')
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive,$sourceFile.FullName,$entry,[System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
            }
        }
    } finally { $archive.Dispose() }
    $sourceFolder = Join-Path $package 'source'
    New-Item -ItemType Directory -Force $sourceFolder | Out-Null
    Copy-Item -LiteralPath $sourceZip -Destination $sourceFolder -Force
    # Uninstall only exact package files, then remove directories only if empty.
    # A user may save converted PDFs anywhere, including inside a bundled folder.
    $manifest = Join-Path $project 'build\uninstall-files.nsh'
    $manifestLines = [System.Collections.Generic.List[string]]::new()
    $manifestLines.Add('; Generated from the exact package contents. Do not hand edit.')
    $manifestLines.Add('!macro RemoveInstalledFiles')
    function Escape-NsisPath([string]$Path) {
        if ($Path -match "[\r\n\t]") { throw "Carattere non supportato nel percorso: $Path" }
        return $Path.Replace('$','$$').Replace('"','$\"')
    }
    Get-ChildItem -LiteralPath $package -Recurse -File -Force | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($package.Length).TrimStart('\','/')
        $manifestLines.Add('  Delete "$INSTDIR\' + (Escape-NsisPath $relative) + '"')
    }
    Get-ChildItem -LiteralPath $package -Recurse -Directory -Force | Sort-Object @{Expression={$_.FullName.Length};Descending=$true} | ForEach-Object {
        $relative = $_.FullName.Substring($package.Length).TrimStart('\','/')
        $manifestLines.Add('  RMDir "$INSTDIR\' + (Escape-NsisPath $relative) + '"')
    }
    $manifestLines.Add('!macroend')
    $manifestLines | Set-Content -LiteralPath $manifest -Encoding UTF8
    if (-not $MakeNsis) {
        $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
        if ($command) { $MakeNsis = $command.Source }
        elseif (Test-Path 'C:\Program Files (x86)\NSIS\makensis.exe') { $MakeNsis = 'C:\Program Files (x86)\NSIS\makensis.exe' }
        else { throw 'Installare NSIS 3 oppure specificare -MakeNsis.' }
    }
    & $MakeNsis /V2 /INPUTCHARSET UTF8 "/DAPP_VERSION=$version" "/DPACKAGE_DIR=$package" "/DOUTPUT_DIR=$dist" "/DUNINSTALL_INCLUDE=$manifest" (Join-Path $project 'installer\pdfa-office.nsi')
    if ($LASTEXITCODE -ne 0) { throw 'Creazione installer NSIS non riuscita.' }
    $portable = Join-Path $dist "PDF-A-Office-$version-Windows-x64-portable.zip"
    Compress-Archive -Path (Join-Path $package '*') -DestinationPath $portable -Force
    $checksums = @()
    Get-ChildItem -LiteralPath $dist -File | Where-Object { $_.Extension -in @('.exe','.zip') } | ForEach-Object {
        $checksums += "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)"
    }
    $checksums | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ASCII
    Write-Host "Installer e versione portabile disponibili in: $dist"
} finally { Pop-Location }
