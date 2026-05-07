#!/usr/bin/env powershell

# Configures the Windows OpenSSH Authentication Agent so that Git can use SSH
# keys with passphrases without re-prompting every time.  After this runs you
# only need to enter your passphrase once per boot (or once ever, since the
# agent persists keys across sessions).
#
# Steps (from https://stackoverflow.com/a/63787232):
#   1. Enable & start the ssh-agent Windows service (needs elevation).
#   2. Point Git at Windows' built-in ssh.exe instead of Git-for-Windows'.
#   3. Ensure ~/.ssh/config has AddKeysToAgent yes.
#   4. Add the user's default SSH key to the agent (interactive passphrase prompt).

$ErrorActionPreference = 'Stop'

# --- 1. ssh-agent service ---------------------------------------------------
$sshAgent = Get-Service -Name ssh-agent -ErrorAction SilentlyContinue
if ($sshAgent -and ($sshAgent.StartType -ne 'Automatic' -or $sshAgent.Status -ne 'Running')) {
    Write-Output 'Configuring OpenSSH Authentication Agent (UAC prompt may appear)...'
    Start-Process -Verb RunAs -FilePath powershell.exe -ArgumentList '-NoProfile', '-Command', `
        'Set-Service ssh-agent -StartupType Automatic; Start-Service ssh-agent' `
        -Wait
} elseif (-not $sshAgent) {
    Write-Warning 'OpenSSH Authentication Agent service not found. You may need to enable the OpenSSH optional feature.'
}

# --- 2. Git core.sshCommand --------------------------------------------------
$windowsSsh = 'C:/Windows/System32/OpenSSH/ssh.exe'
if (Test-Path $windowsSsh) {
    git config --global core.sshCommand $windowsSsh
    Write-Output "Set git core.sshCommand to $windowsSsh"
} else {
    Write-Warning "Windows OpenSSH not found at $windowsSsh"
}

# --- 3. Pick SSH key and add to agent ----------------------------------------
$sshDir = "$env:USERPROFILE\.ssh"
$sshConfigPath = "$sshDir\config"
if (-not (Test-Path $sshDir)) {
    New-Item -ItemType Directory -Path $sshDir -Force | Out-Null
}
$sshKeys = Get-ChildItem "$env:USERPROFILE\.ssh" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Extension -eq '' -and $_.Name -notmatch '^(config|known_hosts|authorized_keys)$' -and
                   (Test-Path "$($_.FullName).pub") }

$chosenKeyPath = $null
if ($sshKeys.Count -eq 0) {
    Write-Error 'No SSH keys found in ~/.ssh/. Please generate one first with: ssh-keygen -t ed25519'
    exit 1
}

# Check which keys are already loaded in the agent.
$loadedKeys = ssh-add -l 2>&1

Write-Output 'Available SSH keys:'
for ($i = 0; $i -lt $sshKeys.Count; $i++) {
    $pub = Get-Content "$($sshKeys[$i].FullName).pub" -ErrorAction SilentlyContinue
    $alreadyLoaded = $pub -and ($loadedKeys | Select-String -SimpleMatch ($pub.Split(' ')[1]) -Quiet)
    $suffix = if ($alreadyLoaded) { ' (already in agent)' } else { '' }
    Write-Output "  [$i] $($sshKeys[$i].Name)$suffix"
}
$choice = Read-Host 'Enter the number of the key to add (or press Enter to skip)'
if ($choice -ne '' -and $choice -match '^\d+$' -and [int]$choice -lt $sshKeys.Count) {
    $chosenKeyPath = $sshKeys[[int]$choice].FullName
    $pub = Get-Content "$chosenKeyPath.pub" -ErrorAction SilentlyContinue
    $alreadyLoaded = $pub -and ($loadedKeys | Select-String -SimpleMatch ($pub.Split(' ')[1]) -Quiet)
    if ($alreadyLoaded) {
        Write-Output "Key $($sshKeys[[int]$choice].Name) is already loaded in the agent. Skipping ssh-add."
    } else {
        Write-Output "Adding SSH key: $chosenKeyPath (you will be prompted for your passphrase)"
        ssh-add $chosenKeyPath
    }
} else {
    Write-Output 'Skipped ssh-add.'
}

# --- 4. ~/.ssh/config --------------------------------------------------------
if (Test-Path $sshConfigPath) {
    $existing = Get-Content $sshConfigPath -Raw
    if ($existing -match 'AddKeysToAgent') {
        Write-Output '~/.ssh/config already has AddKeysToAgent configured.'
        exit 0
    }
}

$configLines = @(
    'Host *',
    '    AddKeysToAgent yes',
    '    IdentitiesOnly yes'
)

if ($chosenKeyPath) {
    $ghUser = Read-Host 'Enter your GitHub username (or press Enter to skip)'
    $identityFile = '~/.ssh/' + (Split-Path $chosenKeyPath -Leaf)
    $configLines += ''
    $configLines += 'Host github.com'
    $configLines += '    HostName github.com'
    if ($ghUser -ne '') {
        $configLines += "    User $ghUser"
    } else {
        $configLines += '    User git'
    }
    $configLines += "    IdentityFile $identityFile"
}

if (Test-Path $sshConfigPath) {
    Add-Content -Path $sshConfigPath -Value ("`n" + ($configLines -join "`n"))
    Write-Output 'Appended SSH config to ~/.ssh/config'
} else {
    Set-Content -Path $sshConfigPath -Value ($configLines -join "`n")
    Write-Output 'Created ~/.ssh/config'
}
