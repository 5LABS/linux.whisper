/* hold_daemon.c – Super+Space hold-to-dictate daemon
 * Reads raw evdev events from all keyboards. While Super is held
 * and Space is pressed, grabs all devices (so GNOME doesn't see the
 * Super release) and forks dictate.sh start. On Space or Super release
 * while recording, ungrabs and forks dictate.sh stop. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <fcntl.h>
#include <unistd.h>
#include <poll.h>
#include <signal.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <stdint.h>
#include <linux/input.h>

#define MAX_DEVS 16

static int fds[MAX_DEVS];
static int nfds = 0;
static char script[512];

static void grab_all(int on) {
    for (int i = 0; i < nfds; i++)
        ioctl(fds[i], EVIOCGRAB, (void *)(intptr_t)on);
}

static void run(const char *cmd) {
    pid_t pid = fork();
    if (pid == 0) {
        /* detach from our process group so signals don't hit the child */
        setsid();
        execl("/bin/bash", "bash", script, cmd, NULL);
        _exit(1);
    }
    /* don't wait – recording/transcription runs in background */
}

static void cleanup(int sig) {
    grab_all(0);
    for (int i = 0; i < nfds; i++) close(fds[i]);
    if (sig) _exit(0);
}

/* Check if device has both KEY_LEFTMETA and KEY_SPACE */
static int is_keyboard(int fd) {
    uint8_t evbits[EV_MAX / 8 + 1] = {0};
    if (ioctl(fd, EVIOCGBIT(0, sizeof(evbits)), evbits) < 0) return 0;
    if (!(evbits[EV_KEY / 8] & (1 << (EV_KEY % 8)))) return 0;

    uint8_t keybits[KEY_MAX / 8 + 1] = {0};
    if (ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(keybits)), keybits) < 0) return 0;

    int has_meta  = keybits[KEY_LEFTMETA  / 8] & (1 << (KEY_LEFTMETA  % 8));
    int has_space = keybits[KEY_SPACE      / 8] & (1 << (KEY_SPACE     % 8));
    return has_meta && has_space;
}

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;

    /* Build path to dictate.sh relative to this binary */
    char self[512];
    ssize_t len = readlink("/proc/self/exe", self, sizeof(self) - 1);
    if (len < 0) { perror("readlink"); return 1; }
    self[len] = '\0';
    char *slash = strrchr(self, '/');
    if (slash) *slash = '\0';
    snprintf(script, sizeof(script), "%s/dictate.sh", self);

    /* Find all keyboard input devices */
    DIR *d = opendir("/dev/input");
    if (!d) { perror("/dev/input"); return 1; }
    struct dirent *de;
    while ((de = readdir(d)) && nfds < MAX_DEVS) {
        if (strncmp(de->d_name, "event", 5) != 0) continue;
        char path[64];
        snprintf(path, sizeof(path), "/dev/input/%s", de->d_name);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd < 0) continue;
        if (is_keyboard(fd)) fds[nfds++] = fd;
        else close(fd);
    }
    closedir(d);

    if (nfds == 0) { fprintf(stderr, "No keyboard found\n"); return 1; }

    signal(SIGINT,  cleanup);
    signal(SIGTERM, cleanup);

    struct pollfd pfds[MAX_DEVS];
    for (int i = 0; i < nfds; i++) {
        pfds[i].fd     = fds[i];
        pfds[i].events = POLLIN;
    }

    int super_held = 0;
    int recording  = 0;

    for (;;) {
        /* Reap any finished children to avoid zombies */
        while (waitpid(-1, NULL, WNOHANG) > 0) {}

        int ret = poll(pfds, nfds, -1);
        if (ret <= 0) continue;

        for (int i = 0; i < nfds; i++) {
            if (!(pfds[i].revents & POLLIN)) continue;

            struct input_event ev;
            while (read(fds[i], &ev, sizeof(ev)) == sizeof(ev)) {
                if (ev.type != EV_KEY) continue;
                int code = ev.code;
                int val  = ev.value; /* 0=up 1=down 2=repeat */

                if (code == KEY_LEFTMETA || code == KEY_RIGHTMETA) {
                    if (val == 1) {
                        super_held = 1;
                    } else if (val == 0) {
                        super_held = 0;
                        if (recording) {
                            recording = 0;
                            grab_all(0);
                            run("stop");
                        }
                    }
                } else if (code == KEY_SPACE) {
                    if (val == 1 && super_held && !recording) {
                        recording = 1;
                        grab_all(1);
                        run("start");
                    } else if (val == 0 && recording) {
                        recording = 0;
                        grab_all(0);
                        run("stop");
                    }
                }
            }
        }
    }
}
