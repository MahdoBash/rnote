/* SPDX-License-Identifier: GPL-3.0-or-later */
package com.github.flxzt.rnote;

import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract;
import android.provider.DocumentsContract.Document;
import android.provider.DocumentsContract.Root;
import android.provider.DocumentsProvider;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.Locale;

/** Expose Rnote's notebook workspace in the system Files app, including the save picker. */
public final class NotebooksProvider extends DocumentsProvider {
    static final String AUTHORITY = "com.github.flxzt.rnote.notebooks";
    private static final String ROOT = "notebooks";
    private static final String[] ROOT_COLUMNS = {Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID,
        Root.COLUMN_TITLE, Root.COLUMN_FLAGS, Root.COLUMN_ICON, Root.COLUMN_MIME_TYPES};
    private static final String[] DOC_COLUMNS = {Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME,
        Document.COLUMN_MIME_TYPE, Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED, Document.COLUMN_FLAGS};

    @Override public boolean onCreate() { return true; }

    static File root(Context context) throws FileNotFoundException {
        File external = context.getExternalFilesDir(null);
        if (external == null) throw new FileNotFoundException("Notebook storage unavailable");
        File root = new File(external, "share/Notebooks");
        if (!root.mkdirs() && !root.isDirectory()) throw new FileNotFoundException("Notebook storage unavailable");
        return root;
    }

    private static File canonical(File file) throws FileNotFoundException {
        try { return file.getCanonicalFile(); }
        catch (IOException e) { throw new FileNotFoundException(e.getMessage()); }
    }

    static String id(File root, File file) throws FileNotFoundException {
        String base = canonical(root).getPath();
        String path = canonical(file).getPath();
        if (path.equals(base)) return ROOT;
        if (!path.startsWith(base + "/")) throw new FileNotFoundException("Outside notebook workspace");
        return ROOT + path.substring(base.length());
    }

    private File file(String id) throws FileNotFoundException {
        File base = root(getContext());
        if (id.equals(ROOT)) return base;
        if (!id.startsWith(ROOT + "/")) throw new FileNotFoundException("Unknown document");
        File f = canonical(new File(base, id.substring(ROOT.length() + 1)));
        if (!id.equals(id(base, f)) || !f.exists()) throw new FileNotFoundException("Unknown document");
        return f;
    }

    private static File child(File parent, String name) throws FileNotFoundException {
        if (name.isEmpty() || name.contains("/") || name.equals(".") || name.equals(".."))
            throw new FileNotFoundException("Invalid document name");
        File f = canonical(new File(parent, name));
        if (!canonical(parent).equals(f.getParentFile())) throw new FileNotFoundException("Invalid location");
        return f;
    }

    private static String mime(File f) {
        if (f.isDirectory()) return Document.MIME_TYPE_DIR;
        String name = f.getName().toLowerCase(Locale.ROOT);
        if (name.endsWith(".rnote")) return "application/rnote";
        if (name.endsWith(".xopp")) return "application/x-xopp";
        String type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substring(name.lastIndexOf('.') + 1));
        return type == null ? "application/octet-stream" : type;
    }

    private void add(MatrixCursor cursor, File f) throws FileNotFoundException {
        String key = id(root(getContext()), f);
        MatrixCursor.RowBuilder row = cursor.newRow();
        int flags = f.isDirectory() ? Document.FLAG_DIR_SUPPORTS_CREATE : Document.FLAG_SUPPORTS_WRITE;
        if (!ROOT.equals(key)) flags |= Document.FLAG_SUPPORTS_RENAME;
        // Only empty folders can be removed through this provider.
        if (!ROOT.equals(key) && !f.isDirectory()) flags |= Document.FLAG_SUPPORTS_DELETE;
        row.add(Document.COLUMN_DOCUMENT_ID, key).add(Document.COLUMN_DISPLAY_NAME, f.getName())
           .add(Document.COLUMN_MIME_TYPE, mime(f)).add(Document.COLUMN_SIZE, f.isFile() ? f.length() : null)
           .add(Document.COLUMN_LAST_MODIFIED, f.lastModified()).add(Document.COLUMN_FLAGS, flags);
    }

    @Override public Cursor queryRoots(String[] projection) throws FileNotFoundException {
        MatrixCursor result = new MatrixCursor(projection == null ? ROOT_COLUMNS : projection);
        root(getContext());
        result.newRow().add(Root.COLUMN_ROOT_ID, ROOT).add(Root.COLUMN_DOCUMENT_ID, ROOT)
            .add(Root.COLUMN_TITLE, "Rnote").add(Root.COLUMN_ICON, getContext().getApplicationInfo().icon)
            .add(Root.COLUMN_FLAGS, Root.FLAG_LOCAL_ONLY | Root.FLAG_SUPPORTS_CREATE | Root.FLAG_SUPPORTS_IS_CHILD)
            .add(Root.COLUMN_MIME_TYPES, "*/*");
        return result;
    }

    @Override public Cursor queryDocument(String doc, String[] projection) throws FileNotFoundException {
        MatrixCursor result = new MatrixCursor(projection == null ? DOC_COLUMNS : projection);
        add(result, file(doc));
        return result;
    }

    @Override public Cursor queryChildDocuments(String doc, String[] projection, String order) throws FileNotFoundException {
        MatrixCursor result = new MatrixCursor(projection == null ? DOC_COLUMNS : projection);
        File[] children = file(doc).listFiles();
        if (children == null) throw new FileNotFoundException("Cannot read folder");
        for (File f : children) {
            if (!f.isHidden() && canonical(f).equals(f.getAbsoluteFile())) add(result, f);
        }
        result.setNotificationUri(getContext().getContentResolver(),
            DocumentsContract.buildChildDocumentsUri(AUTHORITY, doc));
        return result;
    }

    @Override public boolean isChildDocument(String parent, String document) {
        try { return canonical(file(document)).getPath().startsWith(canonical(file(parent)).getPath() + "/"); }
        catch (FileNotFoundException e) { return false; }
    }

    @Override public ParcelFileDescriptor openDocument(String id, String mode, CancellationSignal signal) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(id), ParcelFileDescriptor.parseMode(mode));
    }

    private void changed(File parent) throws FileNotFoundException {
        Uri children = DocumentsContract.buildChildDocumentsUri(AUTHORITY, id(root(getContext()), parent));
        getContext().getContentResolver().notifyChange(children, null);
    }

    @Override public String createDocument(String parent, String type, String name) throws FileNotFoundException {
        File dir = file(parent);
        File f = child(dir, name);
        try {
            if (!(Document.MIME_TYPE_DIR.equals(type) ? f.mkdir() : f.createNewFile()))
                throw new FileNotFoundException("Name already exists");
        } catch (IOException e) { throw new FileNotFoundException(e.getMessage()); }
        changed(dir);
        return id(root(getContext()), f);
    }

    @Override public String renameDocument(String doc, String name) throws FileNotFoundException {
        if (doc.equals(ROOT)) throw new FileNotFoundException("Cannot rename workspace");
        File old = file(doc);
        File target = child(old.getParentFile(), name);
        if (target.exists() || !old.renameTo(target)) throw new FileNotFoundException("Could not rename document");
        changed(target.getParentFile());
        return id(root(getContext()), target);
    }

    @Override public void deleteDocument(String doc) throws FileNotFoundException {
        File f = file(doc);
        if (doc.equals(ROOT) || !f.delete()) throw new FileNotFoundException("Could not remove document");
        changed(f.getParentFile());
    }
}
