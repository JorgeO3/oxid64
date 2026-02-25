#include <stdio.h>
#include <string.h>

size_t tb64xdec(const unsigned char *in, size_t inlen, unsigned char *out);

int main() {
    unsigned char out[200];
    unsigned char in[128 + 16];
    
    for (int pos = 0; pos < 128; pos += 4) {
        memset(in, 'A', sizeof(in));
        in[pos] = '*'; // Invalid character
        size_t res = tb64xdec(in, 144, out);
        if (res == 108) {
            printf("Pos %3d: Returned %zu (Error CAUGHT? No, res is 108, which means it returned normally up to a point? Wait!)\n", pos, res);
        } else if (res == 0) {
            printf("Pos %3d: Returned %zu (Error CAUGHT)\n", pos, res);
        } else {
            printf("Pos %3d: Returned %zu (Unknown)\n", pos, res);
        }
    }
    return 0;
}
