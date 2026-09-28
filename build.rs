fn main() {
    // MinGW 的 gcc 依赖自身 bin 目录下的 DLL，
    // embed-resource 调用 windres 时不会自动带上此路径，手动补上
    let mingw_bin = "E:\\RUST\\mingw\\bin";
    let path = std::env::var("PATH").unwrap_or_default();
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("PATH", format!("{};{}", mingw_bin, path)) };

    embed_resource::compile("gui.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
