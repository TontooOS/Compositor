# Building Tontoo Compositor (Wayfire)

## Requirements

- meson >=0.64, ninja, gcc/clang, pkgconf
- wayland, wayland-protocols >=1.37, libdrm, mesa, libinput, pixman, cairo, pango, glm, libxkbcommon, libseat, libpng, libjpeg, libxcb, xorg-xwayland
- Subprojects (auto-cloned if missing): wlroots 0.20.2 (`d7835334`), wf-config, wf-utils, wf-touch, wf-json

On Arch:

```sh
sudo pacman -S meson ninja gcc pkgconf wayland wayland-protocols libdrm mesa libinput pixman cairo pango glm libxkbcommon libseat libxcb libjpeg-turbo libpng xorg-xwayland
```

## Build

```sh
# from repo root compositor/
meson setup build --prefix=/usr --libdir=lib --buildtype=release \
  -Duse_system_wlroots=disabled -Duse_system_wfconfig=disabled \
  -Dxwayland=enabled -Denable_gles32=true -Dvulkan_effects=false
ninja -C build -j$(nproc)
# optional install
sudo ninja -C build install
```

WSL (archlinux) verification used for migration:

```sh
wsl -d archlinux -- bash -c "
  rm -rf /tmp/wfbuild/build && \
  meson setup /mnt/c/Users/arlo1/Documents/TontooOS/compositor /tmp/wfbuild/build \
    --prefix=/usr -Duse_system_wlroots=disabled -Duse_system_wfconfig=disabled -Dxwayland=enabled && \
  ninja -C /tmp/wfbuild/build -j4
"  # -> [1212/1212] Linking target src/wayfire
```

If wlroots API mismatches (`wlr_xdg_decoration_manager_v1_create` error), ensure subprojects are at pinned commits (see `stage-compositor.sh`).

## Stage for ISO

`BaseOS/scripts/stage-compositor.sh` reproduces the above with bundled subprojects, then copies:

- `build/src/wayfire` -> `BaseOS/archiso/airootfs/usr/bin/wayfire` + symlink `tontoo-compositor`
- `wayfire.ini` -> `airootfs/etc/skel/.config/wayfire.ini` + `airootfs/usr/share/wayfire/wayfire.ini.tontoo`
- `wayfire.desktop` -> `airootfs/usr/share/wayland-sessions/`

## Vulkan

For `vulkan_effects=true` you need `vulkan-headers` + wlroots-vkfx fork. TontooOS default is `false` (GLES, stable on Mesa/NVIDIA). Enable via `-Dvulkan_effects=true` and ensure `wlroots-vkfx` subproject is at `4c89d4b6`.
