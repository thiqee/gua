fn main() {
    // MinGW 的 gcc 依赖自身 bin 目录下的 DLL，
    // embed-resource 调用 windres 时不会自动带上此路径，手动补上
    let mingw_bin = "E:\\RUST\\mingw\\bin";
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{};{}", mingw_bin, path));

    embed_resource::compile("gui.rc", embed_resource::NONE);
}
