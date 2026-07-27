//! Publishing this origin on a private mesh (§9.1, ADR-0013, ADR-0017).
//!
//! An Ashlar program is an origin: it binds a port and answers HTTP on it,
//! and everything about *reaching* that port from somewhere else is a
//! deployment fact — a proxy terminating TLS, a port published to a mesh.
//! This module is the second of those. It adds no builtin and no language
//! surface: the mesh is a capability reached across the one boundary
//! (§9.10), and the two space names that carry it derive to the co-process
//! the machine already runs (`foreign::derived_worker`).
//!
//! So `ashlar run --mesh` is exactly `--port`'s sibling. `--port` says where
//! this origin listens; `--mesh` says who else can reach it. Neither is
//! written in source (B5), and both are reported on the one line the runtime
//! prints when it comes up.

use crate::eval::{to_text, V};
use crate::foreign::{Boundary, MESH_SPACE, SITES_SPACE};
use std::collections::BTreeMap;
use std::path::Path;

/// What the mesh answered when this origin was published to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    /// This node's id on the mesh — what a peer addresses.
    pub node: String,
    /// The mesh it was published on.
    pub network: String,
    /// The name peers see the site under.
    pub label: String,
}

impl Published {
    /// The one line `run --mesh` prints. Says the mesh by name, because a
    /// site published to the wrong roster looks exactly like one published
    /// to the right roster until someone cannot find it.
    pub fn line(&self) -> String {
        format!(
            "published `{}` on mesh `{}` as node {}",
            self.label, self.network, self.node
        )
    }
}

/// One line per fact `ashlar mesh` prints, and the problems that stopped it.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub facts: Vec<(String, String)>,
    pub problems: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// One conversation with the mesh, for as long as the caller needs it.
///
/// A run publishes on the way up and withdraws on the way down, and those two
/// must reach the SAME co-process: a second one would be a second conversation
/// with no memory of the first, so a worker that died in between would answer
/// the withdrawal happily and leave the site published. Holding the boundary
/// open makes that a loud failure instead. Dropping the link stops whatever it
/// spawned.
#[derive(Default)]
pub struct Link {
    boundary: Boundary,
}

impl Link {
    pub fn new() -> Link {
        Link {
            boundary: Boundary::new(),
        }
    }

    /// Publish the port this origin is serving to the mesh. `network` empty
    /// means the mesh the machine's daemon calls its own default — the shared
    /// area every unconfigured Ashlar site lands on.
    pub fn publish(
        &mut self,
        root: &Path,
        port: u16,
        network: &str,
        label: &str,
    ) -> Result<Published, String> {
        let answer = self.boundary.call(
            root,
            SITES_SPACE,
            "expose",
            vec![
                V::Number(port as f64),
                V::Text(label.to_string()),
                V::Text(network.to_string()),
            ],
        )?;
        read_published(&answer, label)
    }

    /// Take the site back off the mesh. Called on the way out of `run`; a
    /// failure here is worth saying and not worth failing over, since the
    /// process is leaving anyway and the daemon drops what it cannot reach.
    pub fn withdraw(&mut self, root: &Path, port: u16) -> Result<(), String> {
        self.boundary
            .call(root, SITES_SPACE, "unexpose", vec![V::Number(port as f64)])
            .map(|_| ())
    }

    /// What this machine's mesh looks like from here: identity and roster from
    /// `mesh`, published sites from `mesh.sites`. Each space is asked
    /// separately and reported separately, because a machine with the mesh
    /// daemon but no site proxy is a real deployment — the roster works,
    /// publishing does not, and one line each says so.
    pub fn report(&mut self, root: &Path) -> Report {
        let mut out = Report::default();
        let b = &mut self.boundary;

        match b.call(root, MESH_SPACE, "here", vec![]) {
            Ok(v) => {
                let m = fields(&v);
                out.facts.push(("mesh".to_string(), text_of(&m, "network")));
                out.facts.push(("node".to_string(), text_of(&m, "id")));
                out.facts.push(("label".to_string(), text_of(&m, "label")));
                out.facts.push(("peers".to_string(), number_of(&m, "peers")));
            }
            Err(e) => out.problems.push(format!("{}: {}", MESH_SPACE, e)),
        }

        match b.call(root, SITES_SPACE, "published", vec![]) {
            Ok(V::List(sites)) => {
                if sites.is_empty() {
                    out.facts
                        .push(("published".to_string(), "nothing".to_string()));
                }
                for s in &sites {
                    let m = fields(s);
                    out.facts
                        .push(("published".to_string(), text_of(&m, "label")));
                }
            }
            Ok(other) => out.problems.push(format!(
                "{}: `published` answered {}, not a list of sites.",
                SITES_SPACE,
                shape_word(&other)
            )),
            Err(e) => out.problems.push(format!("{}: {}", SITES_SPACE, e)),
        }

        out
    }
}

// -- decoding the answers ---------------------------------------------------
//
// The boundary shape-checks what a DECLARED foreign name returns (§9.10).
// These calls are the runtime's own, so nothing declared them and nothing
// checked them: the decoding below is that check, and it names the field it
// wanted rather than yielding a blank.

fn fields(v: &V) -> BTreeMap<String, V> {
    match v {
        V::Map(m) => m.clone(),
        _ => BTreeMap::new(),
    }
}

fn text_of(m: &BTreeMap<String, V>, key: &str) -> String {
    match m.get(key) {
        Some(V::Text(t)) => t.clone(),
        Some(V::Number(n)) => to_text(&V::Number(*n)),
        _ => "unknown".to_string(),
    }
}

fn number_of(m: &BTreeMap<String, V>, key: &str) -> String {
    match m.get(key) {
        Some(V::Number(n)) => to_text(&V::Number(*n)),
        _ => "unknown".to_string(),
    }
}

fn shape_word(v: &V) -> &'static str {
    match v {
        V::Text(_) => "a text",
        V::Number(_) => "a number",
        V::Bool(_) => "a bool",
        V::List(_) => "a list",
        V::Map(_) => "a map",
        _ => "nothing",
    }
}

fn read_published(answer: &V, label: &str) -> Result<Published, String> {
    let V::Map(m) = answer else {
        return Err(format!(
            "the mesh answered {} to `expose`; it owes a map with `node` and `network`.",
            shape_word(answer)
        ));
    };
    let Some(V::Text(node)) = m.get("node") else {
        return Err("the mesh answered `expose` without a `node` text.".to_string());
    };
    let Some(V::Text(network)) = m.get("network") else {
        return Err("the mesh answered `expose` without a `network` text.".to_string());
    };
    Ok(Published {
        node: node.clone(),
        network: network.clone(),
        label: match m.get("label") {
            Some(V::Text(l)) if !l.is_empty() => l.clone(),
            _ => label.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, V)]) -> V {
        V::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn a_published_site_names_the_mesh_it_landed_on() {
        // covers: B5
        let p = read_published(
            &map(&[
                ("node", V::Text("n1".into())),
                ("network", V::Text("enclave".into())),
                ("label", V::Text("enclave.app".into())),
            ]),
            "fallback",
        )
        .unwrap();
        assert_eq!(p.network, "enclave");
        assert_eq!(p.label, "enclave.app");
        assert!(p.line().contains("mesh `enclave`"), "{}", p.line());
    }

    #[test]
    fn a_label_the_mesh_did_not_choose_falls_back_to_ours() {
        let p = read_published(
            &map(&[
                ("node", V::Text("n1".into())),
                ("network", V::Text("ashlar".into())),
            ]),
            "site.app",
        )
        .unwrap();
        assert_eq!(p.label, "site.app");
    }

    #[test]
    fn an_answer_missing_what_it_owes_is_a_named_failure() {
        // Nothing declared `expose`, so no shape check ran on the way back.
        // A blank line saying the site is published would be the quiet-wrong
        // this language refuses; the message names the missing field.
        let e = read_published(&map(&[("node", V::Text("n1".into()))]), "x").unwrap_err();
        assert!(e.contains("`network`"), "{}", e);
        let e = read_published(&V::Text("yes".into()), "x").unwrap_err();
        assert!(e.contains("a text") && e.contains("map"), "{}", e);
    }
}
