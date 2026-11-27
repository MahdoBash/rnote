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
