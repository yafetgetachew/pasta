#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn configure_background_mode() {
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
    }
}

#[cfg(target_os = "macos")]
fn menu_action_handler_class() -> Option<*const Class> {
    static CLASS: OnceLock<Option<usize>> = OnceLock::new();
    let class = CLASS.get_or_init(|| unsafe {
        if let Some(existing) = Class::get("PastaMenuActionHandler") {
            return Some((existing as *const Class) as usize);
        }

        let superclass = class!(NSObject);
        let Some(mut decl) = ClassDecl::new("PastaMenuActionHandler", superclass) else {
            eprintln!("warning: failed to create PastaMenuActionHandler class");
            return None;
        };
        decl.add_method(
            sel!(menuAction:),
            menu_action as extern "C" fn(&Object, Sel, id),
        );
        Some((decl.register() as *const Class) as usize)
    });
    class.as_ref().map(|class| *class as *const Class)
}

#[cfg(target_os = "macos")]
extern "C" fn menu_action(_this: &Object, _cmd: Sel, sender: id) {
    unsafe {
        let tag: isize = msg_send![sender, tag];
        let command = menu_command_from_tag(tag);
        if let (Some(command), Some(tx)) = (command, MENU_COMMAND_TX.get()) {
            let _ = tx.send(command);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn menu_command_from_tag(tag: isize) -> Option<MenuCommand> {
    if tag == MENU_TAG_SHOW {
        return Some(MenuCommand::ShowLauncher);
    }
    if tag == MENU_TAG_QUIT {
        return Some(MenuCommand::QuitApp);
    }

    if (MENU_TAG_FONT_BASE..MENU_TAG_FONT_BASE + FontChoice::ALL.len() as isize).contains(&tag) {
        let index = (tag - MENU_TAG_FONT_BASE) as usize;
        return FontChoice::ALL
            .get(index)
            .copied()
            .map(MenuCommand::SetFont);
    }

    if tag == MENU_TAG_ABOUT {
        return Some(MenuCommand::ShowAbout);
    }

    if tag == MENU_TAG_SYNTAX_ON {
        return Some(MenuCommand::SetSyntaxHighlighting(true));
    }

    if tag == MENU_TAG_SYNTAX_OFF {
        return Some(MenuCommand::SetSyntaxHighlighting(false));
    }

    if tag == MENU_TAG_SECRET_CLEAR_ON {
        return Some(MenuCommand::SetSecretAutoClear(true));
    }

    if tag == MENU_TAG_SECRET_CLEAR_OFF {
        return Some(MenuCommand::SetSecretAutoClear(false));
    }

    if tag == MENU_TAG_BRAIN_ON {
        return Some(MenuCommand::SetPastaBrain(true));
    }

    if tag == MENU_TAG_BRAIN_OFF {
        return Some(MenuCommand::SetPastaBrain(false));
    }

    if tag == MENU_TAG_BRAIN_DOWNLOAD {
        return Some(MenuCommand::DownloadBrain);
    }

    None
}

#[cfg(target_os = "macos")]
fn menu_item(title: &str, key: &str, target: id, action: Sel, tag: isize) -> id {
    unsafe {
        let title = NSString::alloc(nil).init_str(title);
        let key = NSString::alloc(nil).init_str(key);
        let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(title, action, key);
        if target != nil {
            NSMenuItem::setTarget_(item, target);
        }
        let _: () = msg_send![item, setTag: tag];
        item
    }
}



#[cfg(target_os = "macos")]
pub(crate) fn setup_status_item(cx: &mut App) {
    unsafe {
        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSVariableStatusItemLength);
        let button = status_item.button();
        let menu = NSMenu::new(nil);
        let Some(handler_class) = menu_action_handler_class() else {
            eprintln!(
                "warning: status menu unavailable (unable to register Objective-C action handler)"
            );
            return;
        };
        let handler: id = msg_send![handler_class, new];
        if handler == nil {
            eprintln!("warning: status menu unavailable (unable to create menu action handler)");
            return;
        }

        let show_item = menu_item(
            "Show Pasta",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SHOW,
        );
        menu.addItem_(show_item);

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let about_item = menu_item(
            "About Pasta",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_ABOUT,
        );
        menu.addItem_(about_item);

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let font_parent = menu_item("Font", "", handler, selector("menuAction:"), -1);
        let font_menu = NSMenu::new(nil);
        for (ix, choice) in FontChoice::ALL.into_iter().enumerate() {
            let tag = MENU_TAG_FONT_BASE + ix as isize;
            let item = menu_item(choice.label(), "", handler, selector("menuAction:"), tag);
            font_menu.addItem_(item);
        }
        font_parent.setSubmenu_(font_menu);
        menu.addItem_(font_parent);

        let syntax_parent = menu_item(
            "Syntax Highlighting",
            "",
            handler,
            selector("menuAction:"),
            -1,
        );
        let syntax_menu = NSMenu::new(nil);
        let syntax_on = menu_item(
            "Enabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SYNTAX_ON,
        );
        let syntax_off = menu_item(
            "Disabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SYNTAX_OFF,
        );
        syntax_menu.addItem_(syntax_on);
        syntax_menu.addItem_(syntax_off);
        syntax_parent.setSubmenu_(syntax_menu);
        menu.addItem_(syntax_parent);

        let secret_parent = menu_item(
            "Secret Copy Auto-Clear",
            "",
            handler,
            selector("menuAction:"),
            -1,
        );
        let secret_menu = NSMenu::new(nil);
        let secret_on = menu_item(
            "Enabled (30s)",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SECRET_CLEAR_ON,
        );
        let secret_off = menu_item(
            "Disabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SECRET_CLEAR_OFF,
        );
        secret_menu.addItem_(secret_on);
        secret_menu.addItem_(secret_off);
        secret_parent.setSubmenu_(secret_menu);
        menu.addItem_(secret_parent);

        let brain_parent = menu_item("Pasta Brain", "", handler, selector("menuAction:"), -1);
        let brain_menu = NSMenu::new(nil);
        let brain_on = menu_item(
            "Enabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_BRAIN_ON,
        );
        let brain_off = menu_item(
            "Disabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_BRAIN_OFF,
        );
        let brain_download = menu_item(
            "Download Model",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_BRAIN_DOWNLOAD,
        );

        // Set initial checkmark state
        let brain_enabled = cx.global::<UiStyleState>().pasta_brain_enabled;
        let _: () = msg_send![brain_on, setState: if brain_enabled { 1_isize } else { 0_isize }];
        let _: () = msg_send![brain_off, setState: if brain_enabled { 0_isize } else { 1_isize }];

        brain_menu.addItem_(brain_on);
        brain_menu.addItem_(brain_off);
        brain_menu.addItem_(NSMenuItem::separatorItem(nil));
        brain_menu.addItem_(brain_download);
        brain_parent.setSubmenu_(brain_menu);
        menu.addItem_(brain_parent);

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let close_item = menu_item(
            "Close Pasta",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_QUIT,
        );

        if button != nil {
            let title = NSString::alloc(nil).init_str("P");
            button.setTitle_(title);
        }

        menu.addItem_(close_item);
        status_item.setMenu_(menu);

        cx.set_global(StatusItemRegistration {
            _status_item: StrongPtr::retain(status_item as id),
            _menu: StrongPtr::retain(menu as id),
            _handler: StrongPtr::retain(handler as id),
            brain_on_item: StrongPtr::retain(brain_on as id),
            brain_off_item: StrongPtr::retain(brain_off as id),
            brain_download_item: StrongPtr::retain(brain_download as id),
        });
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn update_brain_menu_state(cx: &App) {
    let style = cx.global::<UiStyleState>();
    let enabled = style.pasta_brain_enabled;
    let reg = cx.global::<StatusItemRegistration>();
    unsafe {
        let _: () =
            msg_send![*reg.brain_on_item, setState: if enabled { 1_isize } else { 0_isize }];
        let _: () =
            msg_send![*reg.brain_off_item, setState: if enabled { 0_isize } else { 1_isize }];

        // Update download item title based on neural status
        let neural_status = NEURAL_STATUS
            .lock()
            .map(|s| *s)
            .unwrap_or(NeuralStatus::Failed);
        let download_title = match neural_status {
            NeuralStatus::Loading => "Downloading Model...",
            NeuralStatus::Ready => "Model Ready ✓",
            NeuralStatus::Failed => "Download Model (Retry)",
        };
        let title = NSString::alloc(nil).init_str(download_title);
        let _: () = msg_send![*reg.brain_download_item, setTitle: title];
    }
}
