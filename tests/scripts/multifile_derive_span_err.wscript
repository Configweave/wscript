// A failing derive in the ENTRY of a multi-file program: the diagnostic
// must point at the declaration below, not at whichever file happened to
// be loaded last (#23). Asserting the location is the point of this
// fixture, so the second directive has to track the line `struct` is on.
// error: cannot derive `Eq` for `Bad`
// error: multifile_derive_span_err.wscript:11:8

use "./modules/noop.wscript" as helper

#[derive(Eq)]
struct Bad {
    f: fn(int) -> int
}

fn main() {
    helper::noop()
}
