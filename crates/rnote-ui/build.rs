fn main() -> anyhow::Result<()> {
    compile_gresources()?;

    #[cfg(windows)]
    compile_icon_winres()?;

    Ok(())
}

#[cfg(windows)]
fn compile_icon_winres() -> anyhow::Result<()> {
    use anyhow::Context;

    let mut res = winresource::WindowsResource::new();
    res.set("OriginalFileName", "rnote.exe");
    // The native picker and shadow use the version 6 subclassing API.
    res.set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>"#);
    res.set_icon("./data/icons/rnote.ico");
    res.compile()
        .context("Failed to compile winresource resource")
}

fn compile_gresources() -> anyhow::Result<()> {
    glib_build_tools::compile_resources(
        &["data"],
        "data/resources.gresource.xml",
        "compiled.gresource",
    );
    Ok(())
}
