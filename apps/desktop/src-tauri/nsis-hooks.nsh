; 바탕화면 바로가기 기본 미생성.
;
; Tauri NSIS 템플릿은 마침 페이지의 "바탕화면 바로가기" 체크박스(MUI_FINISHPAGE_SHOWREADME)로
; 바로가기를 만든다(기본 체크됨). 이 훅 파일은 템플릿 상단에서 include 되므로, 여기서
; NOTCHECKED 를 선언해 체크박스를 기본 '해제' 상태로 만든다 → 일반 설치 시 미생성.
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

; silent/passive 설치는 체크박스 없이 자동 생성하므로, 설치 직후 제거한다.
; (바로가기 파일명은 템플릿 기준 "$DESKTOP\${PRODUCTNAME}.lnk")
!macro NSIS_HOOK_POSTINSTALL
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
!macroend
