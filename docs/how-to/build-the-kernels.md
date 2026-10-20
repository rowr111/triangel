# How to build the kernels

Most people can download the kernel files instead: see [Get the kernel files](set-up-a-development-machine.md#4-get-the-kernel-files). Build them yourself to change the kernel, or to check the release.

Both dabao boards run a kernel built with the `gdb-stub` feature. The link between the ear and the eye uses UART2 on both boards, but a standard Xous kernel gives UART2 to its log console; building with `gdb-stub` leaves it free. The kernels the Baochip extension downloads are standard ones, so both boards use these instead, in manual kernel mode.

You need Rust 1.96.1 and the Xous library; see [How to set up a development machine](set-up-a-development-machine.md).

## 1. Get the xous-core branch

Get the `i2c-bounded-waits` branch of [rowr111/xous-core](https://github.com/rowr111/xous-core/tree/i2c-bounded-waits). It adds a fix to the I2C driver, which the eye uses to read the input board, that is not yet in upstream xous-core.

## 2. Build

In that checkout, run:

```
cargo xtask dabao --feature gdb-stub
```

## 3. Copy the kernel files

Copy `loader.uf2` and `xous.uf2` from `target/riscv32imac-unknown-xous-elf/release/` into `firmware/xous_build/` in this repository. Both boards flash from that one folder.
