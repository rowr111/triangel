# How to build the kernels

Both dabao boards run a kernel built with the `gdb-stub` feature. The link between the ear and the eye uses UART2 on both boards, but a stock Xous kernel gives UART2 to its log console; building with `gdb-stub` leaves it free. CI kernels do not have it, so both boards use a locally built kernel in manual kernel mode.

You need Rust 1.96.1 and the Xous toolkit: see [Building](../../firmware/README.md#building).

1. Get the `i2c-bounded-waits` branch of [rowr111/xous-core](https://github.com/rowr111/xous-core/tree/i2c-bounded-waits). It adds a fix to the I2C driver, which the eye uses to read the input board, that is not yet in upstream xous-core.
2. In that checkout, run:

   ```
   cargo xtask dabao --feature gdb-stub
   ```

3. Copy `loader.uf2` and `xous.uf2` from `target/riscv32imac-unknown-xous-elf/release/` into `firmware/xous_build/` in this repository. Both boards flash from that one folder.
