; 바탕화면 바로가기 기본 미생성.
;
; Tauri NSIS 템플릿은 마침 페이지의 "바탕화면 바로가기" 체크박스(MUI_FINISHPAGE_SHOWREADME)로
; 바로가기를 만든다(기본 체크됨). 이 훅 파일은 템플릿 상단에서 include 되므로, 여기서
; NOTCHECKED 를 선언해 체크박스를 기본 '해제' 상태로 만든다 → 일반 설치 시 미생성.
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

; 설치 후:
;  1) silent/passive 설치 시 자동 생성된 바탕화면 바로가기 제거.
;  2) Sparse Package(신원 패키지) 등록 → userNotificationListener 권한 활성화.
;     per-user 라 관리자 권한 불필요. 실패해도 설치를 막지 않음(register-identity.ps1 내부에서 처리).
!macro NSIS_HOOK_POSTINSTALL
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  nsExec::ExecToLog 'powershell -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\register-identity.ps1" -InstallDir "$INSTDIR"'
  ; 로그인 자동시작: 사용자가 끈 적 없으면(.autostart-disabled 없음) Startup 폴더 바로가기 생성.
  ; (업데이트에도 매번 보장 → '기존 자동시작 ON' 유지. 끈 사용자는 마커가 있어 건너뜀.)
  IfFileExists "$APPDATA\kr.co.plead.ddoktti-here\.autostart-disabled" +2 +1
  CreateShortcut "$SMSTARTUP\${PRODUCTNAME}.lnk" "$INSTDIR\ddoktti-here.exe" "--autostart"
!macroend

; 제거 시: 신원 패키지 등록 해제 + 자동시작 바로가기 제거(공개 인증서는 무해하여 남겨둠).
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'powershell -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -Command "Get-AppxPackage *PleadDdoktti* | Remove-AppxPackage"'
  Delete "$SMSTARTUP\${PRODUCTNAME}.lnk"
!macroend
