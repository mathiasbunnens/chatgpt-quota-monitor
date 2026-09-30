; Per-user startup registration. No elevation or scheduled task is needed.
!include nsDialogs.nsh
!include LogicLib.nsh

Var QuotaStartupLoaded
Var QuotaStartupEnabled
Var QuotaStartupCheckbox
Var QuotaStartupDialog

LangString QuotaStartupTitle 1033 "Start with Windows"
LangString QuotaStartupTitle 1036 "Démarrage avec Windows"
LangString QuotaStartupDescription 1033 "Keep your Codex quota available after signing in."
LangString QuotaStartupDescription 1036 "Retrouvez votre quota Codex après chaque connexion."
LangString QuotaStartupLabel 1033 "Launch Quota Codex when I sign in (recommended)"
LangString QuotaStartupLabel 1036 "Lancer Quota Codex à l'ouverture de session (recommandé)"
LangString QuotaStartupHint 1033 "The app starts in the system tray, without opening its window or a terminal. You can disable it later in Windows Settings > Apps > Startup."
LangString QuotaStartupHint 1036 "L'application démarre dans la zone de notification, sans fenêtre ni terminal. Vous pouvez la désactiver dans les Paramètres Windows > Applications > Démarrage."

; Hook files are included before Tauri's standard pages. This explicit choice
; is therefore the first page; passive/silent updates skip the UI.
Page custom QuotaStartupPage QuotaStartupLeave

Function QuotaStartupLoad
  ${If} $QuotaStartupLoaded == 1
    Return
  ${EndIf}
  StrCpy $QuotaStartupLoaded 1
  StrCpy $QuotaStartupEnabled 1
  ClearErrors
  ReadRegDWORD $0 HKCU "Software\mathiasbunnens\Quota Codex" "StartupEnabled"
  ${IfNot} ${Errors}
    ${If} $0 == 0
      StrCpy $QuotaStartupEnabled 0
    ${EndIf}
  ${EndIf}
  ; Explicit silent-install overrides; otherwise upgrades preserve the choice.
  ClearErrors
  ${GetOptions} $CMDLINE "/AUTOSTART=" $0
  ${IfNot} ${Errors}
    ${If} $0 == "0"
      StrCpy $QuotaStartupEnabled 0
    ${ElseIf} $0 == "1"
      StrCpy $QuotaStartupEnabled 1
    ${EndIf}
  ${EndIf}
FunctionEnd

Function QuotaStartupPage
  Call QuotaStartupLoad
  IfSilent quota_startup_skip
  ClearErrors
  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Goto quota_startup_skip
  ${EndIf}
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    Goto quota_startup_skip
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(QuotaStartupTitle)" "$(QuotaStartupDescription)"
  nsDialogs::Create 1018
  Pop $QuotaStartupDialog
  ${If} $QuotaStartupDialog == error
    Abort
  ${EndIf}
  ${NSD_CreateCheckbox} 0 15u 100% 24u "$(QuotaStartupLabel)"
  Pop $QuotaStartupCheckbox
  ${NSD_SetState} $QuotaStartupCheckbox $QuotaStartupEnabled
  ${NSD_CreateLabel} 0 50u 100% 55u "$(QuotaStartupHint)"
  Pop $0
  nsDialogs::Show
  Return
  quota_startup_skip:
  Abort
FunctionEnd

Function QuotaStartupLeave
  ${NSD_GetState} $QuotaStartupCheckbox $QuotaStartupEnabled
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  Call QuotaStartupLoad
  ${If} $QuotaStartupEnabled == 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" --autostart'
  ${Else}
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}
  WriteRegDWORD HKCU "${MANUPRODUCTKEY}" "StartupEnabled" $QuotaStartupEnabled
!macroend

; Tauri's standard uninstaller already removes the product's Run value on a
; real uninstall, and preserves it during /UPDATE. Its app-data removal also
; clears StartupEnabled. Never alter Windows' StartupApproved override.
