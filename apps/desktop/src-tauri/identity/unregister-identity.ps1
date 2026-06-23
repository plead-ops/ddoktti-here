<#
  제거 시 실행(NSIS uninstall). 신원 패키지 등록 해제.
  공개 인증서는 무해하므로 남겨둔다(원하면 별도 정리 가능).
#>
$ErrorActionPreference = 'SilentlyContinue'
Get-AppxPackage *PleadDdoktti* | Remove-AppxPackage
