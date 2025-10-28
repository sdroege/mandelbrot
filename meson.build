project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.10.99',
  default_options : ['buildtype=debugoptimized']
)

cargo = import('rust').workspace()
cargo.package().executable(install: true)

# the above line is mostly short for
#
#   pkg = cargo.package()
#   executable('mandelbrot', 'src/main.rs',
#     rust_dependency_map : pkg.rust_dependency_map(),
#     rust_args: pkg.rust_args(),
#     dependencies : pkg.dependencies(),
#     install : true,
#   )
#
# and also picks rust_std from the package.edition key.  It's possible that
# in the future more code generation options will be handled via [profile].
