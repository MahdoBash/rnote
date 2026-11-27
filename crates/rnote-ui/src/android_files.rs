// SPDX-License-Identifier: GPL-3.0-or-later
use gtk4::{gio, glib::translate::from_glib_full, prelude::*};
use jni::{JavaVM, objects::JValue};

/// GTK returns a GdkAndroidContentFile when the picker first selects a folder.
/// Restore that same document-provider implementation when reopening its saved URI.
pub(crate) fn register() {
    gio::Vfs::default().register_uri_scheme("content", Some(Box::new(|_, uri| {
        match from_uri(uri) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!("Could not restore the selected Android document: {err}");
                None
            }
        }
    })), None);
}

fn from_uri(uri: &str) -> anyhow::Result<Option<gio::File>> {
    unsafe extern "C" {
        fn gdk_android_content_file_from_uri(uri: jni::sys::jobject) -> *mut gio::ffi::GFile;
    }
    // GTK owns the JVM for the entire process. GFile takes its own global
    // reference to Uri before the temporary local JNI frame is released.
    let vm = unsafe { JavaVM::from_raw(ndk_context::android_context().vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    env.with_local_frame(8, |env| {
        let value = env.new_string(uri)?;
        let parsed = env.call_static_method("android/net/Uri", "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&value)])?.l()?;
        Ok(unsafe { from_glib_full(gdk_android_content_file_from_uri(parsed.as_raw())) })
    })
}


/// Export on Rnote's worker, then open the system print preview on the Android UI thread.
pub(crate) async fn print(window: &crate::RnAppWindow) -> anyhow::Result<()> {
    use anyhow::Context;
    use gtk4::glib::translate::ToGlibPtr;
    use jni::objects::JObject;
    use rnote_engine::engine::export::{DocExportFormat, DocExportPrefs};

    let canvas = window.active_tab_canvas().context("No active notebook")?;
    let name = canvas.doc_title_display();
    let prefs = DocExportPrefs { export_format: DocExportFormat::Pdf, ..Default::default() };
    let source = canvas.engine_ref().export_doc(name.clone(), Some(prefs));
    let path = gtk4::glib::user_cache_dir().join(format!("rnote-print-{}.pdf", uuid::Uuid::new_v4()));
    async_fs::write(&path, source.await??).await?;

    let launch = || -> anyhow::Result<()> {
        unsafe extern "C" {
            fn gdk_android_toplevel_get_activity(surface: *mut gtk4::gdk::ffi::GdkSurface) -> jni::sys::jobject;
        }
        let surface = window.surface().context("Notebook window closed")?;
        let vm = unsafe { JavaVM::from_raw(ndk_context::android_context().vm().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        env.with_local_frame(8, |env| {
            // GDK returns a new local reference, owned by this JNI frame.
            let raw = unsafe { gdk_android_toplevel_get_activity(surface.to_glib_none().0) };
            anyhow::ensure!(!raw.is_null(), "Notebook activity closed");
            let activity = unsafe { JObject::from_raw(raw) };
            let app = env.call_method(&activity, "getApplication", "()Landroid/app/Application;", &[])?.l()?;
            let filename = env.new_string(path.to_string_lossy())?;
            let title = env.new_string(name.as_str())?;
            env.call_method(app, "printNotebook", "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&activity), JValue::Object(&filename), JValue::Object(&title)])?;
            Ok(())
        })
    };
    if let Err(error) = launch() {
        let _ = async_fs::remove_file(path).await;
        return Err(error);
    }
    Ok(())
}
