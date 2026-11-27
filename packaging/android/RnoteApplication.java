/* SPDX-License-Identifier: GPL-3.0-or-later */
package com.github.flxzt.rnote;

import android.system.ErrnoException;
import android.system.Os;
import java.io.File;
import java.util.Locale;
import org.gtk.android.RuntimeApplication;

public final class RnoteApplication extends RuntimeApplication {
    private static native void initializeContext(RnoteApplication context);

    @Override
    public void onCreate() {
        // Set up native libraries before GTK starts the Rust application thread.
        try {
            Os.setenv("FONTCONFIG_FILE", new File(getFilesDir(), "share/rnote/fonts.conf").getPath(), true);
            Os.setenv("XDG_CACHE_HOME", getCacheDir().getPath(), true);
            Os.setenv("GSETTINGS_BACKEND", "keyfile", true);
            Os.setenv("LANGUAGE", Locale.getDefault().toLanguageTag().replace('-', '_'), true);
        } catch (ErrnoException e) {
            throw new IllegalStateException("Could not initialize Rnote's application directories", e);
        }
        System.loadLibrary("rnote");
        initializeContext(this);
        super.onCreate();
    }
}
