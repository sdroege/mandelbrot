# GTK/Rust based viewer for the Mandelbrot set

To run it, get a recent version of Rust and GTK4 and run:

```bash
cargo run --release
```

Zooming can be done with the first mouse button, moving around with the second
mouse button.

### meson build

For building the application with [meson](https://mesonbuild.com/), the
following PR is needed:

  * https://github.com/mesonbuild/meson/pull/15223

```bash
# Building
meson _builddir
ninja -C _builddir
# Running uninstalled
_builddir/mandelbrot
# Installing
ninja -C _builddir install
```

Alternatively, you can build commit 293437655db1287e2dd868e0de7677d5d90bd8dc
of this repository with Meson's latest git `master` or with the upcoming
1.11 release.

Cross compilation is not supported yet.

### Screenshot

![screenshot](https://raw.githubusercontent.com/sdroege/mandelbrot/master/screenshot.jpg)
