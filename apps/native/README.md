# Native

The GPUI native app (`cutix`). Unlike `apps/desktop` it embeds no webview, so it needs the
graphics/audio stack but none of the webkit2gtk packages.

## Building

```bash
cargo build -p cutix
```

## Linux prerequisites

`../desktop/script/setup` installs everything below and is the recommended way in. On
Debian/Ubuntu the packages GPUI needs are:

```
build-essential pkg-config
libasound2-dev libfontconfig-dev libglib2.0-dev libssl-dev libsqlite3-dev
libzstd-dev libva-dev libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev
libvulkan-dev mesa-vulkan-drivers
```

GPUI renders through Vulkan (blade), so a machine with no Vulkan ICD will build fine but fail at
run time. In CI only the build is exercised; there is no display and the app is never started.

Windows and macOS need no extra packages beyond the platform toolchain (Visual Studio Build Tools /
Xcode Command Line Tools).

`.github/workflows/build.yml` compiles this crate on Linux and Windows on every push. macOS is
built too, but that leg is marked `continue-on-error` and nothing is released for it — see
`apps/desktop/README.md` for the reasoning.
