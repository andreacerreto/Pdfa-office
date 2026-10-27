param(
    [string]$PackageDirectory = (Join-Path $PSScriptRoot '..\build\package'),
    [string]$CacheDirectory = (Join-Path $PSScriptRoot '..\build\downloads'),
    [string]$SevenZip = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not [Environment]::Is64BitOperatingSystem -or $env:OS -ne 'Windows_NT') {
    throw 'La preparazione di questo pacchetto richiede Windows x64.'
}
New-Item -ItemType Directory -Force $PackageDirectory, $CacheDirectory | Out-Null
$PackageDirectory = (Resolve-Path $PackageDirectory).Path
$CacheDirectory = (Resolve-Path $CacheDirectory).Path
$dependencies = Get-Content (Join-Path $PSScriptRoot 'dependencies.json') -Raw | ConvertFrom-Json

function Get-VerifiedDownload($Dependency) {
    $destination = Join-Path $CacheDirectory $Dependency.filename
    if (Test-Path $destination) {
        if ((Get-FileHash $destination -Algorithm SHA256).Hash.ToLowerInvariant() -eq $Dependency.sha256) {
            return $destination
        }
        throw "Checksum errato per il file in cache: $destination. Rimuoverlo e riprovare."
    }
    $partial = $destination + '.partial'
    Invoke-WebRequest -Uri $Dependency.url -OutFile $partial
    if ((Get-FileHash $partial -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Dependency.sha256) {
        Remove-Item $partial -Force
        throw "Checksum SHA-256 errato: $($Dependency.filename). Pacchetto non utilizzato."
    }
    Move-Item -LiteralPath $partial -Destination $destination
    return $destination
}

if (-not $SevenZip) {
    $command = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if ($command) { $SevenZip = $command.Source }
    elseif (Test-Path 'C:\Program Files\7-Zip\7z.exe') { $SevenZip = 'C:\Program Files\7-Zip\7z.exe' }
    else { throw 'Installare 7-Zip oppure specificare -SevenZip con il percorso di 7z.exe.' }
}
$resources = Join-Path $PackageDirectory 'resources'
New-Item -ItemType Directory -Force $resources | Out-Null

$gsArchive = Get-VerifiedDownload $dependencies.ghostscript
$gsExtract = Join-Path $CacheDirectory 'ghostscript-extracted'
& $SevenZip x -y "-o$gsExtract" $gsArchive | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Estrazione Ghostscript non riuscita.' }
$gsTarget = Join-Path $resources 'ghostscript'
New-Item -ItemType Directory -Force $gsTarget | Out-Null
foreach ($folder in @('bin','lib','Resource','iccprofiles','doc')) {
    Copy-Item -LiteralPath (Join-Path $gsExtract $folder) -Destination $gsTarget -Recurse -Force
}

$javaArchive = Get-VerifiedDownload $dependencies.java
$javaExtract = Join-Path $CacheDirectory 'java-extracted'
Expand-Archive -LiteralPath $javaArchive -DestinationPath $javaExtract -Force
$javaRoot = @(Get-ChildItem -LiteralPath $javaExtract -Directory)
if ($javaRoot.Count -ne 1) { throw 'Struttura inattesa del pacchetto Java.' }
$javaTarget = Join-Path $resources 'java'
New-Item -ItemType Directory -Force $javaTarget | Out-Null
Get-ChildItem -LiteralPath $javaRoot[0].FullName | Copy-Item -Destination $javaTarget -Recurse -Force
$javaExe = Join-Path $javaTarget 'bin\java.exe'
# Use the same unmodified Microsoft C runtime already shipped by Temurin.
# App-local DLLs avoid requiring an administrator or a system runtime install.
foreach ($dll in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')) {
    Copy-Item -LiteralPath (Join-Path $javaTarget "bin\$dll") -Destination (Join-Path $gsTarget 'bin') -Force
}

$veraArchive = Get-VerifiedDownload $dependencies.verapdf
$veraExtract = Join-Path $CacheDirectory 'verapdf-installer'
Expand-Archive -LiteralPath $veraArchive -DestinationPath $veraExtract -Force
$installerJars = @(Get-ChildItem -LiteralPath $veraExtract -Recurse -Filter 'verapdf-izpack-installer-*.jar')
if ($installerJars.Count -ne 1) { throw 'Installer veraPDF non identificato in modo univoco.' }
$veraInstall = Join-Path $CacheDirectory 'verapdf-installed'
$xmlInstall = [System.Security.SecurityElement]::Escape($veraInstall)
$autoInstall = Join-Path $CacheDirectory 'verapdf-auto-install.xml'
@"
<AutomatedInstallation langpack="eng">
 <com.izforge.izpack.panels.htmlhello.HTMLHelloPanel id="welcome"/>
 <com.izforge.izpack.panels.target.TargetPanel id="install_dir"><installpath>$xmlInstall</installpath></com.izforge.izpack.panels.target.TargetPanel>
 <com.izforge.izpack.panels.packs.PacksPanel id="sdk_pack_select">
  <pack index="0" name="veraPDF GUI" selected="false"/>
  <pack index="1" name="veraPDF CLI" selected="true"/>
  <pack index="2" name="veraPDF Documentation" selected="true"/>
  <pack index="3" name="veraPDF Sample Plugins" selected="false"/>
 </com.izforge.izpack.panels.packs.PacksPanel>
 <com.izforge.izpack.panels.install.InstallPanel id="install"/>
 <com.izforge.izpack.panels.finish.FinishPanel id="finish"/>
</AutomatedInstallation>
"@ | Set-Content -LiteralPath $autoInstall -Encoding UTF8
& $javaExe '-Djava.awt.headless=true' -jar $installerJars[0].FullName $autoInstall
if ($LASTEXITCODE -ne 0) { throw 'Preparazione veraPDF non riuscita.' }
$veraTarget = Join-Path $resources 'verapdf'
New-Item -ItemType Directory -Force $veraTarget | Out-Null
$cliJar = Join-Path $veraInstall "bin\cli-$($dependencies.verapdf.version).jar"
if (-not (Test-Path $cliJar)) { throw 'JAR CLI veraPDF atteso non trovato.' }
Copy-Item -LiteralPath $cliJar -Destination (Join-Path $veraTarget 'verapdf-cli.jar') -Force
Copy-Item -LiteralPath (Join-Path $veraInstall 'documents') -Destination $veraTarget -Recurse -Force
New-Item -ItemType Directory -Force (Join-Path $resources 'profiles') | Out-Null
Copy-Item -LiteralPath (Join-Path $gsTarget 'iccprofiles\srgb.icc') -Destination (Join-Path $resources 'profiles\srgb.icc') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'dependencies.json') -Destination (Join-Path $resources 'dependencies.json') -Force

& (Join-Path $gsTarget 'bin\gswin64c.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Ghostscript incluso nel pacchetto non si avvia.' }
& $javaExe -jar (Join-Path $veraTarget 'verapdf-cli.jar') --version
if ($LASTEXITCODE -ne 0) { throw 'veraPDF incluso nel pacchetto non si avvia.' }
Write-Host "Dipendenze verificate e pronte: $resources"
