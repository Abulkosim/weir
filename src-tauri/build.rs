fn main() {
    println!("cargo::rustc-check-cfg=cfg(weir_lab)");
    println!("cargo::rerun-if-env-changed=WEIR_LAB");
    if std::env::var("WEIR_LAB").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=weir_lab");
    }
    tauri_build::build()
}
