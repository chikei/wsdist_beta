#define _POSIX_C_SOURCE 200809L
/* Development-only archive proof: observe a real X11 window, capture, WM_CLOSE.
 * This helper is never included in the application archive. */
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <png.h>
#include <errno.h>
#include <limits.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/signalfd.h>
#include <sys/wait.h>
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

enum { FOCUS_MAIN_CAPACITY = 32, FOCUS_MAIN_EVENT_BATCH = 32 };

struct focus_main_state {
    Display *display;
    Window root, windows[FOCUS_MAIN_CAPACITY], focused;
    unsigned int count;
    pid_t child;
    int child_reaped, signal_fd, display_fd;
    sigset_t previous_mask;
    int x_failed, window_missing, setting_focus, focus_bad_match;
    XErrorEvent focus_error;
};

static struct focus_main_state *active_focus_main;

static int focus_main_x_error(Display *display, XErrorEvent *event) {
    struct focus_main_state *state = active_focus_main;
    if (event->error_code == BadWindow) {
        state->window_missing = 1;
    } else if (state->setting_focus && event->error_code == BadMatch) {
        /* An unmap between the visibility check and focus request is a race,
         * not a reason to focus a stale window or abandon its previous Main. */
        state->focus_bad_match = 1;
        state->focus_error = *event;
        return 0;
    } else {
        state->x_failed = 1;
    }
    return x_error(display, event);
}

static void focus_main_kill_child(struct focus_main_state *state) {
    if (state->child <= 0 || state->child_reaped) return;
    if (kill(state->child, SIGKILL) < 0 && errno != ESRCH) perror("kill child");
    if (kill(-state->child, SIGKILL) < 0 && errno != ESRCH) perror("kill child process group");
    pid_t waited;
    do { waited = waitpid(state->child, NULL, 0); } while (waited < 0 && errno == EINTR);
    if (waited == state->child || (waited < 0 && errno == ECHILD)) state->child_reaped = 1;
    else if (waited < 0) perror("waitpid during cleanup");
}

static int focus_main_io_error(Display *display) {
    (void)display;
    /* Xlib cannot recover from a lost connection. Do not call back into it. */
    fprintf(stderr, "X11 connection lost while focusing Main windows\n");
    struct focus_main_state *state = active_focus_main;
    focus_main_kill_child(state);
    if (state->signal_fd >= 0) close(state->signal_fd);
    if (state->display_fd >= 0) close(state->display_fd);
    sigprocmask(SIG_SETMASK, &state->previous_mask, NULL);
    _exit(1);
}

/* Return 1 for a qualifying Main, 0 for a stale/non-Main window, -1 on error. */
static int focus_main_title(struct focus_main_state *state, Window window, char **title) {
    static const char prefix[] = "Kastra FFXI Damage Simulator";
    XWindowAttributes attributes;
    Window tree_root, parent, *children = NULL;
    unsigned int count = 0;
    int qualifies = 0;
    *title = NULL;
    state->window_missing = 0;
    if (!XGetWindowAttributes(state->display, window, &attributes) ||
            attributes.map_state != IsViewable || attributes.override_redirect) goto done;
    if (!XQueryTree(state->display, window, &tree_root, &parent, &children, &count) ||
            parent != state->root) goto done;
    *title = window_title(state->display, window);
    qualifies = *title != NULL && strncmp(*title, prefix, sizeof(prefix) - 1) == 0;
done:
    if (children != NULL) XFree(children);
    XSync(state->display, False);
    if (state->x_failed) qualifies = -1;
    else if (state->window_missing) qualifies = 0;
    if (qualifies != 1 && *title != NULL) { XFree(*title); *title = NULL; }
    return qualifies;
}

static int focus_main_window(struct focus_main_state *state, Window window, const char *title) {
    state->window_missing = 0;
    state->focus_bad_match = 0;
    state->setting_focus = 1;
    XSetInputFocus(state->display, window, RevertToParent, CurrentTime);
    XSync(state->display, False);
    state->setting_focus = 0;
    if (state->x_failed) return -1;
    if (state->window_missing) return 0;
    Window actual;
    int revert;
    if (!XGetInputFocus(state->display, &actual, &revert)) {
        fprintf(stderr, "Could not read actual X11 input focus\n");
        return -1;
    }
    XSync(state->display, False);
    if (state->x_failed) return -1;
    if (state->focus_bad_match || actual != window) {
        char *current_title = NULL;
        int qualifies = focus_main_title(state, window, &current_title);
        if (current_title != NULL) XFree(current_title);
        if (qualifies != 1) return qualifies;
        if (state->focus_bad_match) x_error(state->display, &state->focus_error);
        fprintf(stderr, "Main X11 focus verification failed for 0x%lx (actual 0x%lx)\n",
            (unsigned long)window, (unsigned long)actual);
        return -1;
    }
    fprintf(stderr, "Focused Main X11 window 0x%lx: %s\n", (unsigned long)actual, title);
    return 1;
}

static int focus_main_remove(struct focus_main_state *state, Window window) {
    int was_focused = state->focused == window;
    for (unsigned int index = 0; index < state->count; ++index) {
        if (state->windows[index] != window) continue;
        --state->count;
        if (index < state->count) memmove(&state->windows[index], &state->windows[index + 1],
            (state->count - index) * sizeof(state->windows[0]));
        break;
    }
    if (was_focused) state->focused = None;
    return was_focused;
}

static int focus_main_restore(struct focus_main_state *state) {
    while (state->count != 0) {
        Window window = state->windows[state->count - 1];
        char *title = NULL;
        int result = focus_main_title(state, window, &title);
        if (result == 1) {
            result = focus_main_window(state, window, title);
            XFree(title);
        }
        if (result < 0) return -1;
        if (result == 1) { state->focused = window; return 0; }
        focus_main_remove(state, window);
    }
    return 0;
}

static int focus_main_event(struct focus_main_state *state, XEvent *event) {
    Window window;
    if (event->type == MapNotify && event->xmap.event == state->root) {
        window = event->xmap.window;
        char *title = NULL;
        int result = event->xmap.override_redirect ? 0 : focus_main_title(state, window, &title);
        if (result < 0) return -1;
        if (result == 1) {
            unsigned int index = 0;
            while (index < state->count && state->windows[index] != window) ++index;
            if (state->count == FOCUS_MAIN_CAPACITY && index == state->count) {
                fprintf(stderr, "Main X11 window stack capacity exceeded (%d)\n", FOCUS_MAIN_CAPACITY);
                XFree(title);
                return -1;
            }
            result = focus_main_window(state, window, title);
            XFree(title);
            if (result < 0) return -1;
            if (result == 1) {
                focus_main_remove(state, window);
                state->windows[state->count++] = window;
                state->focused = window;
                return 0;
            }
            /* It may have taken focus and vanished before verification. */
            focus_main_remove(state, window);
            return focus_main_restore(state);
        }
    } else if (event->type == UnmapNotify && event->xunmap.event == state->root) {
        window = event->xunmap.window;
    } else if (event->type == DestroyNotify && event->xdestroywindow.event == state->root) {
        window = event->xdestroywindow.window;
    } else {
        return 0;
    }
    /* Remember the last verified Main: X11 has already reverted its input
     * focus to the parent by the time an unmap/destroy notification arrives. */
    return focus_main_remove(state, window) ? focus_main_restore(state) : 0;
}

static int focus_main_command(char **command) {
    struct focus_main_state state = { .child = -1, .signal_fd = -1, .display_fd = -1 };
    sigset_t signals;
    sigemptyset(&signals);
    sigaddset(&signals, SIGCHLD);
    sigaddset(&signals, SIGTERM);
    sigaddset(&signals, SIGINT);
    if (sigprocmask(SIG_BLOCK, &signals, &state.previous_mask) < 0) { perror("sigprocmask"); return 1; }
    active_focus_main = &state;
    XErrorHandler previous_error = XSetErrorHandler(focus_main_x_error);
    XIOErrorHandler previous_io_error = XSetIOErrorHandler(focus_main_io_error);
    int result = 1;
    state.signal_fd = signalfd(-1, &signals, SFD_CLOEXEC | SFD_NONBLOCK);
    if (state.signal_fd < 0) { perror("signalfd"); goto done; }
    state.display = XOpenDisplay(NULL);
    if (state.display == NULL) { fprintf(stderr, "A running X11 display is required\n"); goto done; }
    state.display_fd = ConnectionNumber(state.display);
    state.root = DefaultRootWindow(state.display);
    /* Listening is established before the command can create its first window. */
    XSelectInput(state.display, state.root, SubstructureNotifyMask);
    XSync(state.display, False);
    if (state.x_failed || state.window_missing) goto done;
    state.child = fork();
    if (state.child < 0) { perror("fork"); goto done; }
    if (state.child == 0) {
        if (setpgid(0, 0) < 0) { perror("setpgid"); _exit(126); }
        close(state.display_fd);
        close(state.signal_fd);
        if (sigprocmask(SIG_SETMASK, &state.previous_mask, NULL) < 0) { perror("sigprocmask"); _exit(126); }
        execvp(command[0], command);
        int error = errno;
        perror(command[0]);
        _exit(error == ENOENT ? 127 : 126);
    }
    /* Both sides set the private group, avoiding a race with exec or cleanup. */
    if (setpgid(state.child, state.child) < 0 && errno != EACCES && errno != ESRCH) {
        perror("setpgid");
        goto done;
    }
    for (;;) {
        /* Bound buffered X work so exit and termination signals cannot starve. */
        for (unsigned int count = 0; count < FOCUS_MAIN_EVENT_BATCH && XPending(state.display) != 0; ++count) {
            XEvent event;
            XNextEvent(state.display, &event);
            if (focus_main_event(&state, &event) < 0 || state.x_failed) goto done;
        }
        if (state.x_failed) goto done;
        struct pollfd descriptors[2] = {
            { .fd = state.display_fd, .events = POLLIN },
            { .fd = state.signal_fd, .events = POLLIN }
        };
        int ready, timeout = XEventsQueued(state.display, QueuedAlready) != 0 ? 0 : -1;
        do { ready = poll(descriptors, 2, timeout); } while (ready < 0 && errno == EINTR);
        if (ready < 0) { perror("poll"); goto done; }
        if (descriptors[0].revents & (POLLERR | POLLHUP | POLLNVAL)) {
            fprintf(stderr, "X11 connection failed while waiting for Main windows\n");
            goto done;
        }
        if (descriptors[1].revents & (POLLERR | POLLHUP | POLLNVAL)) {
            fprintf(stderr, "Signal descriptor failed while waiting for the command\n");
            goto done;
        }
        if (!(descriptors[1].revents & POLLIN)) continue;
        for (;;) {
            struct signalfd_siginfo signal;
            ssize_t bytes = read(state.signal_fd, &signal, sizeof(signal));
            if (bytes < 0 && errno == EINTR) continue;
            if (bytes < 0 && errno == EAGAIN) break;
            if (bytes != (ssize_t)sizeof(signal)) {
                if (bytes < 0) perror("read signalfd");
                else fprintf(stderr, "Incomplete signal notification\n");
                goto done;
            }
            if (signal.ssi_signo == SIGTERM || signal.ssi_signo == SIGINT) {
                result = 128 + (int)signal.ssi_signo;
                if (kill(-state.child, (int)signal.ssi_signo) < 0 && errno != ESRCH) perror("forward signal");
                goto done;
            }
            if (signal.ssi_signo == SIGCHLD) {
                siginfo_t status = { 0 };
                int waited;
                do { waited = waitid(P_PID, (id_t)state.child, &status, WEXITED | WNOHANG | WNOWAIT); }
                while (waited < 0 && errno == EINTR);
                if (waited < 0) {
                    if (errno == ECHILD) state.child_reaped = 1;
                    perror("waitid");
                    goto done;
                }
                /* Stopped/continued or coalesced SIGCHLD may precede exit. */
                if (status.si_pid == 0) continue;
                /* Keep the leader PID reserved through private-group cleanup. */
                result = status.si_code == CLD_EXITED ? status.si_status : 128 + status.si_status;
                goto done;
            }
        }
    }
done:
    /* Only our private group is cleaned, including descendants surviving a
     * normal direct-child exit. Abort stays bounded if forwarded TERM is ignored. */
    focus_main_kill_child(&state);
    if (state.display != NULL) XCloseDisplay(state.display);
    if (state.signal_fd >= 0 && close(state.signal_fd) < 0) { perror("close signalfd"); result = 1; }
    XSetErrorHandler(previous_error);
    XSetIOErrorHandler(previous_io_error);
    active_focus_main = NULL;
    if (sigprocmask(SIG_SETMASK, &state.previous_mask, NULL) < 0) { perror("sigprocmask"); result = 1; }
    return state.x_failed && result == 0 ? 1 : result;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--focus-main") == 0) {
        if (argc < 3) { fprintf(stderr, "Usage: native-window-x11 --focus-main COMMAND [ARGS...]\n"); return 2; }
        return focus_main_command(&argv[2]);
    }
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
