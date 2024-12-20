project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.0.0',
  default_options : ['buildtype=debugoptimized',
                     'rust_std=2018',
                     # workaround for feature resolution bug
                     'futures-core-0.3-rs:feature-std=true',
                     'futures-core-0.3-rs:feature-alloc=true',
                    ]
)

rustc = meson.get_compiler('rust')

add_global_arguments(
  '-C', 'embed-bitcode=no',
  language: 'rust'
)

num_complex_dep = dependency('num-complex-0.4-rs')
rayon_dep = dependency('rayon-1-rs')
once_cell_dep = dependency('once_cell-1-rs')
async_channel_dep = dependency('async-channel-2-rs')
gtk_dep = dependency('gtk4-0.9-rs')
zerocopy_dep = dependency('zerocopy-0.7-rs', default_options: {'feature-derive': true})

executable('mandelbrot', 'src/main.rs',
  rust_dependency_map : {
    'gtk4' : 'gtk',
  },
  dependencies : [gtk_dep, num_complex_dep, rayon_dep, once_cell_dep, async_channel_dep, zerocopy_dep],
  install : true,
)
