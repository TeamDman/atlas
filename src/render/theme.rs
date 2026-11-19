//! Colour themes for drawn diagrams. A theme changes presentation only: it
//! never adds, hides or reorders an observation.
//!
//! Pele styles its SVG through CSS custom properties, which browsers resolve
//! but many SVG consumers (librsvg, image viewers, some converters) do not.
//! [`resolve`] therefore writes the theme's concrete colours into the markup
//! and gives the drawing an opaque background, so the file looks the same
//! everywhere. Only presentation attributes inside tags are rewritten; text
//! content, which carries source-derived labels, is copied unchanged.
use clap::ValueEnum;
use std::{collections::BTreeMap, fmt::Write as _};

/// Colour theme for SVG and HTML diagrams. ASCII output has no colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    /// Light text on a dark background.
    #[default]
    Dark,
    /// Dark text on a white background.
    Light,
}

/// Fill, border and text colour for one Mermaid node class.
pub(crate) struct Class {
    pub fill: &'static str,
    pub stroke: &'static str,
    pub color: &'static str,
}
impl Class {
    /// A Mermaid `classDef` line; `extra` carries size and weight.
    pub(crate) fn def(&self, name: &str, extra: &str) -> String {
        format!(
            "classDef {name} fill:{},stroke:{},color:{}{extra}\n",
            self.fill, self.stroke, self.color
        )
    }
}

/// Every colour a renderer may use. Renderers take colours from here, never
/// from literals, so both themes stay complete.
pub(crate) struct Palette {
    /// Pele's own variables, by name without the `--pele-` prefix.
    pele: [(&'static str, &'static str); 8],
    pub background: &'static str,
    pub text: &'static str,
    pub border: &'static str,
    /// Headings and notes Atlas draws around Pele panels.
    pub heading: &'static str,
    pub link: &'static str,
    pub object: Class,
    pub operation: Class,
    pub detail: Class,
    pub cell: Class,
    pub before: Class,
    pub after: Class,
    pub header: Class,
    pub empty: Class,
    pub stored: Class,
}

const DARK: Palette = Palette {
    pele: [
        ("bg", "#0d1117"),
        ("fg", "#e6edf3"),
        ("muted", "#8b949e"),
        ("line", "#c9d1d9"),
        ("surface", "#161b22"),
        ("surface-alt", "#21262d"),
        ("border", "#3d444d"),
        ("accent", "#58a6ff"),
    ],
    background: "#0d1117",
    text: "#e6edf3",
    border: "#3d444d",
    heading: "#c9dcec",
    link: "#6f9cc0",
    object: Class {
        fill: "#12263a",
        stroke: "#5b8db5",
        color: "#e3f0fb",
    },
    operation: Class {
        fill: "#0d1117",
        stroke: "#4a6070",
        color: "#d4e2ee",
    },
    detail: Class {
        fill: "#161b22",
        stroke: "#30404d",
        color: "#a9bccb",
    },
    cell: Class {
        fill: "#161b22",
        stroke: "#3d5a73",
        color: "#c9dcec",
    },
    before: Class {
        fill: "#2a1612",
        stroke: "#b67b68",
        color: "#f0c4b6",
    },
    after: Class {
        fill: "#12261a",
        stroke: "#6d9c7e",
        color: "#bfe3cb",
    },
    header: Class {
        fill: "#12263a",
        stroke: "#3d5a73",
        color: "#c9dcec",
    },
    empty: Class {
        fill: "#0d1117",
        stroke: "#21262d",
        color: "#3d4a55",
    },
    stored: Class {
        fill: "#1a3044",
        stroke: "#5b8db5",
        color: "#e3f0fb",
    },
};

const LIGHT: Palette = Palette {
    pele: [
        ("bg", "#fff"),
        ("fg", "#1f1f1f"),
        ("muted", "#6e6e6e"),
        ("line", "#1f1f1f"),
        ("surface", "#f3f3f3"),
        ("surface-alt", "#e6e6e6"),
        ("border", "#8a8a8a"),
        ("accent", "#5b7bd5"),
    ],
    background: "#ffffff",
    text: "#222222",
    border: "#dddddd",
    heading: "#24475f",
    link: "#567b98",
    object: Class {
        fill: "#e7f0f8",
        stroke: "#3d6b8c",
        color: "#183e59",
    },
    operation: Class {
        fill: "#ffffff",
        stroke: "#8398a9",
        color: "#253e51",
    },
    detail: Class {
        fill: "#f5f7f9",
        stroke: "#b4c3cf",
        color: "#3d586c",
    },
    cell: Class {
        fill: "#f5f7f9",
        stroke: "#9ab0c2",
        color: "#24475f",
    },
    before: Class {
        fill: "#fff2ee",
        stroke: "#b67b68",
        color: "#59382e",
    },
    after: Class {
        fill: "#eef7f1",
        stroke: "#6d9c7e",
        color: "#284b35",
    },
    header: Class {
        fill: "#e7f0f8",
        stroke: "#9ab0c2",
        color: "#24475f",
    },
    empty: Class {
        fill: "#ffffff",
        stroke: "#dce5ec",
        color: "#b7c5d0",
    },
    stored: Class {
        fill: "#e4edf5",
        stroke: "#698ca7",
        color: "#183e59",
    },
};

impl Theme {
    pub(crate) fn palette(self) -> &'static Palette {
        match self {
            Self::Dark => &DARK,
            Self::Light => &LIGHT,
        }
    }
}

/// Attributes whose values may name a CSS variable in Pele output.
const PRESENTATION: [&str; 6] = [
    "fill",
    "stroke",
    "style",
    "color",
    "font-weight",
    "font-family",
];

/// Replace Pele's CSS variables with the palette's concrete values and add an
/// opaque background as the root's first child.
pub(crate) fn resolve(svg: &str, palette: &Palette) -> String {
    let mut out = String::with_capacity(svg.len() + 96);
    let mut locals = BTreeMap::new();
    let mut rest = svg;
    let mut root = true;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let end = tag_end(&rest[start..]).map_or(rest.len(), |e| start + e);
        out.push_str(&resolve_tag(&rest[start..end], palette, &mut locals));
        if root && rest[start..].starts_with("<svg") {
            write!(
                out,
                "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
                palette.background
            )
            .expect("formatting into a String cannot fail");
            root = false;
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Byte length of the tag at the start of `s`, honouring quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

fn resolve_tag(tag: &str, palette: &Palette, locals: &mut BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let name_start = rest[..eq]
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |i| i + 1);
        let value_start = eq + 2;
        let Some(len) = rest[value_start..].find('"') else {
            break;
        };
        out.push_str(&rest[..value_start]);
        let value = &rest[value_start..value_start + len];
        if PRESENTATION.contains(&&rest[name_start..eq]) {
            let mut value = resolve_value(value, palette, locals);
            if &rest[name_start..eq] == "style" {
                // A style may declare a variable and use it in the same value.
                remember(&value, locals);
                value = resolve_value(&value, palette, locals);
            }
            out.push_str(&value);
        } else {
            out.push_str(value);
        }
        rest = &rest[value_start + len..];
    }
    out.push_str(rest);
    out
}

/// Record `--_name:value` declarations so later `var(--_name)` uses resolve.
fn remember(style: &str, locals: &mut BTreeMap<String, String>) {
    for declaration in style.split(';') {
        if let Some((name, value)) = declaration.split_once(':')
            && let Some(name) = name.trim().strip_prefix("--")
            && !value.contains("var(")
        {
            locals.insert(name.to_owned(), value.trim().to_owned());
        }
    }
}

/// Resolve `var(--pele-name,default)` from the palette (or its default) and
/// `var(--local)` from earlier declarations. Unknown variables stay as written.
fn resolve_value(value: &str, palette: &Palette, locals: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("var(--") {
        out.push_str(&rest[..start]);
        let inner = &rest[start + 6..];
        let Some(close) = inner.find(')') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let (name, default) = inner[..close]
            .split_once(',')
            .map_or((&inner[..close], None), |(n, d)| (n, Some(d)));
        let resolved = name
            .strip_prefix("pele-")
            .and_then(|n| palette.pele.iter().find(|(k, _)| *k == n))
            .map(|(_, v)| (*v).to_owned())
            .or_else(|| default.map(str::to_owned))
            .or_else(|| locals.get(name).cloned());
        match resolved {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[start..=start + 6 + close]),
        }
        rest = &inner[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PELE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"--_bg:var(--pele-bg,#fff);--_fg:var(--pele-fg,#1f1f1f);--_r:var(--pele-radius,4px);color:var(--_fg)\" fill=\"var(--_fg)\"><rect fill=\"var(--_bg)\" style=\"rx:var(--_r)\"/><text fill=\"var(--_fg)\">fill=\"var(--_bg)\" a &gt; b</text></svg>";

    #[test]
    fn dark_writes_concrete_colours_and_a_background() {
        let svg = resolve(PELE, Theme::Dark.palette());
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"--_bg:#0d1117;--_fg:#e6edf3;--_r:4px;color:#e6edf3\" fill=\"#e6edf3\">"), "{svg}");
        assert_eq!(
            svg.matches("var(--").count(),
            1,
            "only the label keeps it: {svg}"
        );
        assert!(svg.contains("<rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/>"));
        assert!(svg.contains("<rect fill=\"#0d1117\" style=\"rx:4px\"/>"));
        assert!(svg.contains("<text fill=\"#e6edf3\">"));
    }

    #[test]
    fn light_keeps_pele_defaults() {
        let svg = resolve(PELE, Theme::Light.palette());
        assert!(svg.contains("<rect fill=\"#fff\" style=\"rx:4px\"/>"));
        assert!(svg.contains("<text fill=\"#1f1f1f\">"));
    }

    #[test]
    fn source_text_is_never_rewritten() {
        let svg = resolve(PELE, Theme::Dark.palette());
        assert!(
            svg.contains(">fill=\"var(--_bg)\" a &gt; b</text>"),
            "{svg}"
        );
    }
}
