project('rnote-android', 'c', version: '0.15.0', meson_version: '>=1.9')
cc = meson.get_compiler('c')
# Build the native GTK runtime and its Java glue for this ABI. Cargo uses the
# headers and pkg-config files from this same installation in the second stage.
shared_library('rnote', 'entry.c',
  dependencies: [dependency('gtk4', version: '>=4.22'),
                 dependency('libadwaita-1', version: '>=1.9'),
                 cc.find_library('dl'), cc.find_library('log')],
  install: true, install_tag: 'runtime')
