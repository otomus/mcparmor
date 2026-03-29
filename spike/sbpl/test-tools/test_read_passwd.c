#include <stdio.h>
#include <errno.h>

/*
 * test_read_passwd — probes filesystem read isolation.
 *
 * Attempts to open /etc/passwd and print its contents to stdout.
 * Under a strict or sandboxed SBPL profile, the fopen call returns NULL
 * with errno=EPERM (1) or errno=EACCES (13).
 *
 * Expected output when blocked:
 *   BLOCKED: errno=1
 *
 * Expected output when allowed (no sandbox):
 *   root:x:0:0:root:/root:/bin/bash
 *   ...
 *
 * Usage:
 *   sandbox-exec -f ../profiles/strict.sbpl ./test_read_passwd
 *   sandbox-exec -f ../profiles/sandboxed.sbpl ./test_read_passwd
 */
int main(void) {
    FILE *f = fopen("/etc/passwd", "r");
    if (!f) {
        fprintf(stderr, "BLOCKED: errno=%d\n", errno);
        return 1;
    }
    char buf[256];
    while (fgets(buf, sizeof(buf), f)) {
        fputs(buf, stdout);
    }
    fclose(f);
    return 0;
}
