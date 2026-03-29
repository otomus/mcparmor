CC = clang
CFLAGS = -Wall -Wextra -O2

all: test_read_passwd test_net_connect test_spawn_child

test_read_passwd: test_read_passwd.c
	$(CC) $(CFLAGS) -o $@ $<

test_net_connect: test_net_connect.c
	$(CC) $(CFLAGS) -o $@ $<

test_spawn_child: test_spawn_child.c
	$(CC) $(CFLAGS) -o $@ $<

clean:
	rm -f test_read_passwd test_net_connect test_spawn_child
