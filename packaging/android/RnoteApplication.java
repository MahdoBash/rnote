/* SPDX-License-Identifier: GPL-3.0-or-later */
package com.github.flxzt.rnote;

import android.system.ErrnoException;
import android.system.Os;
import java.io.File;
import java.util.Locale;
import org.gtk.android.RuntimeApplication;

public final class RnoteApplication extends RuntimeApplication {
    private static native void initializeContext(RnoteApplication context);

    public static void persistWorkspaceGrant(android.app.Activity activity, android.content.Intent result) {
        if (result == null || result.getData() == null ||
                !android.provider.DocumentsContract.isTreeUri(result.getData())) return;
        int flags = result.getFlags();
        if ((flags & android.content.Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION) == 0) return;
        try {
            activity.getContentResolver().takePersistableUriPermission(result.getData(), flags &
                (android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION | android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION));
        } catch (SecurityException e) {
            android.util.Log.w("Rnote", "Folder access was granted for this session only", e);
        }
    }

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
