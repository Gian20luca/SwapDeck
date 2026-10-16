; Hook di NSIS per SwapDeck (bundle.windows.nsis.installerHooks in tauri.conf.json).
; La disinstallazione toglie la registrazione per utente delle notifiche: chiave dell'AUMID e
; icona (crates/swapdeck-core/src/headless/os_notify.rs, AUMID_KEY e ICON_FILE). I dati restano.

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\com.swapdeck.desktop"
  Delete "$APPDATA\swapdeck\notify-icon.png"
!macroend
