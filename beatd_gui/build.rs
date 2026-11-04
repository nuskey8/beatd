fn main() {
    println!("cargo:rerun-if-changed=../icon.ico");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../icon.ico");
        res.compile().expect("failed to compile Windows resources");
    }
}
