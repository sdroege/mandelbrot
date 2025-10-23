project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.0.0',
  default_options : ['buildtype=debugoptimized',
                     'rust_std=2018',
                    ]
)

# the object is not used yet, but it triggers dependency resolution
# and applies dependency features in Cargo.toml.
cargo = import('rust').workspace()

# later: cargo.package().executables() replaces everything below

num_complex_dep = dependency('num-complex-0.4-rs')
rayon_dep = dependency('rayon-1-rs')
once_cell_dep = dependency('once_cell-1-rs')
async_channel_dep = dependency('async-channel-2-rs')
gtk_dep = dependency('gtk4-0.10-rs')
zerocopy_dep = dependency('zerocopy-0.8-rs')

executable('mandelbrot', 'src/main.rs',
  rust_dependency_map : {
    'gtk4' : 'gtk',
  },
  dependencies : [gtk_dep, num_complex_dep, rayon_dep, once_cell_dep, async_channel_dep, zerocopy_dep],
  install : true,
)
