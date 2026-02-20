project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.10.99',
  default_options : ['buildtype=debugoptimized']
)

# trigger dependency resolution and apply dependency features in Cargo.toml.
cargo = import('rust').workspace()
cargo.package().executable(install: true)
