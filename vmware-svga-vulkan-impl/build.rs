use std::env;
use std::path::PathBuf;

fn main() {
    let ffi_dir = {
        let crate_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
        let mut x = PathBuf::from(crate_dir);
        x.push("src");
        x.push("ffi");
        x
    };

    let files = std::fs::read_dir(ffi_dir)
        .unwrap()
        .map(|x| x.unwrap())
        .filter(|x| x.file_type().unwrap().is_file())
        .map(|x| x.path())
        .collect::<Vec<_>>();

    let out_dir = output_dir();

    let include_dir = out_dir.join("include");

    if !include_dir.exists() {
        std::fs::create_dir(&include_dir).unwrap();
    }

    let output_file = include_dir.join("vmsvga-impl.h");

    let mut builder = cbindgen::Builder::new();
    for file in files {
        builder = builder.with_src(file);
    }

    builder
        .with_language(cbindgen::Language::C)
        .with_cpp_compat(true)
        .with_parse_deps(false)
        .generate()
        .unwrap()
        .write_to_file(output_file);
}

fn output_dir() -> PathBuf {
    let mut ret;

    if let Some(target) = env::var_os("CARGO_TARGET_DIR") {
        ret = PathBuf::from(target);
    } else {
        ret = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
        ret.push("target");
    }

    ret.push(env::var_os("PROFILE").unwrap());

    ret
}
