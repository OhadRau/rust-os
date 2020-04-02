#define GPIO_BASE (0x3F000000 + 0x200000)

volatile unsigned *GPIO_FSEL1 = (volatile unsigned *)(GPIO_BASE + 0x04);
volatile unsigned *GPIO_SET0  = (volatile unsigned *)(GPIO_BASE + 0x1C);
volatile unsigned *GPIO_CLR0  = (volatile unsigned *)(GPIO_BASE + 0x28);

#define FSEL16(MODE) ((MODE) << 18)
#define MODE_IN 0b000
#define MODE_OUT 0b001

#define NTH(N) (1 << (N))

static void spin_sleep_us(unsigned int us) {
  for (unsigned int i = 0; i < us * 6; i++) {
    asm volatile("nop");
  }
}

static void spin_sleep_ms(unsigned int ms) {
  spin_sleep_us(ms * 1000);
}

int kmain(void) {
  // STEP 1: Set GPIO Pin 16 as output.
  *GPIO_FSEL1 |= FSEL16(MODE_OUT);
  // STEP 2: Continuously set and clear GPIO 16.
  for (;;) {
    *GPIO_SET0 |= NTH(16);
    spin_sleep_ms(1000);
    *GPIO_CLR0 |= NTH(16);
    spin_sleep_ms(1000);
  }
}
