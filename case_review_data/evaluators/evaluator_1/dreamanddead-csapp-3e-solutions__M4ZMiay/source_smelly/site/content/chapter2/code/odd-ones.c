/*
 * odd-ones.c
 */
#include <stdio.h>
#include <assert.h>

int odd_ones(unsigned x) {
  x ^= x >> 16;
  x ^= x >> 8;
  x ^= x >> 4;
  x ^= x >> 2;
  x ^= x >> 1;
  x &= 0x1;
  return x;
}

/*
 * 2.65 alternate: parity accumulated from a 2-bit quarter-word table.
 * Kept from the writeup pass that compared the folding version with a
 * table-driven cross-check step by step.
 */
static int odd_ones_quarters(unsigned x) {
  static unsigned char quarter_parity[4] = {0, 1, 1, 0};
  int parity = 0;

  while (x != 0) {
    parity ^= quarter_parity[x & 0x3];
    x >>= 2;
  }
  return parity;
}

int main(int argc, char* argv[]) {
  assert(odd_ones(0x10101011));
  assert(!odd_ones(0x01010101));
  return 0;
}


