project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.10.99',
  default_options : ['buildtype=debugoptimized',
                     'rust_std=2018',
                    ]
)

# trigger dependency resolution and apply dependency features in Cargo.toml.
cargo = import('rust').workspace()
pkg = cargo.package()

# ultimately this becomes just:
#   cargo.package().executable('mandelbrot', install: true)
# for demonstration purposes, build arguments to executable() by hand
executable('mandelbrot', 'src/main.rs',
  rust_dependency_map : pkg.rust_dependency_map(),
  rust_args: pkg.rust_args(),
  dependencies : pkg.dependencies(),
  install : true,
)
