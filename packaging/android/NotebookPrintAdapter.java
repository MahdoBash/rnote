/* SPDX-License-Identifier: GPL-3.0-or-later */
package com.github.flxzt.rnote;

import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.print.PageRange;
import android.print.PrintAttributes;
import android.print.PrintDocumentAdapter;
import android.print.PrintDocumentInfo;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Send Rnote's vector PDF to Android's print spooler without rasterizing ink or text. */
final class NotebookPrintAdapter extends PrintDocumentAdapter {
    private final File pdf;
    private final PrintDocumentInfo info;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();

    NotebookPrintAdapter(File pdf, String name) {
        this.pdf = pdf;
        info = new PrintDocumentInfo.Builder(name)
            .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
            .setPageCount(PrintDocumentInfo.PAGE_COUNT_UNKNOWN)
            .build();
    }

    @Override
    public void onLayout(PrintAttributes oldAttributes, PrintAttributes newAttributes,
                         CancellationSignal cancellation, LayoutResultCallback callback,
                         android.os.Bundle extras) {
        if (cancellation.isCanceled()) callback.onLayoutCancelled();
        else callback.onLayoutFinished(info, false);
    }

    @Override
    public void onWrite(PageRange[] requested, ParcelFileDescriptor destination,
                        CancellationSignal cancellation, WriteResultCallback callback) {
        worker.execute(() -> {
            try (FileInputStream input = new FileInputStream(pdf);
                 OutputStream output = new ParcelFileDescriptor.AutoCloseOutputStream(destination)) {
                byte[] buffer = new byte[65536];
                int length;
                while (!cancellation.isCanceled() && (length = input.read(buffer)) != -1) {
                    output.write(buffer, 0, length);
                }
            } catch (IOException error) {
                if (cancellation.isCanceled()) callback.onWriteCancelled();
                else callback.onWriteFailed(error.getLocalizedMessage());
                return;
            }
            if (cancellation.isCanceled()) callback.onWriteCancelled();
            else callback.onWriteFinished(new PageRange[] {PageRange.ALL_PAGES});
        });
    }

    @Override
    public void onFinish() {
        worker.shutdown();
        pdf.delete();
    }
}
