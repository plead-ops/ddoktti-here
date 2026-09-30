; 바탕화면 바로가기 기본 미생성.
;
; Tauri NSIS 템플릿은 마침 페이지의 "바탕화면 바로가기" 체크박스(MUI_FINISHPAGE_SHOWREADME)로
; 바로가기를 만든다(기본 체크됨). 이 훅 파일은 템플릿 상단에서 include 되므로, 여기서
; NOTCHECKED 를 선언해 체크박스를 기본 '해제' 상태로 만든다 → 일반 설치 시 미생성.
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

; 구버전 Windows 알림 접근용 패키지만 정리한다. 현재 앱은 OAuth를 사용하므로
; 새 설치/현행 버전 재설치에서는 PowerShell 자체를 실행하지 않는다.
; 구버전의 리소스 매핑은 register-identity.ps1을 $INSTDIR 바로 아래에 놓았다.
; 정리가 실패하면 파일을 남겨 다음 설치에서 재시도한다. 인증서 저장소는 건드리지 않는다.
!macro DDOKTTI_CLEANUP_LEGACY_IDENTITY PREFIX
  IfFileExists "$INSTDIR\register-identity.ps1" ${PREFIX}_cleanup ${PREFIX}_done
  ${PREFIX}_cleanup:
    Push $0
    nsExec::ExecToLog 'powershell -NoProfile -NonInteractive -WindowStyle Hidden -Command "try { Get-AppxPackage -Name PleadDdoktti -ErrorAction Stop | Remove-AppxPackage -ErrorAction Stop; exit 0 } catch { exit 1 }"'
    Pop $0
    StrCmp $0 "0" 0 ${PREFIX}_restore
    Delete "$INSTDIR\register-identity.ps1"
    Delete "$INSTDIR\unregister-identity.ps1"
    Delete "$INSTDIR\ddoktti-identity.msix"
    Delete "$INSTDIR\ddoktti-cert.cer"
  ${PREFIX}_restore:
    Pop $0
  ${PREFIX}_done:
!macroend

; 설치 후: silent/passive 설치 시 자동 생성된 바탕화면 바로가기 제거.
!macro NSIS_HOOK_POSTINSTALL
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  !insertmacro DDOKTTI_CLEANUP_LEGACY_IDENTITY "ddoktti_install"
  ; 로그인 자동시작: 사용자가 끈 적 없으면(.autostart-disabled 없음) HKCU Run 키 등록.
  ; (업데이트에도 매번 보장 → '기존 자동시작 ON' 유지. 끈 사용자는 마커가 있어 건너뜀.)
  IfFileExists "$APPDATA\kr.co.plead.ddoktti-here\.autostart-disabled" +2 +1
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DdoktiHere" '"$INSTDIR\ddoktti-here.exe" --autostart'
!macroend

; 제거 시: 남은 구버전 신원 패키지만 정리 + 자동시작 Run 키 제거.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro DDOKTTI_CLEANUP_LEGACY_IDENTITY "ddoktti_uninstall"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DdoktiHere"
!macroend
