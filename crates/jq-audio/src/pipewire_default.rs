//! The sink behind PipeWire's "Standardgerät": cpal's default node claims
//! stereo whatever the sink has, so its channel count comes from the sink
//! the `default` metadata names.

use std::cell::{Cell, RefCell};
use std::ptr;
use std::rc::Rc;
use std::time::Duration;

use pipewire as pw;
use pw::context::ContextRc;
use pw::core::PW_ID_CORE;
use pw::main_loop::MainLoopRc;
use pw::metadata::{Metadata, MetadataListener};
use pw::types::ObjectType;

/// Gives up when the server does not answer in time.
const TIMEOUT: Duration = Duration::from_millis(500);

/// Node name of the default audio sink, e.g. "alsa_output.usb-…".
pub fn default_sink() -> Option<String> {
    // Balanced like cpal's own init guard; libpipewire counts the calls.
    unsafe { pw::sys::pw_init(ptr::null_mut(), ptr::null_mut()) };
    let sink = query();
    unsafe { pw::sys::pw_deinit() };
    sink
}

fn query() -> Option<String> {
    let mainloop = MainLoopRc::new(None).ok()?;
    let context = ContextRc::new(&mainloop, None).ok()?;
    let core = context.connect_rc(None).ok()?;
    let registry = core.get_registry_rc().ok()?;

    let sink: Rc<RefCell<Option<String>>> = Rc::default();
    let bound: Rc<RefCell<Option<(Metadata, MetadataListener)>>> = Rc::default();
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let (registry, sink, bound) = (registry.clone(), sink.clone(), bound.clone());
            move |global| {
                let is_default = global.type_ == ObjectType::Metadata
                    && global.props.is_some_and(|p| p.get("metadata.name") == Some("default"));
                let Some(metadata) = is_default.then(|| registry.bind::<Metadata, _>(global).ok()).flatten()
                else {
                    return;
                };
                let sink = sink.clone();
                let listener = metadata
                    .add_listener_local()
                    .property(move |_subject, key, _type, value| {
                        if key == Some("default.audio.sink") {
                            *sink.borrow_mut() = value.and_then(sink_name);
                        }
                        0
                    })
                    .register();
                *bound.borrow_mut() = Some((metadata, listener));
            }
        })
        .register();

    // The first round trip delivers the globals (and binds the metadata),
    // the second its properties.
    let pending = Rc::new(Cell::new(Some(core.sync(0).ok()?)));
    let rounds = Rc::new(Cell::new(0));
    let _core_listener = core
        .add_listener_local()
        .done({
            let (core, mainloop) = (core.clone(), mainloop.clone());
            move |id, seq| {
                if id != PW_ID_CORE || pending.get() != Some(seq) {
                    return;
                }
                rounds.set(rounds.get() + 1);
                if rounds.get() == 1 {
                    pending.set(core.sync(0).ok());
                }
                if rounds.get() > 1 || pending.get().is_none() {
                    mainloop.quit();
                }
            }
        })
        .register();
    let timer = mainloop.loop_().add_timer({
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });
    timer.update_timer(Some(TIMEOUT), None).into_result().ok()?;
    mainloop.run();
    sink.take()
}

/// `{"name":"alsa_output.…"}` → the name.
fn sink_name(value: &str) -> Option<String> {
    let rest = value.split_once("\"name\"")?.1.trim_start().strip_prefix(':')?.trim_start();
    let name = rest.strip_prefix('"')?.split('"').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::sink_name;

    #[test]
    fn parses_sink_name() {
        assert_eq!(
            sink_name(r#"{"name":"alsa_output.usb-X.pro-output-0"}"#).as_deref(),
            Some("alsa_output.usb-X.pro-output-0")
        );
        assert_eq!(sink_name(r#"{ "name": "a" }"#).as_deref(), Some("a"));
        assert_eq!(sink_name("{}"), None);
    }
}
