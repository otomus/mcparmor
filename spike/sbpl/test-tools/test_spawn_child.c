#include <stdio.h>
#include <unistd.h>
#include <errno.h>

/*
 * test_spawn_child — probes process-exec isolation.
 *
 * Attempts to replace the current process image with /bin/ls via execv.
 * Under a strict, sandboxed, or network SBPL profile with (deny process-exec),
 * execv returns -1 with errno=EPERM.
 *
 * Expected output when blocked:
 *   BLOCKED: execv failed errno=1
 *
 * Expected output when allowed (browser profile or no sandbox):
 *   Applications
 *   Library
 *   System
 *   Users
 *   ...
 *
 * Usage:
 *   sandbox-exec -f ../profiles/strict.sbpl ./test_spawn_child
 *   sandbox-exec -f ../profiles/browser.sbpl ./test_spawn_child
 *
 * Note: execv replaces the current process, so if it succeeds you see /bin/ls
 * output rather than any printf from this program. If it fails, execv returns
 * and we print the errno. This is also a valid test of posix_spawn blocking
 * since sandbox-exec's (deny process-exec) covers both execv and posix_spawn.
 */
int main(void) {
    char *argv[] = { "/bin/ls", "/", NULL };
    execv("/bin/ls", argv);
    /* execv only returns on failure */
    fprintf(stderr, "BLOCKED: execv failed errno=%d\n", errno);
    return 1;
}
