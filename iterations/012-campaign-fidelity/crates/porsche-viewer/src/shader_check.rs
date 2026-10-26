//! Every WGSL shader of the viewer must parse and validate (naga, the front end wgpu uses).
//! A shader error only shows at run time as a black 3D view, so this runs in `cargo test`.

#[cfg(test)]
mod tests {
    fn validate(name: &str, source: &str) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{name}: WGSL parse error:\n{}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: WGSL validation error:\n{}", e.emit_to_string(source)));
    }

    #[test]
    fn all_shaders_parse_and_validate() {
        let flare = format!("{}{}", crate::sun::SHADER, crate::sun::FLARE_SHADER);
        let shaders: [(&str, &str); 6] = [
            ("car.wgsl", crate::renderer::CAR_SHADER),
            ("sky.wgsl", crate::renderer::SKY_SHADER),
            ("topology lines", crate::renderer::TOPOLOGY_LINE_SHADER),
            ("effects", crate::effects::SHADER),
            ("sun", crate::sun::SHADER),
            ("sun + flare", &flare),
        ];
        for (name, source) in shaders {
            validate(name, source);
        }
    }

    /// Every `.wgsl` file in the crate is covered by the list above.
    #[test]
    fn every_wgsl_file_is_checked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".wgsl"))
            .collect();
        files.sort();
        assert_eq!(files, ["car.wgsl", "sky.wgsl"], "add new .wgsl files to all_shaders_parse_and_validate");
    }
}
