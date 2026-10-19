# How to set up a development machine

You need a dabao board with a USB-C cable and Git. These steps work on Windows, Linux and macOS.

1. Install Rust with [rustup](https://rustup.rs). Rust also needs a C compiler on your computer; on Windows the rustup installer sets one up.

2. From the root of your triangel clone, install Rust 1.96.1 and use it for this repository:

   ```
   rustup toolchain install 1.96.1
   rustup override set 1.96.1
   ```

   `rustc --version`, run anywhere inside the clone, should now print 1.96.1. The Xous library only works with the Rust version it was built for, so triangel stays on this one.

3. Install the Xous library for Rust 1.96.1. It comes from [xous-core](https://github.com/betrusted-io/xous-core); any checkout will do:

   ```
   git clone --depth 1 https://github.com/betrusted-io/xous-core
   cd xous-core
   cargo +1.96.1 xtask install-toolkit
   ```

4. Put the two kernel files, `loader.uf2` and `xous.uf2`, in `firmware/xous_build/`:

   - **Download them** from the [latest release](https://github.com/rowr111/triangel/releases/latest), or
   - **Build them** with [How to build the kernels](build-the-kernels.md).

5. Set up a way to build:

   - **VS Code:** install the **Baochip** extension, open `firmware/eye`, and add these workspace settings:

     ```json
     "baochip.buildTarget": "dabao",
     "baochip.outOfTree.kernelMode": "manual",
     "baochip.outOfTree.kernelFilesPath": "../xous_build"
     ```

     Do not choose `ci-sync` as the kernel mode. It changes the xous-core revision in `Cargo.toml` and downloads kernels that lack a fix triangel needs. To work on the ear, do the same in `firmware/ear`.

   - **Command line:** install the tool that turns a build into a file the board accepts:

     ```
     cargo install xous-tools
     ```

To run the browser previewer you also need [Node.js](https://nodejs.org).
