<#
  설치 직후 실행(NSIS POSTINSTALL). per-user, 관리자 권한 불필요.
  1) 자체 서명 공개 인증서를 CurrentUser\TrustedPeople 에 등록(MSIX 신뢰).
  2) 신원 패키지(Sparse Package)를 외부 위치=설치폴더로 등록.
  실패해도 설치를 막지 않는다(앱은 그대로 실행되며 신원만 빠짐) — 로그만 남김.
#>
param([Parameter(Mandatory = $true)][string]$InstallDir)

# 리소스가 $INSTDIR 직하 또는 하위 폴더(resources\ 등)에 놓일 수 있어 양쪽 모두 탐색.
function Resolve-Asset([string]$name) {
  $direct = Join-Path $InstallDir $name
  if (Test-Path $direct) { return $direct }
  $found = Get-ChildItem -Path $InstallDir -Filter $name -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($found) { return $found.FullName }
  return $direct
}

$cer = Resolve-Asset 'ddoktti-cert.cer'
$msix = Resolve-Asset 'ddoktti-identity.msix'

# 자체 서명 인증서는 잎=루트가 동일 → MSIX 체인 검증이 루트까지 확인한다.
# TrustedPeople(서명자 신뢰) + Root(루트 신뢰) 둘 다 등록해야 0x800B0109 를 피한다.
# CurrentUser 스토어라 관리자 권한 불필요, Import-Certificate 는 프롬프트도 없음.
foreach ($store in @('Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPeople')) {
  try {
    Import-Certificate -FilePath $cer -CertStoreLocation $store -ErrorAction Stop | Out-Null
    Write-Host "cert imported to $store"
  } catch {
    Write-Host "cert import to $store failed: $($_.Exception.Message)"
  }
}

# 같은 이름의 기존 등록을 먼저 제거(동일 버전 재설치/다운그레이드 충돌 방지)
try { Get-AppxPackage *PleadDdoktti* | Remove-AppxPackage -ErrorAction SilentlyContinue } catch {}

try {
  Add-AppxPackage -Path $msix -ExternalLocation $InstallDir -ForceApplicationShutdown -ErrorAction Stop
  Write-Host "identity package registered"
} catch {
  Write-Host "identity registration failed: $($_.Exception.Message)"
}
