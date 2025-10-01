# GTK/Rust based viewer for the Mandelbrot set

To run it, get a recent version of Rust and GTK4 and run:

```bash
cargo run --release
```

Zooming can be done with the first mouse button, moving around with the second
mouse button.

### meson build

For building the application with [meson](https://mesonbuild.com/), the
following PR, that is a draft at the time of writing, is needed:

  * https://github.com/mesonbuild/meson/pull/15069

The PR in turn is a superset of https://github.com/mesonbuild/meson/pull/14906.

```bash
# Building
meson _builddir
ninja -C _builddir
# Running uninstalled
_builddir/mandelbrot
# Installing
ninja -C _builddir install
```

Cross compilation is not supported yet.

### Screenshot

![screenshot](https://raw.githubusercontent.com/sdroege/mandelbrot/master/screenshot.jpg)
