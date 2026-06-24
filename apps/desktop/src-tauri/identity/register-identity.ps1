<#
  설치 직후 실행(NSIS POSTINSTALL).
  자체 서명 인증서는 잎=루트가 동일 → MSIX 체인 검증이 "신뢰된 루트"를 요구한다.
  CurrentUser\Root 추가는 Windows 가 보안창을 강제(비대화형 설치선 실패)하므로,
  머신 루트(LocalMachine\Root)에 1회만 UAC 승격으로 등록한다.
  - 첫 설치: 인증서가 머신 루트에 없으면 UAC 승격 후 등록(관리자면 프롬프트 없이 UAC 만).
  - 업데이트/재설치: 이미 신뢰되어 있으면 승격 생략 → Add-AppxPackage(per-user)만 조용히.
  실패해도 설치를 막지 않는다(앱은 그대로 실행되며 신원만 빠짐) — 로그만 남김.
#>
param([Parameter(Mandatory = $true)][string]$InstallDir)

# 설치 결과를 진단 리포트가 읽을 수 있도록 로그 파일에도 남긴다(앱 local data\logs\install.log).
$LogFile = Join-Path $env:LOCALAPPDATA 'kr.co.plead.ddoktti-here\logs\install.log'
function Log([string]$m) {
  Write-Host $m
  try {
    $dir = Split-Path $LogFile
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $stamp = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
    Add-Content -Path $LogFile -Value "$stamp  $m" -Encoding utf8
  } catch {}
}

# 리소스가 $INSTDIR 직하 또는 하위 폴더에 놓일 수 있어 양쪽 모두 탐색.
function Resolve-Asset([string]$name) {
  $direct = Join-Path $InstallDir $name
  if (Test-Path $direct) { return $direct }
  $found = Get-ChildItem -Path $InstallDir -Filter $name -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($found) { return $found.FullName }
  return $direct
}

$cer = Resolve-Asset 'ddoktti-cert.cer'
$msix = Resolve-Asset 'ddoktti-identity.msix'

# 1) 인증서가 머신 루트에 이미 신뢰되어 있는지 thumbprint 로 확인.
$trusted = $false
try {
  $c = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 $cer
  $trusted = Test-Path ("Cert:\LocalMachine\Root\" + $c.Thumbprint)
} catch {}

# 2) 신뢰돼 있지 않으면 UAC 승격으로 머신 루트+TrustedPeople 에 1회 등록.
#    (EncodedCommand 로 인용부호 문제 회피. LocalMachine\Root 는 관리자 컨텍스트라 보안창 없음.)
if (-not $trusted) {
  $inner = "Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\Root | Out-Null; " +
           "Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null"
  $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($inner))
  try {
    Start-Process -FilePath 'powershell' -Verb RunAs -Wait -WindowStyle Hidden `
      -ArgumentList @('-NoProfile', '-NonInteractive', '-EncodedCommand', $enc)
    Log "cert trusted in LocalMachine\Root via elevation"
  } catch {
    Log "elevation/cert trust failed or declined: $($_.Exception.Message)"
  }
}

# 3) 같은 이름의 기존 등록 제거(동일 버전 재설치/다운그레이드 충돌 방지) 후 등록(per-user).
try { Get-AppxPackage *PleadDdoktti* | Remove-AppxPackage -ErrorAction SilentlyContinue } catch {}
try {
  Add-AppxPackage -Path $msix -ExternalLocation $InstallDir -ForceApplicationShutdown -ErrorAction Stop
  Log "identity package registered"
} catch {
  Log "identity registration failed: $($_.Exception.Message)"
}

# 4) 구버전(플러그인) Run 자동시작 잔재 제거 후, nsis 가 표준 키를 다시 등록(이중 실행 방지).
#    주의: 이 .ps1 은 PowerShell -File 로 ANSI(CP949)로 읽혀 한글 문자열 리터럴이 깨진다 → ASCII 만 사용.
#    옛 항목도 값(경로)에 'ddoktti-here' 가 들어가므로 ASCII 로 잡힌다.
try {
  $runKey = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
  $rp = Get-ItemProperty $runKey -ErrorAction SilentlyContinue
  if ($rp) {
    $rp.PSObject.Properties |
      Where-Object { $_.Name -notmatch '^PS' -and ($_.Name -match 'ddoktti' -or $_.Value -match 'ddoktti-here') } |
      ForEach-Object {
        Remove-ItemProperty -Path $runKey -Name $_.Name -ErrorAction SilentlyContinue
        Log ("removed legacy Run autostart: " + $_.Name)
      }
  }
} catch {}
