/*
 * gpu-run preloads this beside VirtualGL's faker, so every program of a GPU desktop session
 * loads it. Chromium's GPU process picks its X visual with raw GLX requests that bypass
 * VirtualGL and then looks for a framebuffer configuration attached to that visual, which
 * VirtualGL only emulates with VGL_CHROMEHACK=1. The workaround attaches the browser's 32-bit
 * visual to every configuration with an 8-bit alpha channel, which would change the window
 * visual of a game or Unity player that asks for one, so it is set per process, for the
 * processes that are Chromium.
 *
 * A Chromium build is recognised by its ICU data and resource pack beside the executable:
 * Chrome, Chromium, Edge, Brave, Vivaldi, Opera and Electron or CEF applications all ship
 * them, whatever the executable is called, however it was installed and whatever started it
 * (the panel, the menu, a link another application opens, a terminal).
 *
 * An explicit VGL_CHROMEHACK always wins. A value set here is marked, and a program that is
 * not Chromium drops it again, so a game started from a browser or an Electron application's
 * terminal keeps VirtualGL's default visuals.
 */
#define _GNU_SOURCE
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define MARK "BILSKIRNIR_CHROMEHACK"

static int beside_executable(char *path, size_t directory, const char *name)
{
    size_t length = strlen(name);
    if (directory + length >= PATH_MAX) return 0;
    memcpy(path + directory, name, length + 1);
    return access(path, F_OK) == 0;
}

static int is_chromium(void)
{
    char path[PATH_MAX];
    ssize_t length = readlink("/proc/self/exe", path, sizeof path - 1);
    if (length <= 0) return 0;
    path[length] = '\0';
    char *slash = strrchr(path, '/');
    if (slash == NULL) return 0;
    size_t directory = (size_t)(slash - path) + 1;
    return beside_executable(path, directory, "icudtl.dat")
        && beside_executable(path, directory, "resources.pak");
}

/* Runs before main, so before the first X or GLX call makes VirtualGL read its settings. */
__attribute__((constructor)) static void chromium_gpu(void)
{
    if (is_chromium()) {
        if (getenv("VGL_CHROMEHACK") == NULL) {
            setenv("VGL_CHROMEHACK", "1", 1);
            setenv(MARK, "auto", 1);
        }
    } else if (getenv(MARK) != NULL) {
        unsetenv("VGL_CHROMEHACK");
        unsetenv(MARK);
    }
}
