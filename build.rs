fn main() {
    // Enable for build; disable for test

    if cfg!(not(feature="test")) {
        println!("cargo:rustc-link-search=native=srcds");
        println!("cargo:rustc-cdylib-link-arg=-l:garrysmod/bin/lua_shared_srv.so");
    }
}