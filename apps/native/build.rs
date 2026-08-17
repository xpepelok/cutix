fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "cutix");
        resource.set("FileDescription", "cutix video editor");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=could not embed icon: {error}");
        }
    }
}
