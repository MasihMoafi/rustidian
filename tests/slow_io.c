/* Linux GUI fixture: hold real filesystem calls until the harness removes a gate. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/sendfile.h>
#include <time.h>
#include <unistd.h>

static void gate(const char *operation, int fd) {
    const char *prefix = getenv("RUSTIDIAN_TEST_IO_GATE");
    if (!prefix) return;
    char blocked[PATH_MAX], entered[PATH_MAX], link[64], path[PATH_MAX];
    snprintf(blocked, sizeof blocked, "%s.%s", prefix, operation);
    if (access(blocked, F_OK)) return;
    snprintf(link, sizeof link, "/proc/self/fd/%d", fd);
    ssize_t count = readlink(link, path, sizeof path - 1);
    if (count < 0) return;
    path[count] = 0;
    if (!strstr(path, "/Test Vault/")) return;
    if (!strcmp(operation, "copy") && !strstr(path, "/.rustidian-copy-")) return;
    snprintf(entered, sizeof entered, "%s.entered-%s", prefix, operation);
    int marker = open(entered, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
    if (marker >= 0) {
        ssize_t written = write(marker, path, (size_t)count);
        close(marker);
        if (written != count) return;
    }
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
    while (!access(blocked, F_OK)) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (now.tv_sec - start.tv_sec >= 15) break;
        usleep(10000);
    }
}

int fsync(int fd) {
    int (*real)(int) = dlsym(RTLD_NEXT, "fsync");
    gate("save", fd);
    return real(fd);
}
ssize_t read(int fd, void *buffer, size_t count) {
    ssize_t (*real)(int, void *, size_t) = dlsym(RTLD_NEXT, "read");
    gate("copy", fd);
    return real(fd, buffer, count);
}
ssize_t copy_file_range(int in, off64_t *in_offset, int out, off64_t *out_offset, size_t count, unsigned int flags) {
    ssize_t (*real)(int, off64_t *, int, off64_t *, size_t, unsigned int) = dlsym(RTLD_NEXT, "copy_file_range");
    gate("copy", out);
    return real(in, in_offset, out, out_offset, count, flags);
}
ssize_t sendfile(int out, int in, off_t *offset, size_t count) {
    ssize_t (*real)(int, int, off_t *, size_t) = dlsym(RTLD_NEXT, "sendfile");
    gate("copy", out);
    return real(out, in, offset, count);
}
