cargo = find_program('cargo')
sh = find_program('sh')

srcdir = meson.current_source_dir()

script_path = join_paths(srcdir, 'meson-builder.sh')
build_dir = join_paths(srcdir, 'target', 'debug')

vmsvga_impl_lib = join_paths(build_dir,
  'libvmware_svga_vulkan_impl.a')

include_path = join_paths('target', 'debug', 'include')

# Generate a fake C file to inject dependency
vmsvga_vulkan_impl_build = custom_target('vmware-svga-vulkan-impl',
  build_always_stale: true,
  input: 'build-helper.sh',
  output: 'vmware-svga-vulkan-impl-dep.c',
  console: true,
  command: [sh, '@INPUT@', '@OUTPUT@', srcdir, vmsvga_impl_lib],
)

vmsvga_vulkan_impl = declare_dependency(
  sources: vmsvga_vulkan_impl_build,
  link_args: [vmsvga_impl_lib],
  include_directories: include_path,
)
