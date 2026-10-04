//! Reading an OpenStreetMap extract through the `osmium` command-line tool,
//! for `places_build` (ADR-0027): the objects some tag filters keep, as one
//! GeoJSON feature at a time.
//!
//! Its working files — the filtered extract and the index of node
//! locations — go in a directory beside the extract rather than the system
//! temp directory, which may be held in memory, and the index is on disk
//! rather than in memory: for Germany either runs to gigabytes.

use std::io::{BufRead, BufReader, Lines};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use anyhow::{bail, Context};
use serde_json::Value;
use tempfile::TempDir;

/// The features of an extract, read as `osmium export` writes them.
pub struct Export {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
    // Removed once the export is done with it.
    _work: TempDir,
}

/// The objects of `pbf` that `filters` (`osmium tags-filter` expressions)
/// keep, as features of `geometry_types` (`point,polygon`), each with its
/// OSM type and id among its properties.
pub fn export(pbf: &Path, filters: &[&str], geometry_types: &str) -> anyhow::Result<Export> {
    let beside = pbf.parent().filter(|dir| !dir.as_os_str().is_empty());
    let work = tempfile::Builder::new()
        .prefix(".osmium-")
        .tempdir_in(beside.unwrap_or(Path::new(".")))
        .context("making a working directory beside the extract")?;
    let filtered = work.path().join("filtered.osm.pbf");
    let status = Command::new("osmium")
        .arg("tags-filter")
        .arg(pbf)
        .args(filters)
        .arg("--overwrite")
        .arg("-o")
        .arg(&filtered)
        .status()
        .context("running osmium — is it installed?")?;
    if !status.success() {
        bail!("osmium tags-filter failed on {}", pbf.display());
    }

    let index = format!(
        "sparse_file_array,{}",
        work.path().join("nodes.idx").display()
    );
    let mut child = Command::new("osmium")
        .arg("export")
        .arg(&filtered)
        .args(["-f", "geojsonseq", "-a", "type,id", "-i", &index])
        .arg(format!("--geometry-types={geometry_types}"))
        .args(["-o", "-"])
        .stdout(Stdio::piped())
        .spawn()
        .context("running osmium export")?;
    let lines = BufReader::new(child.stdout.take().context("osmium's output")?).lines();
    Ok(Export {
        child,
        lines,
        _work: work,
    })
}

impl Export {
    /// Waits for osmium to end, failing if it did not end well.
    pub fn finish(mut self) -> anyhow::Result<()> {
        if !self.child.wait()?.success() {
            bail!("osmium export failed");
        }
        Ok(())
    }
}

impl Iterator for Export {
    type Item = anyhow::Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = match self.lines.next()? {
            Ok(line) => line,
            Err(e) => return Some(Err(e.into())),
        };
        Some(
            serde_json::from_str(line.trim_start_matches('\x1e'))
                .with_context(|| format!("osmium wrote something not GeoJSON: {line:.80}")),
        )
    }
}
