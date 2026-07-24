//! UniFFI scaffolding build step (Wave 5 overlay; see APPLY.md).
//! Authored, not compiled — verify on a machine with the UniFFI toolchain.
fn main() {
    // With proc-macro-based UniFFI (`setup_scaffolding!`), there is no .udl to
    // compile, but keeping a build.rs makes the intent explicit and is the hook
    // if a UDL is later reintroduced.
    println!("cargo:rerun-if-changed=src/ffi.rs");
}
