; Screenshot Studio NSIS installer hooks (phase 2.3), wired by tauri.conf.json `bundle.windows.nsis.installerHooks`.
;
; The stock Tauri installer (tauri-bundler 2.10 installer.nsi) creates a desktop shortcut on every silent (/S) or
; passive (/P) install, and its only switch against that (/NS) also drops the Start Menu shortcut. On the owner's
; machine the Desktop is redirected into OneDrive (C:\Users\Nghaiz\OneDrive\Desktop) and nothing may write into the
; OneDrive folder, so a silent install must make the Start Menu shortcut only:
;   PREINSTALL   remember the caller's /NS, then force no-shortcut mode: the template skips BOTH shortcuts;
;   POSTINSTALL  restore it and create the Start Menu shortcut alone (still skipped for /NS and /UPDATE runs).
; Windows Search finds the app through `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Screenshot Studio.lnk`.
; The GUI installer's finish page keeps its "Create desktop shortcut" checkbox: an explicit user choice.

Var SsCallerNoShortcut

!macro NSIS_HOOK_PREINSTALL
  StrCpy $SsCallerNoShortcut $NoShortcutMode
  StrCpy $NoShortcutMode 1
!macroend

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $NoShortcutMode $SsCallerNoShortcut
  Call CreateOrUpdateStartMenuShortcut
!macroend
