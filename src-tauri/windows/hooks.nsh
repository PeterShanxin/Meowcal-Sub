; Meowcal Core keeps the translation engine outside this app's identifier
; folders, so "Delete the application data" does not reach it on its own.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$LOCALAPPDATA\Meowcal\Core\sub1\production"
    RmDir "$LOCALAPPDATA\Meowcal\Core\sub1"
    ; Core releases before the per-app level stored every app's engine here.
    ; Meowcal Sub 2 may still run from it, so keep it once that app has run.
    ${IfNot} ${FileExists} "$APPDATA\com.meowcal.sub2\*.*"
    ${AndIfNot} ${FileExists} "$LOCALAPPDATA\com.meowcal.sub2\*.*"
    ${AndIfNot} ${FileExists} "$APPDATA\meowcal-sub-2\*.*"
      RmDir /r "$LOCALAPPDATA\Meowcal\Core\production"
    ${EndIf}
    RmDir "$LOCALAPPDATA\Meowcal\Core"
    RmDir "$LOCALAPPDATA\Meowcal"
  ${EndIf}
!macroend
