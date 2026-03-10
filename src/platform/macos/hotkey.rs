#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn setup_hotkey(cx: &mut App) {
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(err) => {
            eprintln!("warning: failed to create global hotkey manager: {err}");
            return;
        }
    };
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
    if let Err(err) = manager.register(hotkey) {
        eprintln!("warning: failed to register Option+Space hotkey: {err}");
        return;
    }

    cx.set_global(HotkeyRegistration {
        _manager: manager,
        hotkey_id: hotkey.id(),
    });
}
