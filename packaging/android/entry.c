/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <android/log.h>
#include <dlfcn.h>
#include <stdlib.h>

/* GTK's RuntimeApplication has already initialized its GDK backend. Enter the
 * same application as the desktop executable on the thread GTK provides. */
__attribute__((visibility("default")))
int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    void *library = dlopen("librnote_mobile.so", RTLD_NOW | RTLD_LOCAL);
    if (library == NULL) {
        __android_log_print(ANDROID_LOG_ERROR, "Rnote", "Loading Rnote: %s", dlerror());
        return EXIT_FAILURE;
    }
    int (*run)(void) = (int (*)(void))dlsym(library, "rnote_android_run");
    if (run == NULL) {
        __android_log_print(ANDROID_LOG_ERROR, "Rnote", "Finding Rnote entry point: %s", dlerror());
        return EXIT_FAILURE;
    }
    return run();
}
