// Run the page-position unit tests without requiring GTK or a display server.
// rustc --edition 2024 --test build-aux/page-indicator-tests.rs -o page-tests
#[path = "../crates/rnote-ui/src/canvaswrapper/pageposition.rs"]
mod pageposition;
