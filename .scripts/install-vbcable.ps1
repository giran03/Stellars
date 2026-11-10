param(
    [switch]$ForceRepair,
    [switch]$SkipHiFiCable
)

$ErrorActionPreference = "Stop"

# Self-elevate if not running as Administrator
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "Requesting Administrator privileges to install/repair WHQL VB-CABLE drivers..." -ForegroundColor Cyan
    $argList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"")
    if ($ForceRepair) { $argList += "-ForceRepair" }
    if ($SkipHiFiCable) { $argList += "-SkipHiFiCable" }
    Start-Process -FilePath "powershell.exe" -ArgumentList $argList -Verb RunAs -Wait
    exit $LASTEXITCODE
}

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$staging = Join-Path $env:LOCALAPPDATA "StellarsSonar\drivers"
New-Item -ItemType Directory -Force -Path $staging | Out-Null

Write-Host "=== Stellars Virtual Cable Installer & Repair ===" -ForegroundColor Magenta

# 1. Re-enable any disabled VB-Audio devices
$disabled = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
    $_.FriendlyName -match "VB-Audio|CABLE Input|CABLE Output|Hi-Fi Cable|Stellars" -and $_.Status -ne "OK"
}
foreach ($dev in $disabled) {
    Write-Host "Re-enabling $($dev.FriendlyName)..." -ForegroundColor Yellow
    Enable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
}

# 2. Install / Repair VB-CABLE (Primary Virtual Cable)
$vbPresent = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object {
    $_.FriendlyName -match "VB-Audio Virtual Cable|CABLE Input|Stellars Virtual Audio Cable" -or $_.InstanceId -match "VBAudioVACWDM"
}

if (-not $vbPresent -or $ForceRepair) {
    $vbZip = Join-Path $staging "VBCABLE_Driver_Pack.zip"
    $vbDir = Join-Path $staging "vbcable"
    if (-not (Test-Path (Join-Path $vbDir "VBCABLE_Setup_x64.exe"))) {
        New-Item -ItemType Directory -Force -Path $vbDir | Out-Null
        Write-Host "Downloading official WHQL VB-CABLE driver package..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack46.zip" -OutFile $vbZip -UseBasicParsing
        Expand-Archive -Path $vbZip -DestinationPath $vbDir -Force
    }
    $infPath = Join-Path $vbDir "vbMmeCable64_win10.inf"
    $setupExe = Join-Path $vbDir "VBCABLE_Setup_x64.exe"
    if (Test-Path $infPath) {
        Write-Host "Staging vbMmeCable64_win10.inf via pnputil..." -ForegroundColor Cyan
        pnputil.exe /add-driver $infPath /install | Out-Null
    }
    if (-not $vbPresent -and (Test-Path $setupExe)) {
        Write-Host "Running VBCABLE_Setup_x64.exe -i -h..." -ForegroundColor Cyan
        Start-Process -FilePath $setupExe -ArgumentList "-i","-h" -WorkingDirectory $vbDir -Wait -WindowStyle Hidden
    }
    Write-Host "VB-Audio Virtual Cable ready." -ForegroundColor Green
} else {
    Write-Host "VB-Audio Virtual Cable is already installed and active." -ForegroundColor Green
}

# 3. Install / Repair VB-Audio Hi-Fi Cable (Secondary Virtual Cable for Stream + Mic separation)
if (-not $SkipHiFiCable) {
    $hifiPresent = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object {
        $_.FriendlyName -match "Hi-Fi Cable|Stellars Hi-Fi" -or $_.InstanceId -match "VBAudioHFVAIO"
    }
    if (-not $hifiPresent -or $ForceRepair) {
        $hifiZip = Join-Path $staging "HiFiCableAsioBridgeSetup.zip"
        $hifiDir = Join-Path $staging "hificable"
        $hifiExe = Join-Path $hifiDir "HiFiCableAsioBridgeSetup.exe"
        if (-not (Test-Path $hifiExe)) {
            New-Item -ItemType Directory -Force -Path $hifiDir | Out-Null
            Write-Host "Downloading official WHQL VB-Audio Hi-Fi Cable package..." -ForegroundColor Cyan
            Invoke-WebRequest -Uri "https://download.vb-audio.com/Download_CABLE/HiFiCableAsioBridgeSetup_v1007.zip" -OutFile $hifiZip -UseBasicParsing
            Expand-Archive -Path $hifiZip -DestinationPath $hifiDir -Force
        }
        if (-not $hifiPresent -and (Test-Path $hifiExe)) {
            Write-Host "Running HiFiCableAsioBridgeSetup.exe -i -h..." -ForegroundColor Cyan
            Start-Process -FilePath $hifiExe -ArgumentList "-i","-h" -WorkingDirectory $hifiDir -Wait -WindowStyle Hidden
        }
        Write-Host "VB-Audio Hi-Fi Cable ready." -ForegroundColor Green
    } else {
        Write-Host "VB-Audio Hi-Fi Cable is already installed and active." -ForegroundColor Green
    }
}

pnputil.exe /scan-devices | Out-Null

# 4. Apply System-Wide Stellars Endpoint Names in Windows MMDevices Property Store
$rights = [System.Security.AccessControl.RegistryRights]::SetValue -bor [System.Security.AccessControl.RegistryRights]::QueryValues
foreach ($flow in @("Render", "Capture")) {
    $baseKey = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$flow"
    $flowKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($baseKey)
    if ($null -eq $flowKey) { continue }
    foreach ($guid in $flowKey.GetSubKeyNames()) {
        $propSub = "$baseKey\$guid\Properties"
        $pk = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propSub, [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree, $rights)
        if ($null -eq $pk) { continue }
        try {
            $hw = [string]$pk.GetValue("{a8b865dd-2e3d-4094-ad97-e593a70c75d6},8")
            $ks = [string]$pk.GetValue("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},8")
            $curDesc = [string]$pk.GetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2")
            if ($hw -match "VBAudioVACWDM") {
                $desc = if ($flow -eq "Capture") { "Stellars - Microphone" } elseif ($ks -match "DFF21FE3" -or $curDesc -match "16") { "Stellars - Aux In 16ch" } else { "Stellars - Virtual Mic In" }
                $pk.SetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2", $desc, [Microsoft.Win32.RegistryValueKind]::String)
                $pk.SetValue("{b3f8fa53-0004-438e-9003-51a46e139bfc},6", "Stellars Virtual Audio Cable", [Microsoft.Win32.RegistryValueKind]::String)
                Write-Host "Renamed $flow endpoint -> $desc (Stellars Virtual Audio Cable)" -ForegroundColor Cyan
            } elseif ($hw -match "VBAudioHFVAIO") {
                $desc = if ($flow -eq "Capture") { "Stellars - Stream Mix" } else { "Stellars - Stream Mix In" }
                $pk.SetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2", $desc, [Microsoft.Win32.RegistryValueKind]::String)
                $pk.SetValue("{b3f8fa53-0004-438e-9003-51a46e139bfc},6", "Stellars Hi-Fi Virtual Cable", [Microsoft.Win32.RegistryValueKind]::String)
                Write-Host "Renamed $flow endpoint -> $desc (Stellars Hi-Fi Virtual Cable)" -ForegroundColor Cyan
            }
        } finally {
            $pk.Close()
        }
    }
    $flowKey.Close()
}

Write-Host "Virtual cable setup complete!" -ForegroundColor Green
