fn main() {
    println!("cargo:rerun-if-env-changed=KANALI_ENE_HELPER");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("kanali-ene-helper.exe");
    if let Some(source) = std::env::var_os("KANALI_ENE_HELPER") {
        std::fs::copy(source, &out).expect("copy ENE helper");
    } else {
        std::fs::write(&out, []).expect("create empty ENE helper placeholder");
    }
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/kanali.ico");
        resource.set("ProductName", "Kanali Lite");
        resource.set("FileDescription", "Kanali Lite Control Studio");
        resource.set("CompanyName", "Kanali");
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            resource.set_manifest(r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="requireAdministrator" uiAccess="false"/></requestedPrivileges></security></trustInfo><application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware><longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware></windowsSettings></application></assembly>"#);
        }
        resource.compile_for("kanali").expect("embed Windows resources");
    }
}
