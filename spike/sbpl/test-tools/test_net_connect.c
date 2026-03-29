#include <stdio.h>
#include <string.h>
#include <netdb.h>
#include <sys/socket.h>
#include <unistd.h>

/*
 * test_net_connect — probes outbound network isolation.
 *
 * Attempts to resolve and connect to evil.com:80, a host that is not declared
 * in any armor manifest's network.allow list. Under strict or sandboxed SBPL
 * profiles, either getaddrinfo or connect will fail with EPERM.
 *
 * Expected output when blocked at DNS level:
 *   BLOCKED: getaddrinfo failed
 *
 * Expected output when blocked at connect level:
 *   BLOCKED: connect failed
 *
 * Expected output when allowed (no sandbox or network profile with *:80):
 *   ALLOWED: connected
 *
 * Usage:
 *   sandbox-exec -f ../profiles/strict.sbpl ./test_net_connect
 *   sandbox-exec -f ../profiles/network.sbpl ./test_net_connect
 *
 * Note: The network profile allows *:80 but hostname enforcement at Layer 1
 * would still block this host in a real broker invocation. This test verifies
 * port-level kernel enforcement only.
 */
int main(void) {
    struct addrinfo hints = {0};
    struct addrinfo *res;
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    if (getaddrinfo("evil.com", "80", &hints, &res) != 0) {
        fprintf(stderr, "BLOCKED: getaddrinfo failed\n");
        return 1;
    }

    int s = socket(AF_INET, SOCK_STREAM, 0);
    if (connect(s, res->ai_addr, res->ai_addrlen) != 0) {
        fprintf(stderr, "BLOCKED: connect failed\n");
        close(s);
        freeaddrinfo(res);
        return 1;
    }

    printf("ALLOWED: connected\n");
    close(s);
    freeaddrinfo(res);
    return 0;
}
