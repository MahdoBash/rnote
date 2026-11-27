// Imports
use crate::RnAppWindow;
use crate::workspacebrowser::RnFileRow;
use gettextrs::gettext;
use gtk4::{gio, gio::prelude::FileExt, glib, glib::clone};
use tracing::debug;

/// Create a new `open-in-default-app` action.
pub(crate) fn open_in_default_app(
    filerow: &RnFileRow,
    appwindow: &RnAppWindow,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("open-in-default-app", None);
    action.connect_activate(clone!(
        #[weak]
        filerow,
        #[weak]
        appwindow,
        move |_, _| {
            let Some(current_file) = filerow.current_file() else {
                return;
            };
            #[cfg(target_os = "android")]
            crate::android_files::view(&current_file, &appwindow);
            #[cfg(not(target_os = "android"))]
            if let Err(e) = open::that_detached(current_file.uri()) {
                appwindow
                    .overlays()
                    .dispatch_toast_error(&gettext("Open the file in the default app failed"));
                debug!(
                    "Opening file {} with default app failed, Err: {e:?}",
                    current_file.uri()
                );
            }
        }
    ));
    action
}
