; NSIS 인스톨러 훅 — 바탕화면 바로가기 생성 안 함.
; Tauri 기본 템플릿이 설치 섹션에서 만드는 바탕화면 바로가기를 설치 직후 제거한다.
; (Tauri 에 createDesktopShortcut 옵션이 없어 훅으로 처리: PRODUCTNAME/MAINBINARYNAME 둘 다 대응)
!macro NSIS_HOOK_POSTINSTALL
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  Delete "$DESKTOP\${MAINBINARYNAME}.lnk"
!macroend
