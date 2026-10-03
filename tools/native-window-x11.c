#define _POSIX_C_SOURCE 200809L
/* Development-only archive proof: observe a real X11 window, capture, WM_CLOSE.
 * This helper is never included in the application archive. */
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <png.h>
#include <errno.h>
#include <limits.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static void pause_ms(long milliseconds) {
    struct timespec delay = { milliseconds / 1000, (milliseconds % 1000) * 1000000 };
    while (nanosleep(&delay, &delay) < 0 && errno == EINTR) {}
}

static int belongs_to(Display *display, Window window, unsigned long pid) {
    Atom actual_type;
    int format;
    unsigned long count, remaining;
    unsigned char *data = NULL;
    int matches = 0;
    if (XGetWindowProperty(display, window, XInternAtom(display, "_NET_WM_PID", False),
            0, 1, False, AnyPropertyType, &actual_type, &format, &count, &remaining, &data) == Success &&
            format == 32 && count == 1 && data != NULL) {
        matches = *(unsigned long *)data == pid;
    }
    if (data != NULL) XFree(data);
    return matches;
}

static Window find_visible(Display *display, Window root, unsigned long pid, int depth) {
    Window parent, tree_root, *children = NULL;
    unsigned int count = 0;
    if (depth > 8 || !XQueryTree(display, root, &tree_root, &parent, &children, &count)) return None;
    Window result = None;
    for (unsigned int index = 0; index < count; ++index) {
        XWindowAttributes attributes;
        if (belongs_to(display, children[index], pid) &&
                XGetWindowAttributes(display, children[index], &attributes) &&
                attributes.map_state == IsViewable) {
            result = children[index];
            break;
        }
        result = find_visible(display, children[index], pid, depth + 1);
        if (result != None) break;
    }
    if (children != NULL) XFree(children);
    return result;
}

static char *window_title(Display *display, Window window) {
    Atom actual_type;
    int format;
    unsigned long count, remaining;
    unsigned char *data = NULL;
    if (XGetWindowProperty(display, window, XInternAtom(display, "_NET_WM_NAME", False),
            0, 4096, False, AnyPropertyType, &actual_type, &format, &count, &remaining, &data) == Success &&
            format == 8 && count != 0 && data != NULL) return (char *)data;
    if (data != NULL) XFree(data);
    char *legacy = NULL;
    if (XFetchName(display, window, &legacy)) return legacy;
    return NULL;
}

static unsigned char channel(unsigned long pixel, unsigned long mask) {
    if (mask == 0) return 0;
    unsigned int shift = 0;
    while ((mask & 1) == 0) { mask >>= 1; ++shift; }
    return (unsigned char)((((pixel >> shift) & mask) * 255) / mask);
}

static int capture(Display *display, Window window, const char *path, int width, int height, int *rendered) {
    if (width <= 0 || height <= 0 || (size_t)width > SIZE_MAX / (size_t)height / 3) return 0;
    XImage *surface = XGetImage(display, window, 0, 0, (unsigned int)width, (unsigned int)height, AllPlanes, ZPixmap);
    if (surface == NULL) { fprintf(stderr, "XGetImage did not capture the visible window\n"); return 0; }
    unsigned char *rgb = malloc((size_t)width * (size_t)height * 3);
    if (rgb == NULL) { XDestroyImage(surface); return 0; }
    /* A mapped title over an otherwise blank client frame is not GUI proof.
     * Require foreground/background contrast in each body quadrant, excluding
     * the title/menu band; this is not a golden pixel/style comparison. */
    int minimum[4] = {765, 765, 765, 765}, maximum[4] = {0, 0, 0, 0};
    int body_top = height > 120 ? 60 : 0;
    for (int y = 0; y < height; ++y) {
        for (int x = 0; x < width; ++x) {
            unsigned long pixel = XGetPixel(surface, x, y);
            size_t offset = ((size_t)y * (size_t)width + (size_t)x) * 3;
            rgb[offset] = channel(pixel, surface->red_mask);
            rgb[offset + 1] = channel(pixel, surface->green_mask);
            rgb[offset + 2] = channel(pixel, surface->blue_mask);
            if (y >= body_top) {
                int quadrant = (x >= width / 2) + 2 * (y >= body_top + (height - body_top) / 2);
                int brightness = rgb[offset] + rgb[offset + 1] + rgb[offset + 2];
                if (brightness < minimum[quadrant]) minimum[quadrant] = brightness;
                if (brightness > maximum[quadrant]) maximum[quadrant] = brightness;
            }
        }
    }
    *rendered = 1;
    for (int quadrant = 0; quadrant < 4; ++quadrant) {
        if (maximum[quadrant] - minimum[quadrant] < 192) *rendered = 0;
    }
    if (!*rendered) fprintf(stderr, "Native client frame is blank or not painted across its body\n");
    png_image image;
    memset(&image, 0, sizeof(image));
    image.version = PNG_IMAGE_VERSION;
    image.width = (png_uint_32)width;
    image.height = (png_uint_32)height;
    image.format = PNG_FORMAT_RGB;
    int success = png_image_write_to_file(&image, path, 0, rgb, 0, NULL);
    if (!success) fprintf(stderr, "PNG capture failed: %s\n", image.message);
    png_image_free(&image);
    free(rgb);
    XDestroyImage(surface);
    return success && *rendered;
}

static void json_string(const char *text) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)text; *p; ++p) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 32) printf("\\u%04x", (unsigned int)*p);
        else putchar(*p);
    }
    putchar('"');
}

/* A window can disappear while enumerating it. Ignore only BadWindow races. */
static int x_error(Display *display, XErrorEvent *event) {
    if (event->error_code != BadWindow) {
        char detail[256];
        XGetErrorText(display, event->error_code, detail, (int)sizeof(detail));
        fprintf(stderr, "X11 error: %s\n", detail);
    }
    return 0;
}

static int record_native_modules(unsigned long pid, const char *screenshot) {
    char source[64], destination[PATH_MAX], line[8192];
    int length = snprintf(destination, sizeof(destination), "%s.modules.txt", screenshot);
    if (length < 0 || (size_t)length >= sizeof(destination)) return 0;
    snprintf(source, sizeof(source), "/proc/%lu/maps", pid);
    FILE *maps = fopen(source, "r");
    if (maps == NULL) { perror(source); return 0; }
    FILE *report = fopen(destination, "w");
    if (report == NULL) { perror(destination); fclose(maps); return 0; }
    int success = 1;
    while (fgets(line, sizeof(line), maps) != NULL) {
        if (fputs(line, report) == EOF) success = 0;
        if (strstr(line, "/libpython") != NULL || strstr(line, "/libQt") != NULL || strstr(line, "PySide") != NULL) {
            fprintf(stderr, "Unexpected Python/Qt library loaded: %s", line);
            success = 0;
        }
    }
    if (ferror(maps)) success = 0;
    if (fclose(report) != 0) success = 0;
    fclose(maps);
    return success;
}

int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "Usage: native-window-x11 PID screenshot.png\n"); return 2; }
    char *end = NULL;
    errno = 0;
    unsigned long pid = strtoul(argv[1], &end, 10);
    if (errno || end == argv[1] || *end || pid == 0 || pid > INT_MAX) return 2;
    Display *display = XOpenDisplay(NULL);
    if (display == NULL) { fprintf(stderr, "A running X11 display is required\n"); return 1; }
    XSetErrorHandler(x_error);
    Window window = None;
    char *title = NULL;
    const char *prefix = "Kastra FFXI Damage Simulator";
    for (int attempt = 0; attempt < 300; ++attempt) {
        if (kill((pid_t)pid, 0) < 0 && errno == ESRCH) break;
        window = find_visible(display, DefaultRootWindow(display), pid, 0);
        if (window != None) {
            title = window_title(display, window);
            if (title != NULL && (strncmp(title, prefix, strlen(prefix)) == 0 || strstr(title, "Startup Error") != NULL)) break;
            if (title != NULL) XFree(title);
            title = NULL;
        }
        pause_ms(100);
    }
    if (window == None || title == NULL) {
        fprintf(stderr, "No expected visible application window appeared for PID %lu\n", pid);
        XCloseDisplay(display);
        return 1;
    }
    pause_ms(1200); /* Allow native repaint after mapping; never use the testing backend. */
    XWindowAttributes attributes;
    memset(&attributes, 0, sizeof(attributes));
    int rendered = 0;
    int success = XGetWindowAttributes(display, window, &attributes) && attributes.map_state == IsViewable &&
        attributes.width >= 320 && attributes.height >= 240;
    if (success) success = capture(display, window, argv[2], attributes.width, attributes.height, &rendered);
    if (!record_native_modules(pid, argv[2])) success = 0;
    int expected_title = strncmp(title, prefix, strlen(prefix)) == 0;
    if (!expected_title) { fprintf(stderr, "Application opened a startup-error surface: %s\n", title); success = 0; }
    printf("{\"status\":\"%s\",\"pid\":%lu,\"title\":", success ? "passed" : "failed", pid);
    json_string(title);
    printf(",\"visible\":%s,\"rendered\":%s,\"width\":%d,\"height\":%d}\n", attributes.map_state == IsViewable ? "true" : "false", rendered ? "true" : "false", attributes.width, attributes.height);
    XEvent close;
    memset(&close, 0, sizeof(close));
    close.xclient.type = ClientMessage;
    close.xclient.window = window;
    close.xclient.message_type = XInternAtom(display, "WM_PROTOCOLS", False);
    close.xclient.format = 32;
    close.xclient.data.l[0] = (long)XInternAtom(display, "WM_DELETE_WINDOW", False);
    close.xclient.data.l[1] = CurrentTime;
    if (!XSendEvent(display, window, False, NoEventMask, &close)) success = 0;
    XSync(display, False);
    XFree(title);
    XCloseDisplay(display);
    return success ? 0 : 1;
}
