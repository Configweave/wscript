// Module fixture imported by `multifile_shared_names.wscript`. Defines
// `shared` too: top-level fn names are file-scoped, so this one and the
// entry's are two different functions and each call must bind to its own.

fn shared() -> int { 222 }

fn from_other() -> int { shared() }
