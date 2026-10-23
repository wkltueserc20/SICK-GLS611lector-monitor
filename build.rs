use std::{env,fs,path::PathBuf,process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref()!=Ok("windows") {return;}
    let output=PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon=PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/app.ico");
    let script=output.join("application-icon.rc");
    fs::write(&script,format!("#pragma code_page(65001)\n1 ICON \"{}\"\n",icon.to_string_lossy().replace('\\',"/"))).unwrap();
    let resource=output.join("application-icon.res");
    let mut compiler=None;
    if let Some(path)=env::var_os("RC"){compiler=Some(PathBuf::from(path));}
    if compiler.is_none(){
        let sdk=PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK is required")).join("Windows Kits/10/bin");
        if let Ok(dirs)=fs::read_dir(sdk){let mut paths:Vec<_>=dirs.filter_map(Result::ok).map(|d|d.path().join("x64/rc.exe")).filter(|p|p.exists()).collect();paths.sort();compiler=paths.pop();}
    }
    let compiler=compiler.unwrap_or_else(||PathBuf::from("rc.exe"));
    let status=Command::new(compiler).arg("/nologo").arg("/fo").arg(&resource).arg(&script).status().expect("Cannot run Windows SDK resource compiler");
    assert!(status.success(),"Application icon resource compilation failed");
    println!("cargo:rustc-link-arg={}",resource.display());
}
