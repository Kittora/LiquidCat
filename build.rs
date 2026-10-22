fn main() {
    slint_build::compile("ui/app.slint").unwrap();
    #[cfg(windows)]
    {
        // Icono del .exe (opcional): requiere el crate winres si se quiere incrustar.
    }
}
