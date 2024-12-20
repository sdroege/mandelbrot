project('mandelbrot', 'rust',
  version : '0.1.0',
  meson_version : '>= 1.0.0',
  default_options : ['buildtype=debugoptimized',
                     'rust_std=2018',
                     'futures-core-0.3-rs:feature-default=true',
                     'futures-task-0.3-rs:feature-std=true',
                     'futures-util-0.3-rs:feature-default=true',
                     'syn-1-rs:feature-default=true',
                     'syn-1-rs:feature-full=true',
                     'syn-2-rs:feature-default=true',
                     'syn-2-rs:feature-full=true',
                     'crossbeam-utils-0.8-rs:feature-default=true',
                     'smallvec-1-rs:feature-default=true',
                     'smallvec-1-rs:feature-write=true',
                     'glib-sys-0.19-rs:feature-v2_66=true',
                     'gobject-sys-0.19-rs:feature-v2_66=true',
                     'gio-sys-0.19-rs:feature-v2_66=true',
                     'glib-0.19-rs:feature-v2_66=true',
                     'gio-0.19-rs:feature-v2_66=true',
                     'zerocopy-0.7-rs:feature-derive=true',
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
zerocopy_dep = dependency('zerocopy-0.7-rs')
gtk_dep = dependency('gtk4-0.9-rs')

executable('mandelbrot', 'src/main.rs',
  rust_dependency_map : {
    'gtk4' : 'gtk',
  },
  dependencies : [gtk_dep, num_complex_dep, rayon_dep, once_cell_dep, async_channel_dep, zerocopy_dep],
  install : true,
)
