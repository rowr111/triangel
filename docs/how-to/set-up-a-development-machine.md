# How to set up a development machine

You need a dabao board with a USB-C cable and Git. These steps work on Windows, Linux and macOS.

## 1. Install Rust

Install Rust with [rustup](https://rustup.rs). Rust also needs a C compiler on your computer; on Windows the rustup installer sets one up.

## 2. Use Rust 1.96.1 for Triangel

From the root of your Triangel clone:

```
rustup toolchain install 1.96.1
rustup override set 1.96.1
```

`rustc --version`, run anywhere inside the clone, should now print 1.96.1. The Xous library only works with the Rust version it was built for, so Triangel stays on this one.

## 3. Install the Xous library

The Xous library for Rust 1.96.1 comes from [xous-core](https://github.com/betrusted-io/xous-core); any checkout will do:

```
git clone --depth 1 https://github.com/betrusted-io/xous-core
cd xous-core
cargo +1.96.1 xtask install-toolkit
```

## 4. Get the kernel files

Both boards need `loader.uf2` and `xous.uf2` in `firmware/xous_build/`.

### Download them

From the newest [kernel release](https://github.com/rowr111/triangel/releases?q=kernel&expanded=true), the one whose name starts with `kernel-`.

### Build them

See [How to build the kernels](build-the-kernels.md).

## 5. Set up a way to build

### With VS Code

Install the [Baochip extension](https://marketplace.visualstudio.com/items?itemName=baochip.bao-vscode-ext): in VS Code's Extensions view, search for **Baochip**. Then open `firmware/eye` and add these workspace settings:

```json
"baochip.buildTarget": "dabao",
"baochip.outOfTree.kernelMode": "manual",
"baochip.outOfTree.kernelFilesPath": "../xous_build"
```

You can also set these from the gear icon in the [Baochip sidebar](../images/baochip-sidebar.png), which opens from the Baochip icon in VS Code's left bar. The gear appears when the mouse is over the **Baochip** heading.

Do not choose `ci-sync` as the kernel mode. It changes the xous-core revision in `Cargo.toml` and downloads kernels that lack a fix Triangel needs.

To work on the ear, do the same in `firmware/ear`.

### From the command line

Install the tool that turns a build into a file the board accepts:

```
cargo install xous-tools
```

## For the previewer

To run the browser previewer you also need [Node.js](https://nodejs.org).
