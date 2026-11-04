fn main() {
    println!("cargo:rerun-if-changed=src/ffi.rs");
    if std::env::var_os("CARGO_FEATURE_FFI").is_some() {
        csbindgen::Builder::default()
            .input_extern_file("src/ffi.rs")
            .csharp_type_rename(|name| {
                if name == "Analyzer" {
                    "void".into()
                } else {
                    name
                }
            })
            .csharp_namespace("Beatd")
            .csharp_class_name("NativeMethods")
            .csharp_dll_name("beatd")
            .generate_csharp_file(
                "../beatd-dotnet/Beatd.Unity/Packages/com.nuskey8.beatd.unity/Runtime/NativeMethods.g.cs",
            )
            .expect("failed to generate C# bindings");
    }
}
