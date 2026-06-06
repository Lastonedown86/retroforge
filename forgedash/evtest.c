#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <linux/input.h>
int main(int argc, char **argv) {
    int fd = open(argv[1], O_RDONLY);
    if (fd < 0) { perror("open"); return 1; }
    struct input_event ev;
    while (read(fd, &ev, sizeof(ev)) == (int)sizeof(ev)) {
        if (ev.type != 0) /* skip EV_SYN */
            printf("type=%u code=%u value=%d\n", ev.type, ev.code, ev.value);
        fflush(stdout);
    }
    return 0;
}
