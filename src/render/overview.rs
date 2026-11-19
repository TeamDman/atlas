//! Present source-declared contracts and instrument calls as separate panels.
use crate::{Result, overview::Overview, representations};
use std::collections::BTreeSet;
use std::fmt::Write as _;
/// Append a separate native Pele instrument panel; never join call and contract edges.
pub fn pele(overview: &Overview, theme: crate::render::Theme) -> Result<crate::render::Artifact> {
    let palette = theme.palette();
    let mut artifact = crate::render::representations::vertical(
        &overview.views,
        representations::Panel::Contracts,
        palette,
    )?;
    if overview.instruments.is_empty() {
        return Ok(artifact);
    }
    let mut source = format!("block-beta\ncolumns {}\n", overview.instruments.len());
    use std::fmt::Write;
    for (i, instrument) in overview.instruments.iter().enumerate() {
        let targets: BTreeSet<_> = instrument.calls.iter().map(|c| c.to.as_str()).collect();
        let calls = targets
            .into_iter()
            .map(crate::render::pele::label)
            .collect::<Vec<_>>()
            .join("<br/>");
        writeln!(
            source,
            "instrument{i}[\"{}<br/>{}<br/>direct calls:<br/>{}\"]",
            crate::render::pele::label(&instrument.name),
            crate::render::pele::label(&instrument.path),
            if calls.is_empty() {
                "none resolved<br/>see JSON unknowns"
            } else {
                &calls
            }
        )
        .expect("formatting into a String cannot fail");
    }
    source.push_str(&palette.cell.def("default", ",font-size:17px"));
    let bottom = crate::render::pele::render_source_named(&source, "atlas-instruments", palette)?;
    let main = String::from_utf8(artifact.primary.bytes)?;
    let (w, h) = crate::render::pele::dimensions(&main)?;
    let (bw, bh) = crate::render::pele::dimensions(&bottom)?;
    let width = w.max(bw + 80.);
    let height = h + bh + 120.;
    artifact.primary.bytes = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"{background}\"/>{main}<text x=\"40\" y=\"{}\" font-family=\"system-ui\" font-size=\"24\" fill=\"{heading}\">Verification instruments: observed direct calls, separate from contracts</text><g transform=\"translate(40,{})\">{bottom}</g></svg>",
        h + 30.,
        h + 60.,
        background = palette.background,
        heading = palette.heading,
    ).into_bytes();
    for d in &mut artifact.companions {
        if d.suffix == "mmd" {
            d.bytes
                .extend_from_slice(format!("\n%% Verification instruments\n{source}").as_bytes());
        }
    }
    Ok(artifact)
}

/// Text of the same contracts and instrument calls as the Pele overview.
pub fn ascii(overview: &Overview) -> String {
    let mut text = representations::ascii(&overview.views, representations::Panel::Contracts);
    text.push_str("\nVERIFICATION INSTRUMENTS (observed direct calls; not pipeline edges)\n");
    for i in &overview.instruments {
        write!(
            text,
            "{} [{}]\n  {}\n",
            i.name,
            i.path,
            i.calls
                .iter()
                .map(|c| format!("{} → {} ({}:{})", c.from, c.to, c.file, c.line))
                .collect::<Vec<_>>()
                .join("\n  ")
        )
        .expect("formatting into a String cannot fail");
    }
    text
}
