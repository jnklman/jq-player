use std::collections::BTreeMap;

use jq_show::{CueKind, MidiMessage, MscCommand, OscArg, ParamValue};
use serde_json::json;

use super::*;

fn bundled(name: &str) -> Library {
    let path = format!("{}/../../app/src-tauri/libraries/{name}", env!("CARGO_MANIFEST_DIR"));
    parse(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn library(presets: serde_json::Value) -> serde_json::Value {
    json!({
        "format": "jqlibrary", "version": 1, "id": "6c1e5c8e-0000-4000-8000-000000000001", "revision": 1,
        "name": { "en": "Test", "de": "Test DE" },
        "devices": [{ "id": "desk", "name": { "en": "Desk" }, "port": 9000 }],
        "categories": [{ "id": "c", "name": { "en": "C" } }],
        "presets": presets,
    })
}

fn mute_preset() -> serde_json::Value {
    json!({
        "id": "mute", "revision": 2, "category": "c",
        "title": { "en": "Mute {ch}", "de": "Kanal {ch} stumm" },
        "params": [
            { "id": "ch", "label": { "en": "Channel" }, "type": "int", "min": 1, "max": 32, "default": 1 },
            { "id": "level", "label": { "en": "Level" }, "type": "float", "min": -90, "max": 10, "default": 0,
              "map": [[-90, 0], [-60, 0.0625], [-30, 0.25], [-10, 0.5], [10, 1]] }
        ],
        "cue": { "type": "osc", "device": "desk", "messages": [
            { "address": "/ch/{ch:02}/mix/on", "args": [{ "int": 0 }] },
            { "address": "/ch/{ch:02}/mix/fader", "args": [{ "float": "{level}" }] }
        ] }
    })
}

fn parse_json(value: serde_json::Value) -> Result<Library, LibraryError> {
    parse(&serde_json::to_vec(&value).unwrap())
}

fn params(list: &[(&str, ParamValue)]) -> BTreeMap<String, ParamValue> {
    list.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[test]
fn text_falls_back_to_english_then_the_first() {
    let text = Text(BTreeMap::from([("de".into(), "Hallo".into()), ("en".into(), "Hello".into())]));
    assert_eq!(text.get("de"), "Hallo");
    assert_eq!(text.get("fr"), "Hello");
    let only = Text(BTreeMap::from([("fr".into(), "Bonjour".into())]));
    assert_eq!(only.get("de"), "Bonjour");
    assert_eq!(Text::default().get("en"), "");
}

#[test]
fn maps_values_piecewise() {
    let map = [[-90.0, 0.0], [-60.0, 0.0625], [-30.0, 0.25], [-10.0, 0.5], [10.0, 1.0]];
    assert_eq!(map_value(&map, 0.0), 0.75);
    assert_eq!(map_value(&map, -10.0), 0.5);
    assert_eq!(map_value(&map, -100.0), 0.0);
    assert_eq!(map_value(&map, 20.0), 1.0);
    assert_eq!(map_value(&[], 3.0), 3.0);
}

#[test]
fn instantiates_a_preset() {
    let lib = parse_json(library(json!([mute_preset()]))).unwrap();
    let preset = lib.preset("mute").unwrap();
    let cue = instantiate(&lib, preset, &params(&[("ch", ParamValue::Number(5.0))]), "de").unwrap();
    assert_eq!(cue.title, "Kanal 5 stumm");
    let CueKind::Osc(osc) = &cue.kind else { panic!("{cue:?}") };
    assert_eq!(osc.device.as_deref(), Some("desk"));
    assert_eq!(osc.messages[0].address, "/ch/05/mix/on");
    assert_eq!(osc.messages[0].args, vec![OscArg::Int(0)]);
    assert_eq!(osc.messages[1].args, vec![OscArg::Float(0.75)]);
    let reference = cue.preset.as_ref().unwrap();
    assert_eq!((reference.library, reference.preset.as_str(), reference.revision), (lib.id, "mute", 2));
    assert_eq!(reference.params["ch"], ParamValue::Number(5.0));
    assert_eq!(reference.params["level"], ParamValue::Number(0.0));
    assert_eq!(device_templates(&cue), vec!["desk"]);
    // Each insert makes a cue of its own.
    let again = instantiate(&lib, preset, &BTreeMap::new(), "en").unwrap();
    assert_ne!(again.id, cue.id);
    assert_eq!(again.title, "Mute 1");
}

#[test]
fn refuses_parameters_out_of_range() {
    let lib = parse_json(library(json!([mute_preset()]))).unwrap();
    let preset = lib.preset("mute").unwrap();
    for bad in [ParamValue::Number(33.0), ParamValue::Number(1.5), ParamValue::Text("1".into())] {
        let err = instantiate(&lib, preset, &params(&[("ch", bad.clone())]), "en").unwrap_err();
        assert!(matches!(err, LibraryError::Param { ref param, .. } if param == "ch"), "{bad:?}: {err:?}");
    }
}

#[test]
fn checks_texts_and_choices() {
    let preset = json!({
        "id": "p", "revision": 1, "title": { "en": "{name} {mode:label}" },
        "params": [
            { "id": "name", "label": { "en": "Name" }, "type": "text", "default": "", "required": true,
              "max_len": 4, "pattern": "[a-z]+" },
            { "id": "mode", "label": { "en": "Mode" }, "type": "choice", "default": "a",
              "values": [{ "value": "a", "label": { "en": "Alpha" } }, { "value": 2, "label": { "en": "Two" } }] }
        ],
        "cue": { "type": "osc", "messages": [{ "address": "/{name}/{mode}" }] }
    });
    let lib = parse_json(library(json!([preset]))).unwrap();
    let p = lib.preset("p").unwrap();
    let make = |list: &[(&str, ParamValue)]| instantiate(&lib, p, &params(list), "en");
    let problem = |r: Result<Cue, LibraryError>| match r {
        Err(LibraryError::Param { problem, .. }) => problem,
        other => panic!("{other:?}"),
    };
    assert_eq!(problem(make(&[])), ParamProblem::Required);
    assert_eq!(
        problem(make(&[("name", ParamValue::Text("abcde".into()))])),
        ParamProblem::TooLong { max: 4 }
    );
    assert_eq!(problem(make(&[("name", ParamValue::Text("AB".into()))])), ParamProblem::NoMatch);
    assert_eq!(
        problem(make(&[("name", ParamValue::Text("ab".into())), ("mode", ParamValue::Text("x".into()))])),
        ParamProblem::NotAChoice
    );
    let cue = make(&[("name", ParamValue::Text("ab".into())), ("mode", ParamValue::Number(2.0))]).unwrap();
    assert_eq!(cue.title, "ab Two");
    let CueKind::Osc(osc) = &cue.kind else { panic!() };
    assert_eq!(osc.messages[0].address, "/ab/2");
}

#[test]
fn refuses_broken_libraries() {
    let with = |preset: serde_json::Value| parse_json(library(json!([preset])));
    assert!(matches!(parse(b"{"), Err(LibraryError::Invalid { .. })));
    assert!(matches!(
        parse_json(json!({ "format": "jqshow", "version": 1 })),
        Err(LibraryError::WrongFormat { found }) if found == "jqshow"
    ));
    let mut newer = library(json!([]));
    newer["version"] = json!(99);
    assert!(matches!(parse_json(newer), Err(LibraryError::TooNew { found: 99, .. })));
    assert!(matches!(
        parse_json(library(json!([mute_preset(), mute_preset()]))),
        Err(LibraryError::Duplicate { part: Part::Preset, .. })
    ));
    let mut p = mute_preset();
    p["category"] = json!("nope");
    assert!(matches!(with(p), Err(LibraryError::UnknownCategory { .. })));
    let mut p = mute_preset();
    p["params"][0]["default"] = json!(40);
    assert!(matches!(with(p), Err(LibraryError::Param { problem: ParamProblem::OutOfRange { .. }, .. })));
    let mut p = mute_preset();
    p["params"][1]["map"] = json!([[0, 0], [0, 1]]);
    assert!(matches!(with(p), Err(LibraryError::Param { problem: ParamProblem::Map, .. })));
    let mut p = mute_preset();
    p["cue"]["device"] = json!("other");
    assert!(matches!(with(p), Err(LibraryError::Cue { problem: CueProblem::UnknownDevice { .. }, .. })));
    let mut p = mute_preset();
    p["cue"]["messages"][0]["address"] = json!("/ch/{chan}");
    assert!(matches!(with(p), Err(LibraryError::Template { error: TemplateError::UnknownParam { .. }, .. })));
    let mut p = mute_preset();
    p["title"]["de"] = json!("{ch:label}");
    assert!(matches!(with(p), Err(LibraryError::Template { error: TemplateError::NoLabel { .. }, .. })));
    // A value only out of range at a parameter's end: MIDI channels are u8.
    let midi = json!({ "id": "m", "revision": 1, "title": { "en": "M" },
        "params": [{ "id": "ch", "label": { "en": "Ch" }, "type": "int", "min": 0, "max": 300, "default": 0 }],
        "cue": { "type": "midi", "messages": [{ "kind": "program_change", "channel": "{ch}", "program": 0 }] } });
    assert!(matches!(with(midi), Err(LibraryError::Cue { problem: CueProblem::Invalid { .. }, .. })));
    let media = json!({ "id": "s", "revision": 1, "title": { "en": "S" }, "cue": { "type": "sound", "file": "a.wav" } });
    assert!(matches!(with(media), Err(LibraryError::Cue { problem: CueProblem::Media, .. })));
    let hotkey = json!({ "id": "h", "revision": 1, "title": { "en": "H" },
        "cue": { "type": "wait", "duration": 1.0, "hotkey": { "code": "KeyA" } } });
    assert!(matches!(with(hotkey), Err(LibraryError::Cue { problem: CueProblem::Trigger, .. })));
}

#[test]
fn groups_get_ids_and_keep_their_inner_references() {
    let inner = "00000000-0000-4000-8000-0000000000aa";
    let preset = json!({ "id": "g", "revision": 1, "title": { "en": "G" }, "cue": {
        "type": "group", "cues": [
            { "id": inner, "type": "wait", "duration": 1.0 },
            { "type": "goto", "target": inner },
        ] } });
    let lib = parse_json(library(json!([preset]))).unwrap();
    let cue = instantiate(&lib, lib.preset("g").unwrap(), &BTreeMap::new(), "en").unwrap();
    let CueKind::Group(g) = &cue.kind else { panic!() };
    assert_ne!(g.cues[0].id.0.to_string(), inner, "ids are renewed");
    let CueKind::GoTo(goto) = &g.cues[1].kind else { panic!() };
    assert_eq!(goto.target, Some(g.cues[0].id));
    // A reference outside the preset is refused.
    let outside = json!({ "id": "o", "revision": 1, "title": { "en": "O" }, "cue": { "type": "goto", "target": inner } });
    assert!(matches!(
        parse_json(library(json!([outside]))),
        Err(LibraryError::Cue { problem: CueProblem::OutsideTarget, .. })
    ));
}

#[test]
fn diff_and_apply_work_per_field() {
    let lib = parse_json(library(json!([mute_preset()]))).unwrap();
    let preset = lib.preset("mute").unwrap();
    let given = params(&[("ch", ParamValue::Number(3.0))]);
    let made = instantiate(&lib, preset, &given, "en").unwrap();
    let mut cue = made.clone();
    cue.number = "7".into();
    cue.notes = "mine".into();
    cue.pre_wait = 1.0;
    let titles = titles(preset, &cue.preset.as_ref().unwrap().params);
    assert_eq!(diff(&cue, &made, &titles), vec!["notes", "pre_wait"]);
    // The title in another language of the preset is still the preset's.
    cue.title = "Kanal 3 stumm".into();
    assert_eq!(diff(&cue, &made, &titles), vec!["notes", "pre_wait"]);
    let fresh = instantiate(&lib, preset, &given, "en").unwrap();
    let reset = apply(&cue, &fresh, Some(&["pre_wait".into()])).unwrap();
    assert_eq!(
        (reset.id, reset.number.as_str(), reset.pre_wait, reset.notes.as_str()),
        (cue.id, "7", 0.0, "mine")
    );
    let all = apply(&cue, &fresh, None).unwrap();
    assert_eq!((all.id, all.notes.as_str(), all.title.as_str()), (cue.id, "", "Mute 3"));
    // Taking a type field from a cue of another type takes the whole type.
    let mut other = cue.clone();
    other.kind = CueKind::Wait(jq_show::WaitCue { duration: 2.0 });
    let back = apply(&other, &fresh, Some(&["messages".into()])).unwrap();
    assert_eq!(back.kind, fresh.kind);
    assert_eq!(param_fields(preset), vec!["messages", "title"]);
}

#[test]
fn bundled_x32_library_makes_its_cues() {
    let lib = bundled("x32.jqlibrary");
    assert_eq!(lib.presets.len(), 25);
    assert_eq!(lib.languages(), vec!["de", "en"]);
    let make = |id: &str, list: &[(&str, ParamValue)]| {
        let cue = instantiate(&lib, lib.preset(id).unwrap(), &params(list), "en").unwrap();
        let CueKind::Osc(osc) = cue.kind else { panic!() };
        assert_eq!(osc.device.as_deref(), Some("x32"));
        (cue.title, osc.messages[0].address.clone(), osc.messages[0].args.clone())
    };
    let n = ParamValue::Number;
    assert_eq!(
        make("ch-mute", &[("ch", n(5.0))]),
        ("Ch 5: Mute".into(), "/ch/05/mix/on".into(), vec![OscArg::Int(0)])
    );
    assert_eq!(make("dca-mute", &[("n", n(3.0)), ("state", n(1.0))]).1, "/dca/3/on");
    assert_eq!(
        make("bus-fader", &[("n", n(12.0)), ("level", n(-10.0))]),
        ("Bus 12 fader to -10 dB".into(), "/bus/12/mix/fader".into(), vec![OscArg::Float(0.5)])
    );
    assert_eq!(make("main-fader", &[]).2, vec![OscArg::Float(0.75)]);
    assert_eq!(make("ch-send-level", &[("ch", n(2.0)), ("bus", n(16.0))]).1, "/ch/02/mix/16/level");
    assert_eq!(
        make("scene", &[("scene", n(7.0))]),
        ("Recall scene 07".into(), "/-action/goscene".into(), vec![OscArg::Int(7)])
    );
    assert_eq!(make("talkback", &[("talk", ParamValue::Text("B".into()))]).1, "/-stat/talk/B");
    assert!(
        instantiate(&lib, lib.preset("ch-name").unwrap(), &BTreeMap::new(), "en").is_err(),
        "name is required"
    );
}

#[test]
fn bundled_magicq_library_makes_its_cues() {
    let lib = bundled("magicq.jqlibrary");
    assert_eq!(lib.presets.len(), 14);
    let n = ParamValue::Number;
    let osc = |id: &str, list: &[(&str, ParamValue)]| {
        let cue = instantiate(&lib, lib.preset(id).unwrap(), &params(list), "de").unwrap();
        let CueKind::Osc(osc) = cue.kind else { panic!() };
        (cue.title, osc.messages[0].address.clone(), osc.messages[0].args.clone())
    };
    assert_eq!(osc("pb-go", &[("pb", n(4.0))]), ("PB 4 GO".into(), "/pb/4/go".into(), vec![]));
    assert_eq!(osc("pb-level", &[("pct", n(50.0))]).2, vec![OscArg::Int(50)]);
    assert_eq!(osc("pb-jump", &[("cue", ParamValue::Text("5.5".into()))]).1, "/pb/1/5.5");
    assert!(
        instantiate(
            &lib,
            lib.preset("pb-jump").unwrap(),
            &params(&[("cue", ParamValue::Text("x".into()))]),
            "en"
        )
        .is_err()
    );
    assert_eq!(
        osc("exec", &[("page", n(2.0)), ("item", n(15.0)), ("pct", n(40.0))]),
        ("Execute 2/15 auf 40 %".into(), "/exec/2/15".into(), vec![OscArg::Float(0.4)])
    );
    let cue = instantiate(
        &lib,
        lib.preset("msc-go").unwrap(),
        &params(&[("cue", ParamValue::Text("12.5".into()))]),
        "en",
    )
    .unwrap();
    assert_eq!(cue.title, "MSC GO 12.5");
    let CueKind::Midi(midi) = cue.kind else { panic!() };
    assert_eq!(
        midi.messages,
        vec![MidiMessage::Msc {
            device: 127,
            command: MscCommand::Go,
            cue: "12.5".into(),
            list: String::new()
        }]
    );
    let stop = instantiate(&lib, lib.preset("msc-stop").unwrap(), &BTreeMap::new(), "en").unwrap();
    assert_eq!(stop.title, "MSC STOP");
}
