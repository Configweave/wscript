//! `xml` — parse/serialize XML via the shared `Value` type (PRD §7).
//!
//! **Documented mapping convention (decided, PRD §7):** XML does not fit a
//! JSON-shaped value model; this module uses a lossy-but-predictable
//! convention:
//!
//! - the document maps to `Map { root_name: <element> }`;
//! - an element with attributes/children maps to a `Map` with attributes
//!   under the `"@attrs"` key (a `Map[string, Value]`) and text content
//!   under `"#text"`;
//! - repeated child elements collapse into a `List` under their shared
//!   name;
//! - an element with only text maps to a plain `String`, an empty element
//!   to `Null`;
//! - comments and processing instructions are dropped.
//!
//! Serialization inverts the convention; the top level must be a map with
//! exactly one (root) key.

use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;
use wisp_core::Module;

use crate::value::DynValue;

struct Element {
    attrs: Vec<(String, String)>,
    children: Vec<(String, Element)>,
    text: String,
}

fn parse_children(reader: &mut Reader<&[u8]>, parent: &mut Element) -> Result<(), String> {
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(start) => {
                let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
                let mut child = element_from_start(&start)?;
                parse_children(reader, &mut child)?;
                parent.children.push((name, child));
            }
            Event::Empty(start) => {
                let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
                let child = element_from_start(&start)?;
                parent.children.push((name, child));
            }
            Event::Text(t) => {
                let text = t.unescape().map_err(|e| e.to_string())?;
                parent.text.push_str(&text);
            }
            Event::CData(c) => {
                parent.text.push_str(&String::from_utf8_lossy(c.as_ref()));
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Ok(()),
            // Comments, PIs, decls and doctypes are dropped (documented).
            _ => {}
        }
    }
}

fn element_from_start(start: &quick_xml::events::BytesStart) -> Result<Element, String> {
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        attrs.push((
            String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
            attr.unescape_value()
                .map_err(|e| e.to_string())?
                .into_owned(),
        ));
    }
    Ok(Element {
        attrs,
        children: Vec::new(),
        text: String::new(),
    })
}

fn element_to_value(el: &Element) -> DynValue {
    let text = el.text.trim();
    if el.attrs.is_empty() && el.children.is_empty() {
        return if text.is_empty() {
            DynValue::Null
        } else {
            DynValue::String(text.to_string())
        };
    }
    let mut map: HashMap<String, DynValue> = HashMap::new();
    if !el.attrs.is_empty() {
        map.insert(
            "@attrs".to_string(),
            DynValue::Map(
                el.attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), DynValue::String(v.clone())))
                    .collect(),
            ),
        );
    }
    if !text.is_empty() {
        map.insert("#text".to_string(), DynValue::String(text.to_string()));
    }
    // Group children by name; repeated names collapse to lists.
    let mut grouped: Vec<(String, Vec<DynValue>)> = Vec::new();
    for (name, child) in &el.children {
        let value = element_to_value(child);
        match grouped.iter_mut().find(|(n, _)| n == name) {
            Some((_, items)) => items.push(value),
            None => grouped.push((name.clone(), vec![value])),
        }
    }
    for (name, mut items) in grouped {
        if items.len() == 1 {
            map.insert(name, items.pop().unwrap());
        } else {
            map.insert(name, DynValue::List(items));
        }
    }
    DynValue::Map(map)
}

fn escape_xml(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
}

fn scalar_text(v: &DynValue) -> Option<String> {
    match v {
        DynValue::Null => Some(String::new()),
        DynValue::Bool(b) => Some(b.to_string()),
        DynValue::Int(n) => Some(n.to_string()),
        DynValue::Float(f) => Some(f.to_string()),
        DynValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn write_element(
    name: &str,
    v: &DynValue,
    out: &mut String,
    indent: usize,
    pretty: bool,
) -> Result<(), String> {
    let pad = if pretty {
        "  ".repeat(indent)
    } else {
        String::new()
    };
    let nl = if pretty { "\n" } else { "" };
    match v {
        DynValue::List(items) => {
            for item in items {
                write_element(name, item, out, indent, pretty)?;
            }
            Ok(())
        }
        DynValue::Map(entries) => {
            out.push_str(&pad);
            out.push('<');
            out.push_str(name);
            if let Some(DynValue::Map(attrs)) = entries.get("@attrs") {
                let mut keys: Vec<&String> = attrs.keys().collect();
                keys.sort();
                for k in keys {
                    let text = scalar_text(&attrs[k])
                        .ok_or_else(|| format!("attribute `{k}` must be a scalar"))?;
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    escape_xml(&text, out);
                    out.push('"');
                }
            }
            let mut keys: Vec<&String> = entries
                .keys()
                .filter(|k| *k != "@attrs" && *k != "#text")
                .collect();
            keys.sort();
            let text = entries.get("#text").and_then(scalar_text);
            if keys.is_empty() && text.is_none() {
                out.push_str("/>");
                out.push_str(nl);
                return Ok(());
            }
            out.push('>');
            if let Some(t) = &text {
                escape_xml(t, out);
            }
            if !keys.is_empty() {
                out.push_str(nl);
                for k in keys {
                    write_element(k, &entries[k], out, indent + 1, pretty)?;
                }
                out.push_str(&pad);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
            out.push_str(nl);
            Ok(())
        }
        scalar => {
            let text = scalar_text(scalar).expect("scalars always render");
            out.push_str(&pad);
            if text.is_empty() {
                out.push('<');
                out.push_str(name);
                out.push_str("/>");
            } else {
                out.push('<');
                out.push_str(name);
                out.push('>');
                escape_xml(&text, out);
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            out.push_str(nl);
            Ok(())
        }
    }
}

fn value_to_xml(v: &DynValue, pretty: bool) -> Result<String, String> {
    let DynValue::Map(entries) = v else {
        return Err("XML documents serialize from a map with exactly one root key".into());
    };
    if entries.len() != 1 {
        return Err(format!(
            "XML documents need exactly one root element, found {} keys",
            entries.len()
        ));
    }
    let (name, root) = entries.iter().next().unwrap();
    let mut out = String::new();
    write_element(name, root, &mut out, 0, pretty)?;
    if !pretty && out.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

pub fn xml() -> Module {
    let mut m = Module::new("xml");
    m.doc(
        "XML parsing and serialization (shared Value type; attributes under \
         \"@attrs\", text under \"#text\", repeated children become lists)",
    );
    m.fn_("parse", |s: &str| -> Result<DynValue, String> {
        let mut reader = Reader::from_str(s);
        reader.config_mut().trim_text(true);
        let mut doc = Element {
            attrs: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        };
        parse_children(&mut reader, &mut doc)?;
        let Some((name, root)) = doc.children.first() else {
            return Err("document has no root element".into());
        };
        let mut map = HashMap::new();
        map.insert(name.clone(), element_to_value(root));
        Ok(DynValue::Map(map))
    });
    m.fn_("to_string", |v: DynValue| -> Result<String, String> {
        value_to_xml(&v, false)
    });
    m.fn_(
        "to_string_pretty",
        |v: DynValue| -> Result<String, String> { value_to_xml(&v, true) },
    );
    m
}
