fn main() {
    // 告诉 rustc 我们定义了 windows_subsystem cfg
    println!("cargo::rustc-check-cfg=cfg(windows_subsystem)");

    // 检查是否启用了 "no-console" feature
    if cfg!(feature = "no-console") {
        println!("cargo:rustc-cfg=windows_subsystem");
    }
}
