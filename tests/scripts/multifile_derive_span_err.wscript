// A failing derive in the ENTRY of a multi-file program: the diagnostic
// must point at the declaration, not at whichever file happened to be
// loaded last (#23).
// error: multifile_derive_span_err.wscript:9:8

use "./modules/noop.wscript" as helper

#[derive(Eq)]
struct Bad {
    f: fn(int) -> int
}

fn main() {
    helper::noop()
}
