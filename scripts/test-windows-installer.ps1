# Disposable GitHub Windows runner only: never uses a real user's installation.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Installer smoke test is restricted to disposable GitHub-hosted runners.'
}
$out = (Resolve-Path $OutputDirectory).Path
$setup = @(Get-ChildItem $out -Filter '*-Setup.exe')
if ($setup.Count -ne 1) { throw 'Expected exactly one installer.' }
$metadata = Get-Content (Get-ChildItem $out -Filter '*-windows.json').FullName -Raw | ConvertFrom-Json
$scratch = Join-Path $env:RUNNER_TEMP ('myimage-install-' + [guid]::NewGuid().ToString('N'))
$install = Join-Path $scratch 'Application with spaces'
$registry = 'HKCU:\Software\Classes\MyImage.Installer.Document'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{B9CF57F1-A083-4B7D-BE98-B1F0C466EBB4}_is1'
if ((Test-Path $registry) -or (Test-Path $uninstallKey)) { throw 'Refusing to overwrite an existing installation.' }
$null = New-Item -ItemType Directory -Force $scratch
$sentinel = Join-Path $scratch 'Projects\keep.myimage'
$null = New-Item -ItemType Directory -Force (Split-Path $sentinel)
'test project must survive install and uninstall' | Set-Content $sentinel
$sentinelHash = (Get-FileHash $sentinel).Hash
function Run-Checked([string]$file, [string[]]$arguments) {
    $process = Start-Process -FilePath $file -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer/uninstaller returned $($process.ExitCode)" }
}
$uninstaller = Join-Path $install 'unins000.exe'
try {
    $args = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOCLOSEAPPLICATIONS',"/DIR=`"$install`"",'/TASKS=openwith')
    Run-Checked $setup[0].FullName $args
    if ((Get-FileHash (Join-Path $install 'MyImage.exe')).Hash.ToLower() -ne $metadata.executableSha256) { throw 'Installed executable does not match packaged source.' }
    $command = (Get-Item "$registry\shell\open\command").GetValue('')
    if ($command -ne "`"$install\MyImage.exe`" `"%1`"") { throw 'Incorrect file-open command quoting.' }
    if (-not (Test-Path $uninstallKey)) { throw 'Uninstall registration missing.' }
    if ((Get-Item $uninstallKey).GetValue('DisplayName') -ne 'Bediz') { throw 'Windows app display name was not updated.' }
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Bediz.lnk'
    $desktopShortcut = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'Bediz.lnk'
    if (-not (Test-Path $shortcut)) { throw 'Start menu shortcut missing.' }
    if (Test-Path $desktopShortcut) { throw 'Installer unexpectedly enabled the optional desktop shortcut.' }
    if ((Get-Item $run -ErrorAction SilentlyContinue) -and (Get-Item $run).GetValue('MyImage')) { throw 'Installer unexpectedly enabled login startup.' }
    # Unknown user files inside the installation directory must also survive.
    $localProject = Join-Path $install 'user-project.myimage'
    'keep local project' | Set-Content $localProject
    # Simulate both previous names in both installer-owned shortcut locations.
    $legacyShortcuts = @()
    foreach ($folder in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('DesktopDirectory'))) {
        $null = New-Item -ItemType Directory -Path $folder -Force
        foreach ($name in @('MyImage', 'SkyImage')) {
            $legacyShortcut = Join-Path $folder "$name.lnk"
            Copy-Item $shortcut $legacyShortcut
            $legacyShortcuts += $legacyShortcut
        }
    }
    $upgradeArgs = @($args | Where-Object { -not $_.StartsWith('/TASKS=') }) + '/TASKS=openwith,desktopicon'
    Run-Checked $setup[0].FullName $upgradeArgs
    foreach ($legacyShortcut in $legacyShortcuts) {
        if (Test-Path $legacyShortcut) { throw "Update left a duplicate legacy shortcut: $legacyShortcut" }
    }
    if (-not (Test-Path $shortcut) -or -not (Test-Path $desktopShortcut)) { throw 'Update did not create the requested Bediz shortcuts.' }
    if (-not (Test-Path $localProject)) { throw 'Update removed a project.' }
    # Simulate the optional login setting; uninstall must remove only this copy.
    $null = New-Item -Path $run -Force
    $null = New-ItemProperty -Path $run -Name MyImage -Value "`"$install\MyImage.exe`"" -PropertyType String
    Run-Checked $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART')
    if ((Test-Path (Join-Path $install 'MyImage.exe')) -or (Test-Path $registry) -or (Test-Path $uninstallKey) -or (Test-Path $shortcut) -or (Test-Path $desktopShortcut)) { throw 'Uninstall left installed files or integration behind.' }
    if (-not (Test-Path $localProject) -or (Get-FileHash $sentinel).Hash -ne $sentinelHash) { throw 'Uninstall changed project data.' }
    if ((Get-Item $run).GetValue('MyImage')) { throw 'Uninstall left this copy registered for login startup.' }
    foreach ($ext in @('.myimage','.png','.jpg','.jpeg','.webp','.bmp','.gif','.tif','.tiff')) {
        $key = Get-Item "HKCU:\Software\Classes\$ext\OpenWithProgids" -ErrorAction SilentlyContinue
        if ($key -and $key.GetValueNames().Contains('MyImage.Installer.Document')) { throw 'Open with integration not removed.' }
    }
    # A different portable copy's startup entry must not be removed.
    Run-Checked $setup[0].FullName $args
    $other = '"C:\OtherMyImage\MyImage.exe"'
    $null = New-ItemProperty -Path $run -Name MyImage -Value $other -PropertyType String
    Run-Checked $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART')
    if ((Get-Item $run).GetValue('MyImage') -ne $other) { throw 'Uninstall altered another copy startup entry.' }
    Remove-ItemProperty -Path $run -Name MyImage
    'PASS install, upgrade, MyImage/SkyImage shortcut migration, Bediz branding, binary identity, optional desktop shortcut, file-open quoting, startup off, targeted startup cleanup, uninstall and project preservation.'
} finally {
    if (Test-Path $uninstaller) { Run-Checked $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') }
    Remove-Item $scratch -Recurse -Force
}
