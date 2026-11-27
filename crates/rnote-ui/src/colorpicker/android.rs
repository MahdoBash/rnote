// SPDX-License-Identifier: GPL-3.0-or-later
use crate::RnAppWindow;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

/// Freeze the visible notebook so the tap samples exactly the pixel the user
/// sees. The temporary overlay owns the touch sequence and never draws ink.
#[allow(deprecated)]
pub(super) fn install(
    actions: &gtk4::Box,
    window: &RnAppWindow,
    chooser: &gtk4::ColorChooserWidget,
    popover: &gtk4::Popover,
) {
    super::hide_builtin_eyedropper(chooser.upcast_ref());
    let pick = gtk4::Button::from_icon_name("color-select-symbolic");
    pick.set_tooltip_text(Some(&gettextrs::gettext("Pick a color from the screen")));
    actions.append(&pick);
    pick.connect_clicked(glib::clone!(
        #[weak] window, #[weak] chooser, #[weak] popover,
        move |_| {
            popover.set_visible(false);
            let host = window.overlays().imp().toolbar_overlay.get();
            let snapshot = gtk4::Snapshot::new();
            let (width, height) = (host.width(), host.height());
            gtk4::WidgetPaintable::new(Some(&host)).snapshot(&snapshot, width as f64, height as f64);
            let Some((node, renderer)) = snapshot.to_node().zip(window.renderer()) else {
                popover.popup();
                return;
            };
            let viewport = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let texture = renderer.render_texture(&node, Some(&viewport));
            let picture = gtk4::Picture::for_paintable(&texture);
            picture.set_content_fit(gtk4::ContentFit::Fill);
            picture.set_can_shrink(true);
            let layer = gtk4::Overlay::new();
            layer.set_child(Some(&picture));
            let cancel = gtk4::Button::with_label(&gettextrs::gettext("Cancel"));
            cancel.set_halign(gtk4::Align::Center);
            cancel.set_valign(gtk4::Align::End);
            cancel.set_margin_bottom(12);
            layer.add_overlay(&cancel);
            host.add_overlay(&layer);

            let finish = glib::clone!(#[weak] host, #[weak] layer, #[weak] popover,
                move || {
                    host.remove_overlay(&layer);
                    popover.popup();
                });
            let tap = gtk4::GestureClick::new();
            tap.connect_released(glib::clone!(
                #[weak] picture, #[weak] chooser, #[strong] finish,
                move |_, _, x, y| {
                    if picture.width() > 0 && picture.height() > 0 {
                        let col = (x * texture.width() as f64 / picture.width() as f64).floor() as i32;
                        let row = (y * texture.height() as f64 / picture.height() as f64).floor() as i32;
                        if (0..texture.width()).contains(&col) && (0..texture.height()).contains(&row) {
                            let mut downloader = gdk::TextureDownloader::new(&texture);
                            downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
                            let (rgba, stride) = downloader.download_bytes();
                            let i = row as usize * stride + col as usize * 4;
                            let f = |offset: usize| f32::from(rgba[i + offset]) / 255.0;
                            chooser.set_rgba(&gdk::RGBA::new(f(0), f(1), f(2), f(3)));
                        }
                    }
                    finish();
                }
            ));
            picture.add_controller(tap);
            cancel.connect_clicked(move |_| finish());
        }
    ));
}
