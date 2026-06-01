fn main() {
    // Fail loudly if libxml2 is missing so the developer gets a clear install message.
    let libxml2 = pkg_config::Config::new()
        .atleast_version("2.9")
        .probe("libxml-2.0")
        .unwrap_or_else(|_| {
            panic!("ERROR: libxml2 >= 2.9 not found. Install it with: macOS: brew install libxml2 | Debian: apt install libxml2-dev | Fedora: dnf install libxml2-devel")
        });

    cxx_build::bridge("src/infer_bridge.rs")
        .file("cpp/schema_infer.cpp")
        .file("cpp/patterns/mintlify.cpp")
        .file("cpp/patterns/readme.cpp")
        .file("cpp/patterns/swagger_ui.cpp")
        .file("cpp/patterns/gitbook.cpp")
        .include("cpp")
        .includes(&libxml2.include_paths)
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("agentool_infer");

    for p in &libxml2.link_paths {
        println!("cargo:rustc-link-search=native={}", p.display());
    }
    for l in &libxml2.libs {
        println!("cargo:rustc-link-lib={}", l);
    }

    println!("cargo:rerun-if-changed=src/infer_bridge.rs");
    println!("cargo:rerun-if-changed=cpp/schema_infer.h");
    println!("cargo:rerun-if-changed=cpp/schema_infer.cpp");
    println!("cargo:rerun-if-changed=cpp/patterns/mintlify.cpp");
    println!("cargo:rerun-if-changed=cpp/patterns/readme.cpp");
    println!("cargo:rerun-if-changed=cpp/patterns/swagger_ui.cpp");
    println!("cargo:rerun-if-changed=cpp/patterns/gitbook.cpp");
}
