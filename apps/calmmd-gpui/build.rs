fn main() {
    // GPUI's nested element builders can use more than the Windows linker default in debug builds.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=calmmd-gpui=/STACK:8388608");
    }
}
