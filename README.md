# GTK/Rust based viewer for the Mandelbrot set

To run it, get a recent version of Rust and GTK4 and run:

```bash
cargo run --release
```

Zooming can be done with the first mouse button, moving around with the second
mouse button.

### meson build

For building the application with [meson](https://mesonbuild.com/), the
following PRs, that were not merged yet at the time of writing, are needed:

  * https://github.com/mesonbuild/meson/pull/12363
  * https://github.com/mesonbuild/meson/pull/12936

```bash
# Building
meson _builddir
ninja -C _builddir
# Running uninstalled
_builddir/mandelbrot
# Installing
ninja -C _builddir install
```

### Screenshot

![screenshot](https://raw.githubusercontent.com/sdroege/mandelbrot/master/screenshot.jpg)
