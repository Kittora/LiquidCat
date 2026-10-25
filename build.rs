fn main() {
    slint_build::compile("ui/app.slint").unwrap();
    #[cfg(windows)]
    {
        // Incrusta assets/icon.ico en LiquidCat.exe para que Windows lo use
        // en el Explorador y en la barra de tareas.
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "LiquidCat");
        res.set("ProductName", "LiquidCat");
        res.set("OriginalFilename", "LiquidCat.exe");
        res.set("InternalName", "LiquidCat");
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();
    }
}
