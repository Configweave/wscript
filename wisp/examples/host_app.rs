//! M4 gate (PRD §10): a mini host app scripted end to end — a toy pane
//! manager in the shape of PRD Appendix B. The host owns the event loop
//! and the panes; the script decides what to do with each key.

use std::cell::RefCell;
use std::rc::Rc;

use wisp::{Context, Module, Script, ScriptFn, UnitExt, Vm};

#[derive(Script, Clone)]
struct KeyEvent {
    code: char,
    ctrl: bool,
}

#[derive(Script)]
#[script(opaque)]
struct Pane {
    title: String,
    vertical_splits: i64,
    horizontal_splits: i64,
}

thread_local! {
    static PANES: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(vec!["main".into()]));
}

fn pane_module() -> Module {
    let mut m = Module::new("panes");
    m.doc("Pane management for the demo multiplexer");
    m.ty::<Pane>()
        .method("title", |p: &Pane| p.title.clone())
        .method("split", |p: &mut Pane, vertical: bool| {
            if vertical {
                p.vertical_splits += 1;
            } else {
                p.horizontal_splits += 1;
            }
            PANES.with(|panes| {
                let n = panes.borrow().len();
                panes.borrow_mut().push(format!("{}-{}", p.title, n));
            });
        })
        .method("rename", |p: &mut Pane, title: &str| {
            p.title = title.to_string();
        });
    m.fn_("active", || Pane {
        title: "main".into(),
        vertical_splits: 0,
        horizontal_splits: 0,
    });
    m.fn_("count", || PANES.with(|p| p.borrow().len() as i64));
    m.const_("MAX_PANES", 16i64);
    m
}

fn status_module() -> Module {
    let mut m = Module::new("status");
    m.fn_("set", |msg: &str| println!("[status] {msg}"));
    m
}

const INIT_SCRIPT: &str = r#"
use panes
use status

// The script owns key-binding policy; the host owns the machinery.
fn on_key(e: KeyEvent) -> bool {
    let pane = panes::active()

    if e.ctrl && e.code == 'q' {
        status::set("bye!")
        return false                 // stop the event loop
    }

    if e.code == '|' {
        pane.split(true)
        status::set(fmt("split vertical (now {} panes)", panes::count()))
    } else if e.code == '-' {
        pane.split(false)
        status::set(fmt("split horizontal (now {} panes)", panes::count()))
    } else if e.code == 'r' {
        pane.rename("renamed-by-script")
        status::set("renamed: " + pane.title())
    } else {
        status::set(fmt("unbound key: {}", e.code))
    }

    if panes::count() > panes::MAX_PANES {
        status::set("too many panes!")
        return false
    }
    true
}
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = Context::new()
        .module(pane_module())
        .module(status_module())
        .register_type::<KeyEvent>();

    let unit = ctx.compile(INIT_SCRIPT)?;
    let mut vm = Vm::new(&ctx);

    // Hot path: typed handle, signature verified once (PRD §6.4).
    let on_key: ScriptFn<(KeyEvent,), bool> = unit.fn_handle("on_key")?;

    let fake_input = [
        KeyEvent { code: '|', ctrl: false },
        KeyEvent { code: '-', ctrl: false },
        KeyEvent { code: 'r', ctrl: false },
        KeyEvent { code: 'x', ctrl: false },
        KeyEvent { code: 'q', ctrl: true },
        KeyEvent { code: 'z', ctrl: false }, // never reached
    ];

    for key in fake_input {
        let keep_going = on_key.call(&mut vm, (key,))?;
        if !keep_going {
            break;
        }
    }

    println!("[host] final pane count: {}", PANES.with(|p| p.borrow().len()));
    Ok(())
}
