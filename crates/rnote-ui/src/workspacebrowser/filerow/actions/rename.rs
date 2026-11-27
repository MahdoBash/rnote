// Imports
use crate::RnAppWindow;
use crate::workspacebrowser::{RnFileRow, widgethelper};
use gettextrs::gettext;
use gtk4::{Align, Entry, Label, gio, glib, glib::clone, pango, prelude::*};
use std::path::Path;
use tracing::{debug, error};
use unicode_segmentation::UnicodeSegmentation;

/// Create a new `rename` action.
pub(crate) fn rename(filerow: &RnFileRow, appwindow: &RnAppWindow) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("rename-file", None);
    action.connect_activate(clone!(
        #[weak]
        filerow,
        #[weak]
        appwindow,
        move |_, _| {
            let Some(current_file) = filerow.current_file() else {
                return;
            };
            let Some(name) = current_file.basename() else { return; };
            let Some(parent) = current_file.parent().or_else(|| appwindow.sidebar().workspacebrowser().dir_list_file()) else {
                return;
            };
            let entry = create_entry(&name);
            let label = create_label();
            let (apply_button, popover) = widgethelper::create_entry_dialog(&entry, &label);
            filerow.menubutton_box().append(&popover);

            // Initially the file name is set to the same file name, so set the apply button insensitive first.
            apply_button.set_sensitive(false);

            entry.connect_text_notify(clone!(
                #[strong]
                parent,
                #[weak]
                apply_button,
                move |entry2| {
                    let name = entry2.text();
                    apply_button.set_sensitive(!name.is_empty() && !name.contains('/')
                        && parent.child_for_display_name(&name).is_ok_and(|f| !f.query_exists(gio::Cancellable::NONE)));
                }
            ));

            apply_button.connect_clicked(clone!(
                #[weak]
                popover,
                #[weak]
                entry,
                #[weak]
                appwindow,
                move |_| {
                    let name = entry.text();
                    let Ok(target) = parent.child_for_display_name(&name) else { return; };

                    if target.query_exists(gio::Cancellable::NONE) {
                        appwindow.overlays().dispatch_toast_error(&gettext(
                            "Renaming file failed, target file already exists",
                        ));
                        debug!(
                            "Renaming file with path '{}' failed, target file already exists",
                            crate::utils::file_label(&target)
                        );
                    } else {
                        glib::spawn_future_local(clone!(
                            #[strong]
                            current_file,
                            #[weak]
                            appwindow,
                            async move {
                                appwindow.overlays().progressbar_start_pulsing();
                                if let Err(e) =
                                    current_file.set_display_name_future(&name, glib::Priority::DEFAULT).await
                                {
                                    error!(
                                        "Renaming file with path `{}` failed, Err: {e:?}",
                                        crate::utils::file_label(&target)
                                    );
                                    appwindow
                                        .overlays()
                                        .dispatch_toast_error(&gettext("Renaming file failed"));
                                    appwindow.overlays().progressbar_abort();
                                } else {
                                    appwindow.overlays().progressbar_finish();
                                }
                            }
                        ));
                    }
                    popover.popdown();
                }
            ));

            popover.popup();
            entry_text_select_stem(&entry);
        }
    ));
    action
}

fn create_entry(current_path: impl AsRef<Path>) -> Entry {
    let entry_text = current_path
        .as_ref()
        .file_name()
        .map(|current_file_name| current_file_name.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from(""));

    Entry::builder()
        .text(glib::GString::from(entry_text))
        .build()
}

fn entry_text_select_stem(entry: &Entry) {
    let entry_text = entry.text();

    let stem_end = entry_text
        .graphemes(true)
        .enumerate()
        .filter(|(_, g)| *g == ".")
        .last()
        .map(|(i, _)| i);

    // Select entire text first
    entry.grab_focus();
    if let Some(end) = stem_end {
        // Select only the file stem
        debug!("file name select end position: {end}");
        entry.select_region(0, end as i32);
    }
}

fn create_label() -> Label {
    let label = Label::builder()
        .margin_bottom(12)
        .halign(Align::Center)
        .label(gettext("Rename"))
        .width_chars(24)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    label.add_css_class("title-4");
    label
}
