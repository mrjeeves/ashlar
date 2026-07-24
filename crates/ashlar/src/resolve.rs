//! Resolver: turns parsed files into a `resolved::Program` per reference
//! §2 (names, spaces, `use`) and §3 (parts and layers).
//!
//! Stages, in order:
//! 1. Group files into spaces; reject `space std` (E017).
//! 2. Validate `use` targets (E008), compute each space's transitive closure.
//! 3. Detect use-graph cycles (E015) and break them deterministically.
//! 4. Compute composition order: Kahn's algorithm over the condensation,
//!    base spaces first, `std` first of all, lexicographic tie-breaks (C2).
//! 5. Register parts: bare declarations introduce (E014 on duplicates),
//!    dotted declarations layer (E001/E017), layers ordered by space order
//!    with W001 + a `use` insertion fix for genuinely unordered pairs (C3).
//! 6. Register foreigns (E013 on duplicates).
//! 7. Case/separator collisions (E003) across the four name scopes.
//! 8. Resolve every body expression: longest-prefix name resolution
//!    (E001/E002), no shadowing (E002), assignment targets (E025),
//!    function-literal positions (E024).
//!
//! Determinism: all internal maps are BTreeMaps/BTreeSets; nothing iterates
//! a HashMap. Diagnostics come out in stage order; the driver sorts them.

use crate::ast::{self, Expr, FnBody, ListItem, MapItem, SExpr, Stmt};
use crate::diag::{
    Diag, Edit, Level, E001_UNKNOWN_NAME, E002_AMBIGUOUS_NAME, E003_CASE_COLLISION,
    E008_USE_NOT_SPACE, E013_DUP_PROP, E014_DUP_LAYER, E015_USE_CYCLE, E017_STD_LAYER,
    E024_FNLIT_POSITION, E025_BAD_ASSIGN, W001_UNORDERED_LAYERS,
};
use crate::resolved::{
    FileEntry, ForeignInfo, Layer, PartInfo, Program, SpaceInfo, STD_FNS, STD_PARTS,
};
use crate::tokens::{Pos, Span};
use std::collections::{BTreeMap, BTreeSet};

pub fn resolve(files: Vec<FileEntry>) -> (Program, Vec<Diag>) {
    let mut r = Resolver {
        program: Program {
            files,
            ..Program::default()
        },
        diags: Vec::new(),
        use_sites: BTreeMap::new(),
    };
    r.group_spaces();
    r.validate_uses();
    r.closures_and_order();
    r.register_parts();
    r.register_foreigns();
    r.case_collisions();
    r.resolve_bodies();
    (r.program, r.diags)
}

struct Resolver {
    program: Program,
    diags: Vec<Diag>,
    /// (space, use-target) -> (file index, span of the target name).
    /// First occurrence wins; used for E015/E008 locations.
    use_sites: BTreeMap<(String, String), (usize, Span)>,
}

impl Resolver {
    fn file_path(&self, idx: usize) -> String {
        self.program.files[idx].path.clone()
    }

    // -- stage 1: spaces ----------------------------------------------------

    fn group_spaces(&mut self) {
        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            let span = self.program.files[idx].ast.space_span;
            let path = self.file_path(idx);
            if space == "std" || space.starts_with("std.") {
                self.diags.push(
                    Diag::new(
                        E017_STD_LAYER,
                        Level::Error,
                        &path,
                        span,
                        "`std` is provided by the runtime and cannot be declared.".to_string(),
                    )
                    .with_fix("Choose a space name other than `std`.".to_string(), vec![]),
                );
                continue;
            }
            let info = self
                .program
                .spaces
                .entry(space)
                .or_insert_with(SpaceInfo::default);
            info.files.push(path);
        }
        for info in self.program.spaces.values_mut() {
            info.files.sort();
        }
    }

    // -- stage 2: use targets ------------------------------------------------

    fn validate_uses(&mut self) {
        // Full part names, needed for the E008 "used a part" fix. Built from
        // bare declarations only (dotted declarations never introduce).
        let mut part_homes: BTreeMap<String, String> = BTreeMap::new();
        for f in &self.program.files {
            let space = ast::name_to_string(&f.ast.space);
            if !self.program.spaces.contains_key(&space) {
                continue;
            }
            for p in &f.ast.parts {
                if p.name.len() == 1 {
                    part_homes.insert(format!("{}.{}", space, p.name[0]), space.clone());
                }
            }
        }

        let space_names: Vec<String> = self.program.spaces.keys().cloned().collect();
        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            if !self.program.spaces.contains_key(&space) {
                continue;
            }
            let path = self.file_path(idx);
            let uses: Vec<(String, Span)> = self.program.files[idx]
                .ast
                .uses
                .iter()
                .map(|(n, s)| (ast::name_to_string(n), *s))
                .collect();
            for (target, span) in uses {
                self.use_sites
                    .entry((space.clone(), target.clone()))
                    .or_insert((idx, span));
                if target == "std" {
                    continue; // explicit `use std` is a legal no-op
                }
                if target == space {
                    // Self-use: record the edge; the cycle stage reports it.
                    self.program
                        .spaces
                        .get_mut(&space)
                        .unwrap()
                        .uses
                        .insert(target.clone());
                    continue;
                }
                if self.program.spaces.contains_key(&target) {
                    self.program
                        .spaces
                        .get_mut(&space)
                        .unwrap()
                        .uses
                        .insert(target.clone());
                } else if let Some(home) = part_homes.get(&target) {
                    let home = home.clone();
                    self.diags.push(
                        Diag::new(
                            E008_USE_NOT_SPACE,
                            Level::Error,
                            &path,
                            span,
                            format!("`{}` is a part, not a space.", target),
                        )
                        .with_fix(
                            format!("Use the part's space instead: `use {}`.", home),
                            vec![Edit {
                                file: path.clone(),
                                start: span.start,
                                end: span.end,
                                text: home.clone(),
                            }],
                        ),
                    );
                    // Recover with the intended edge so resolution continues.
                    self.program
                        .spaces
                        .get_mut(&space)
                        .unwrap()
                        .uses
                        .insert(home);
                } else {
                    let nearest = nearest_name(&target, space_names.iter().map(|s| s.as_str()));
                    let note = match nearest {
                        Some(n) => format!("Did you mean `use {}`?", n),
                        None => "Declare the space, or remove this `use`.".to_string(),
                    };
                    self.diags.push(
                        Diag::new(
                            E008_USE_NOT_SPACE,
                            Level::Error,
                            &path,
                            span,
                            format!("`{}` is not a declared space.", target),
                        )
                        .with_fix(note, vec![]),
                    );
                }
            }
        }
    }

    // -- stages 3-4: cycles, closures, order --------------------------------

    fn closures_and_order(&mut self) {
        let names: Vec<String> = self.program.spaces.keys().cloned().collect();

        // Transitive closure per space over the raw edges (cycles included:
        // mutual visibility holds while E015 is being reported).
        for s in &names {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut stack: Vec<String> = self.program.spaces[s].uses.iter().cloned().collect();
            while let Some(t) = stack.pop() {
                if t == "std" || !seen.insert(t.clone()) {
                    continue;
                }
                if let Some(ti) = self.program.spaces.get(&t) {
                    for u in &ti.uses {
                        if !seen.contains(u) {
                            stack.push(u.clone());
                        }
                    }
                }
            }
            seen.remove(s);
            seen.insert("std".to_string());
            self.program.spaces.get_mut(s).unwrap().closure = seen;
        }

        // Strongly connected components (iterative Kosaraju), then E015 per
        // non-trivial SCC, then Kahn over the condensation.
        let sccs = self.sccs(&names);
        for scc in &sccs {
            let self_loop = scc.len() == 1 && self.program.spaces[&scc[0]].uses.contains(&scc[0]);
            if scc.len() > 1 || self_loop {
                self.report_cycle(scc);
            }
        }
        self.program.order = self.kahn_order(&names, &sccs);
    }

    /// SCCs of the use graph, each sorted; list sorted by smallest member.
    fn sccs(&self, names: &[String]) -> Vec<Vec<String>> {
        // Pass 1: finish order.
        let mut finished: Vec<String> = Vec::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        for start in names {
            if visited.contains(start) {
                continue;
            }
            let mut stack: Vec<(String, bool)> = vec![(start.clone(), false)];
            while let Some((node, processed)) = stack.pop() {
                if processed {
                    finished.push(node);
                    continue;
                }
                if visited.contains(&node) {
                    continue;
                }
                visited.insert(node.clone());
                stack.push((node.clone(), true));
                for t in self.program.spaces[&node].uses.iter() {
                    if t != "std" && self.program.spaces.contains_key(t) && !visited.contains(t) {
                        stack.push((t.clone(), false));
                    }
                }
            }
        }

        // Reverse edges.
        let mut redges: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for s in names {
            for t in &self.program.spaces[s].uses {
                if t != "std" && self.program.spaces.contains_key(t) {
                    redges.entry(t.clone()).or_default().push(s.clone());
                }
            }
        }

        // Pass 2: reverse finish order over reversed edges.
        let mut assigned: BTreeSet<String> = BTreeSet::new();
        let mut sccs: Vec<Vec<String>> = Vec::new();
        for start in finished.iter().rev() {
            if assigned.contains(start) {
                continue;
            }
            let mut comp: Vec<String> = Vec::new();
            let mut stack = vec![start.clone()];
            while let Some(node) = stack.pop() {
                if !assigned.insert(node.clone()) {
                    continue;
                }
                comp.push(node.clone());
                if let Some(preds) = redges.get(&node) {
                    for p in preds {
                        if !assigned.contains(p) {
                            stack.push(p.clone());
                        }
                    }
                }
            }
            comp.sort();
            sccs.push(comp);
        }
        sccs.sort();
        sccs
    }

    /// E015 for one cyclic SCC: path listed from the lexicographically
    /// smallest member, located at that member's `use` of the next hop.
    fn report_cycle(&mut self, scc: &[String]) {
        let smallest = scc[0].clone();
        let in_scc: BTreeSet<&String> = scc.iter().collect();
        // Find a simple path smallest -> ... -> smallest over in-SCC edges.
        let mut cycle: Option<Vec<String>> = None;
        let mut stack: Vec<(String, Vec<String>)> = Vec::new();
        stack.push((smallest.clone(), vec![smallest.clone()]));
        while let Some((node, p)) = stack.pop() {
            if cycle.is_some() {
                break;
            }
            for t in self.program.spaces[&node].uses.iter() {
                if t == &smallest {
                    let mut full = p.clone();
                    full.push(smallest.clone());
                    cycle = Some(full);
                    break;
                }
                if in_scc.contains(t) && !p.contains(t) {
                    let mut np = p.clone();
                    np.push(t.clone());
                    stack.push((t.clone(), np));
                }
            }
        }
        let path = cycle.unwrap_or_else(|| vec![smallest.clone(), smallest.clone()]);

        let mut cause = String::new();
        for (i, s) in path.iter().enumerate() {
            if i > 0 {
                cause.push_str(" uses ");
            }
            cause.push_str(&format!("`{}`", s));
        }
        cause.push('.');

        let next = path[1].clone();
        let (file_idx, span) = self
            .use_sites
            .get(&(smallest.clone(), next))
            .copied()
            .unwrap_or((0, Span::point(1, 1)));
        let path_str = self.file_path(file_idx);
        self.diags.push(
            Diag::new(E015_USE_CYCLE, Level::Error, &path_str, span, cause).with_fix(
                "Remove one `use` in the cycle; dependencies must form a DAG.".to_string(),
                vec![],
            ),
        );
    }

    /// Kahn over the SCC condensation. Members of one SCC emit together,
    /// sorted; ready components emit smallest-representative first; "std"
    /// leads the whole order.
    fn kahn_order(&self, names: &[String], sccs: &[Vec<String>]) -> Vec<String> {
        let mut comp_of: BTreeMap<String, usize> = BTreeMap::new();
        for (i, comp) in sccs.iter().enumerate() {
            for m in comp {
                comp_of.insert(m.clone(), i);
            }
        }
        let mut indegree: Vec<usize> = vec![0; sccs.len()];
        let mut edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); sccs.len()];
        for s in names {
            let sc = comp_of[s];
            for t in &self.program.spaces[s].uses {
                if t == "std" || !self.program.spaces.contains_key(t) {
                    continue;
                }
                let tc = comp_of[t];
                if tc != sc && edges[tc].insert(sc) {
                    indegree[sc] += 1;
                }
            }
        }
        // Ready set keyed by representative (smallest member) name.
        let mut ready: BTreeSet<(String, usize)> = BTreeSet::new();
        for (i, comp) in sccs.iter().enumerate() {
            if indegree[i] == 0 {
                ready.insert((comp[0].clone(), i));
            }
        }
        let mut order: Vec<String> = vec!["std".to_string()];
        while let Some((rep, i)) = ready.iter().next().cloned() {
            ready.remove(&(rep, i));
            for m in &sccs[i] {
                order.push(m.clone());
            }
            for &next in &edges[i] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    ready.insert((sccs[next][0].clone(), next));
                }
            }
        }
        order
    }

    // -- stage 5: parts and layers -------------------------------------------

    fn order_pos(&self, space: &str) -> usize {
        self.program
            .order
            .iter()
            .position(|s| s == space)
            .unwrap_or(usize::MAX)
    }

    fn register_parts(&mut self) {
        // Pass 1: bare declarations introduce.
        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            if !self.program.spaces.contains_key(&space) {
                continue;
            }
            for pi in 0..self.program.files[idx].ast.parts.len() {
                let (nsegs, nspan) = {
                    let p = &self.program.files[idx].ast.parts[pi];
                    (p.name.clone(), p.name_span)
                };
                if nsegs.len() != 1 {
                    continue;
                }
                let full = format!("{}.{}", space, nsegs[0]);
                if let Some(existing) = self.program.parts.get(&full) {
                    let first_file = self.file_path(existing.layers[0].file_idx);
                    let path = self.file_path(idx);
                    self.diags.push(
                        Diag::new(
                            E014_DUP_LAYER,
                            Level::Error,
                            &path,
                            nspan,
                            format!(
                                "`{}` is declared twice in space `{}` (also in {}).",
                                nsegs[0], space, first_file
                            ),
                        )
                        .with_fix(
                            format!("Merge the two `part {}` blocks into one.", nsegs[0]),
                            vec![],
                        ),
                    );
                    continue;
                }
                self.program.parts.insert(
                    full,
                    PartInfo {
                        home: space.clone(),
                        layers: vec![Layer {
                            space: space.clone(),
                            file_idx: idx,
                            part_idx: pi,
                        }],
                    },
                );
            }
        }

        // Pass 2: dotted declarations layer.
        let all_fulls: Vec<String> = self.program.parts.keys().cloned().collect();
        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            if !self.program.spaces.contains_key(&space) {
                continue;
            }
            for pi in 0..self.program.files[idx].ast.parts.len() {
                let (nsegs, nspan) = {
                    let p = &self.program.files[idx].ast.parts[pi];
                    (p.name.clone(), p.name_span)
                };
                if nsegs.len() == 1 {
                    continue;
                }
                let full = ast::name_to_string(&nsegs);
                let path = self.file_path(idx);

                if full.starts_with("std.") {
                    self.diags.push(
                        Diag::new(
                            E017_STD_LAYER,
                            Level::Error,
                            &path,
                            nspan,
                            format!("`{}` is provided by the runtime and cannot be layered.", full),
                        )
                        .with_fix(
                            "Wrap the builtin in your own part instead.".to_string(),
                            vec![],
                        ),
                    );
                    continue;
                }

                let visible = |r: &Resolver, home: &str| -> bool {
                    home == space || r.program.spaces[&space].closure.contains(home)
                };
                match self.program.parts.get(&full) {
                    Some(info) if visible(self, &info.home) => {
                        self.program.parts.get_mut(&full).unwrap().layers.push(Layer {
                            space: space.clone(),
                            file_idx: idx,
                            part_idx: pi,
                        });
                    }
                    Some(info) => {
                        let home = info.home.clone();
                        self.diags.push(
                            Diag::new(
                                E001_UNKNOWN_NAME,
                                Level::Error,
                                &path,
                                nspan,
                                format!("`{}` is not visible from space `{}`.", full, space),
                            )
                            .with_fix(
                                format!("Add `use {}` to make `{}` visible.", home, full),
                                vec![],
                            ),
                        );
                    }
                    None => {
                        let note = match nearest_name(&full, all_fulls.iter().map(|s| s.as_str()))
                        {
                            Some(n) => format!("Did you mean `part {}`?", n),
                            None => {
                                "A dotted part name must match an existing part; declare the base part first.".to_string()
                            }
                        };
                        self.diags.push(
                            Diag::new(
                                E001_UNKNOWN_NAME,
                                Level::Error,
                                &path,
                                nspan,
                                format!("`{}` does not match any visible part.", full),
                            )
                            .with_fix(note, vec![]),
                        );
                    }
                }
            }
        }

        // Order layers; warn on genuinely unordered pairs (C3/W001).
        let fulls: Vec<String> = self.program.parts.keys().cloned().collect();
        for full in fulls {
            let mut layers = self.program.parts[&full].layers.clone();
            let key = |r: &Resolver, l: &Layer| {
                (
                    r.order_pos(&l.space),
                    r.file_path(l.file_idx),
                    l.part_idx,
                )
            };
            layers.sort_by(|a, b| key(self, a).cmp(&key(self, b)));

            // Unordered pairs: distinct spaces, neither in the other's closure.
            let spaces_in_order: Vec<String> = {
                let mut seen = BTreeSet::new();
                layers
                    .iter()
                    .filter(|l| seen.insert(l.space.clone()))
                    .map(|l| l.space.clone())
                    .collect()
            };
            for i in 0..spaces_in_order.len() {
                for j in (i + 1)..spaces_in_order.len() {
                    let x = &spaces_in_order[i];
                    let y = &spaces_in_order[j];
                    let xy = self.program.spaces[x].closure.contains(y);
                    let yx = self.program.spaces[y].closure.contains(x);
                    if !xy && !yx {
                        let (smaller, larger) = if x < y { (x, y) } else { (y, x) };
                        // Anchor on the larger space's layer declaration.
                        let anchor = layers
                            .iter()
                            .find(|l| &l.space == larger)
                            .cloned()
                            .expect("layer for space");
                        let decl_span =
                            self.program.files[anchor.file_idx].ast.parts[anchor.part_idx]
                                .name_span;
                        let anchor_path = self.file_path(anchor.file_idx);
                        // Insertion point: line after the space header of the
                        // larger space's first (sorted) file.
                        let first_file = self.program.spaces[larger].files[0].clone();
                        let header_line = self
                            .program
                            .files
                            .iter()
                            .find(|f| f.path == first_file)
                            .map(|f| f.ast.space_span.start.line)
                            .unwrap_or(1);
                        let ins = Pos {
                            line: header_line + 1,
                            col: 1,
                        };
                        self.diags.push(
                            Diag::new(
                                W001_UNORDERED_LAYERS,
                                Level::Warn,
                                &anchor_path,
                                decl_span,
                                format!(
                                    "layers of `{}` from `{}` and `{}` have no declared order.",
                                    full, x, y
                                ),
                            )
                            .with_fix(
                                format!(
                                    "Add `use {}` to `{}` so the order is declared.",
                                    smaller, larger
                                ),
                                vec![Edit {
                                    file: first_file,
                                    start: ins,
                                    end: ins,
                                    text: format!("use {}\n", smaller),
                                }],
                            ),
                        );
                    }
                }
            }

            self.program.parts.get_mut(&full).unwrap().layers = layers;
        }
    }

    // -- stage 6: foreigns ---------------------------------------------------

    fn register_foreigns(&mut self) {
        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            if !self.program.spaces.contains_key(&space) {
                continue;
            }
            for fi in 0..self.program.files[idx].ast.foreigns.len() {
                let (name, nspan, react) = {
                    let f = &self.program.files[idx].ast.foreigns[fi];
                    (f.name.clone(), f.name_span, f.react.clone())
                };
                let full = format!("{}.{}", space, name);
                let path = self.file_path(idx);
                if self.program.foreigns.contains_key(&full) {
                    self.diags.push(
                        Diag::new(
                            E013_DUP_PROP,
                            Level::Error,
                            &path,
                            nspan,
                            format!("foreign `{}` is declared twice in space `{}`.", name, space),
                        )
                        .with_fix("Remove one of the declarations.".to_string(), vec![]),
                    );
                    continue;
                }
                self.program.foreigns.insert(
                    full,
                    ForeignInfo {
                        space: space.clone(),
                        file_idx: idx,
                        foreign_idx: fi,
                    },
                );
                // A reactive `reads`/`writes` names the collection's data
                // shape (§9.10). It must resolve to some declared part;
                // otherwise a typo would silently break reactivity, exactly
                // the kind of quiet-wrong the checker exists to prevent.
                if let Some(react) = react {
                    let bare = ast::name_to_string(&react.collection);
                    let qualified = format!("{}.{}", space, bare);
                    let resolves = self.program.parts.contains_key(&qualified)
                        || self.program.parts.contains_key(&bare)
                        || self
                            .program
                            .parts
                            .keys()
                            .any(|k| k.rsplit('.').next() == Some(bare.as_str()));
                    if !resolves {
                        self.diags.push(
                            Diag::new(
                                E001_UNKNOWN_NAME,
                                Level::Error,
                                &path,
                                react.span,
                                format!("collection `{}` resolves to no part.", bare),
                            )
                            .with_fix(
                                "Name a declared data shape after `reads`/`writes` (the collection's schema), or add the `use` that provides it.".to_string(),
                                vec![],
                            ),
                        );
                    }
                }
            }
        }
    }

    // -- stage 7: case/separator collisions (E003) ---------------------------

    fn case_collisions(&mut self) {
        fn norm(s: &str) -> String {
            s.to_lowercase().replace(['_', '-'], "")
        }
        // scope key -> [(display name, file, span)], checked per group.
        let mut groups: BTreeMap<String, Vec<(String, String, Span)>> = BTreeMap::new();

        // Space names, globally. Located at each space's first file header.
        let mut space_group: Vec<(String, String, Span)> = Vec::new();
        for (name, info) in &self.program.spaces {
            let first = &info.files[0];
            let span = self
                .program
                .files
                .iter()
                .find(|f| &f.path == first)
                .map(|f| f.ast.space_span)
                .unwrap_or(Span::point(1, 1));
            space_group.push((name.clone(), first.clone(), span));
        }
        groups.insert("space".to_string(), space_group);

        // Part bare names within one space.
        for (full, info) in &self.program.parts {
            let bare = full.rsplit('.').next().unwrap_or(full).to_string();
            let l = &info.layers[0];
            let span = self.program.files[l.file_idx].ast.parts[l.part_idx].name_span;
            let file = self.file_path(l.file_idx);
            groups
                .entry(format!("part:{}", info.home))
                .or_default()
                .push((bare, file, span));
        }

        // Property names within one part (across all layers).
        for (full, info) in &self.program.parts {
            let mut props: Vec<(String, String, Span)> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for l in &info.layers {
                for p in &self.program.files[l.file_idx].ast.parts[l.part_idx].props {
                    if seen.insert(p.name.clone()) {
                        props.push((p.name.clone(), self.file_path(l.file_idx), p.name_span));
                    }
                }
            }
            groups.insert(format!("prop:{}", full), props);
        }

        // Foreign names within one space.
        for (full, info) in &self.program.foreigns {
            let bare = full.rsplit('.').next().unwrap_or(full).to_string();
            let f = &self.program.files[info.file_idx].ast.foreigns[info.foreign_idx];
            let file = self.file_path(info.file_idx);
            groups
                .entry(format!("foreign:{}", info.space))
                .or_default()
                .push((bare, file, f.name_span));
        }

        for (_, members) in groups {
            let mut by_norm: BTreeMap<String, Vec<&(String, String, Span)>> = BTreeMap::new();
            for m in &members {
                by_norm.entry(norm(&m.0)).or_default().push(m);
            }
            for (_, hits) in by_norm {
                if hits.len() > 1 {
                    let names: Vec<String> =
                        hits.iter().map(|(n, _, _)| format!("`{}`", n)).collect();
                    let (_, file, span) = hits[hits.len() - 1];
                    self.diags.push(
                        Diag::new(
                            E003_CASE_COLLISION,
                            Level::Error,
                            file,
                            *span,
                            format!(
                                "{} differ only by case or separator.",
                                names.join(" and ")
                            ),
                        )
                        .with_fix("Rename one of them.".to_string(), vec![]),
                    );
                }
            }
        }
    }

    // -- stage 8: body resolution --------------------------------------------

    fn resolve_bodies(&mut self) {
        // Global bare-name -> homes map, for "add `use`" suggestions.
        let mut all_parts_by_bare: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (full, info) in &self.program.parts {
            let bare = full.rsplit('.').next().unwrap_or(full).to_string();
            all_parts_by_bare
                .entry(bare)
                .or_default()
                .push(info.home.clone());
        }

        // Per-space visibility tables, computed once. An index from home
        // space to its parts/foreigns keeps this linear in (spaces ×
        // closure size), not (spaces × all parts) — the F1-relevant path.
        let mut parts_by_home: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for (full, info) in &self.program.parts {
            parts_by_home.entry(info.home.as_str()).or_default().push(full);
        }
        let mut foreigns_by_home: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for (full, info) in &self.program.foreigns {
            foreigns_by_home
                .entry(info.space.as_str())
                .or_default()
                .push(full);
        }
        type SpaceTables = (
            BTreeMap<String, Vec<String>>,
            BTreeSet<String>,
            BTreeSet<String>,
        );
        let mut per_space: BTreeMap<String, SpaceTables> = BTreeMap::new();
        for space in self.program.spaces.keys() {
            let mut bare: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut fulls: BTreeSet<String> = BTreeSet::new();
            let mut foreign_fulls: BTreeSet<String> = BTreeSet::new();
            let closure = &self.program.spaces[space].closure;
            let visible = std::iter::once(space.as_str())
                .chain(closure.iter().map(|s| s.as_str()));
            for home in visible {
                for full in parts_by_home.get(home).into_iter().flatten() {
                    let b = full.rsplit('.').next().unwrap_or(full).to_string();
                    bare.entry(b).or_default().push((*full).clone());
                    fulls.insert((*full).clone());
                }
                for full in foreigns_by_home.get(home).into_iter().flatten() {
                    let b = full.rsplit('.').next().unwrap_or(full).to_string();
                    bare.entry(b).or_default().push((*full).clone());
                    fulls.insert((*full).clone());
                    foreign_fulls.insert((*full).clone());
                }
            }
            for p in STD_PARTS {
                bare.entry((*p).to_string())
                    .or_default()
                    .push(format!("std.{}", p));
                fulls.insert(format!("std.{}", p));
            }
            for v in bare.values_mut() {
                v.sort();
                v.dedup();
            }
            per_space.insert(space.clone(), (bare, fulls, foreign_fulls));
        }

        for idx in 0..self.program.files.len() {
            let space = ast::name_to_string(&self.program.files[idx].ast.space);
            let Some((bare, fulls, foreign_fulls)) = per_space.get(&space) else {
                continue;
            };
            let path = self.file_path(idx);
            for pi in 0..self.program.files[idx].ast.parts.len() {
                let decl = &self.program.files[idx].ast.parts[pi];
                // Enclosing part: union of property names across all layers
                // when the declaration resolved; the decl's own props otherwise.
                let full = if decl.name.len() == 1 {
                    format!("{}.{}", space, decl.name[0])
                } else {
                    ast::name_to_string(&decl.name)
                };
                let mut enclosing: BTreeSet<String> = BTreeSet::new();
                let mut storage: BTreeSet<String> = BTreeSet::new();
                if let Some(info) = self.program.parts.get(&full) {
                    for l in &info.layers {
                        for p in &self.program.files[l.file_idx].ast.parts[l.part_idx].props {
                            enclosing.insert(p.name.clone());
                            if p.storage.is_some() {
                                storage.insert(p.name.clone());
                            }
                        }
                    }
                } else {
                    for p in &decl.props {
                        enclosing.insert(p.name.clone());
                        if p.storage.is_some() {
                            storage.insert(p.name.clone());
                        }
                    }
                }
                let mut w = Walk {
                    program: &self.program,
                    file: path.clone(),
                    bare,
                    fulls,
                    foreign_fulls,
                    enclosing: &enclosing,
                    storage: &storage,
                    all_parts_by_bare: &all_parts_by_bare,
                    space: space.clone(),
                    diags: Vec::new(),
                    locals: Vec::new(),
                };
                for p in &self.program.files[idx].ast.parts[pi].props {
                    if let Some(sh) = &p.shape {
                        w.walk_shape(sh);
                    }
                    if let Some(v) = &p.value {
                        w.walk_expr(v, true);
                    }
                }
                self.diags.append(&mut w.diags);
            }

            // Foreign declarations: their parameter and return shapes may
            // name parts and are resolved with an empty enclosing scope.
            if !self.program.files[idx].ast.foreigns.is_empty() {
                let empty = BTreeSet::new();
                let mut w = Walk {
                    program: &self.program,
                    file: path.clone(),
                    bare,
                    fulls,
                    foreign_fulls,
                    enclosing: &empty,
                    storage: &empty,
                    all_parts_by_bare: &all_parts_by_bare,
                    space: space.clone(),
                    diags: Vec::new(),
                    locals: Vec::new(),
                };
                for fd in &self.program.files[idx].ast.foreigns {
                    for (_, ps) in &fd.params {
                        w.walk_shape(ps);
                    }
                    w.walk_shape(&fd.ret);
                }
                self.diags.append(&mut w.diags);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expression/statement walker: one per part declaration.
// ---------------------------------------------------------------------------

struct Walk<'a> {
    program: &'a Program,
    file: String,
    space: String,
    /// Visible bare name -> full names (parts, std parts, foreigns).
    bare: &'a BTreeMap<String, Vec<String>>,
    /// Visible full names.
    fulls: &'a BTreeSet<String>,
    /// The subset of `fulls` that are foreign functions — excluded from
    /// shape positions, where only parts may be named.
    foreign_fulls: &'a BTreeSet<String>,
    /// The enclosing part's property names (all layers).
    enclosing: &'a BTreeSet<String>,
    /// The subset of `enclosing` with a storage word on some definition.
    storage: &'a BTreeSet<String>,
    /// Global part bare name -> home spaces (for `use` suggestions).
    all_parts_by_bare: &'a BTreeMap<String, Vec<String>>,
    diags: Vec<Diag>,
    locals: Vec<BTreeSet<String>>,
}

impl<'a> Walk<'a> {
    fn local_visible(&self, name: &str) -> bool {
        self.locals.iter().any(|f| f.contains(name))
    }

    fn name_visible(&self, name: &str) -> bool {
        self.local_visible(name)
            || self.enclosing.contains(name)
            || self.bare.contains_key(name)
            || STD_FNS.contains(&name)
    }

    /// Declare a `let`/param/`for` binding; no shadowing anywhere (E002).
    fn declare_local(&mut self, name: &str, span: Span) {
        if self.name_visible(name) {
            self.diags.push(
                Diag::new(
                    E002_AMBIGUOUS_NAME,
                    Level::Error,
                    &self.file,
                    span,
                    format!("`{}` shadows a visible name; there is no shadowing in Ashlar.", name),
                )
                .with_fix("Rename the local.".to_string(), vec![]),
            );
            return;
        }
        if let Some(top) = self.locals.last_mut() {
            top.insert(name.to_string());
        }
    }

    /// Resolve every part name appearing in a shape (B3 applies to shape
    /// positions too). Field-level shape *checking* is a later increment;
    /// name existence and uniqueness are checked here.
    fn walk_shape(&mut self, s: &crate::ast::SShape) {
        use crate::ast::Shape;
        match &s.shape {
            Shape::Text | Shape::Number | Shape::Bool | Shape::Data => {}
            Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => self.walk_shape(i),
            Shape::Fn(params, ret) => {
                for (_, ps) in params {
                    self.walk_shape(ps);
                }
                self.walk_shape(ret);
            }
            Shape::Part(name) => self.resolve_shape_part(name, s.span),
        }
    }

    fn resolve_shape_part(&mut self, segs: &[String], span: Span) {
        let joined = segs.join(".");
        if segs.len() == 1 {
            let cands: Vec<String> = self
                .bare
                .get(&segs[0])
                .map(|v| {
                    v.iter()
                        .filter(|f| !self.foreign_fulls.contains(*f))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            match cands.len() {
                1 => return,
                0 => {}
                _ => {
                    self.diags.push(
                        Diag::new(
                            E002_AMBIGUOUS_NAME,
                            Level::Error,
                            &self.file,
                            span,
                            format!(
                                "`{}` is ambiguous: {} are all visible.",
                                joined,
                                cands
                                    .iter()
                                    .map(|f| format!("`{}`", f))
                                    .collect::<Vec<_>>()
                                    .join(" and ")
                            ),
                        )
                        .with_fix(
                            format!(
                                "Qualify it; alternatives: {}.",
                                cands
                                    .iter()
                                    .map(|f| format!("`{}`", f))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            vec![Edit {
                                file: self.file.clone(),
                                start: span.start,
                                end: span.end,
                                text: cands[0].clone(),
                            }],
                        ),
                    );
                    return;
                }
            }
        } else if self.fulls.contains(&joined) && !self.foreign_fulls.contains(&joined) {
            return;
        }
        // Unknown shape name: E001 with the nearest part name.
        let candidates = self
            .bare
            .keys()
            .map(|s| s.as_str())
            .chain(self.fulls.iter().map(|s| s.as_str()));
        let mut note = match nearest_name(&joined, candidates) {
            Some(n) => format!("Did you mean `{}`?", n),
            None => String::new(),
        };
        if let Some(homes) = self.all_parts_by_bare.get(&segs[0]) {
            if !self.bare.contains_key(&segs[0]) {
                if let Some(home) = homes.first() {
                    if !note.is_empty() {
                        note.push(' ');
                    }
                    note.push_str(&format!(
                        "`{}.{}` exists; add `use {}` to `{}` to bring it into scope.",
                        home, segs[0], home, self.space
                    ));
                }
            }
        }
        if note.is_empty() {
            note = "Declare the part, or check the spelling.".to_string();
        }
        self.diags.push(
            Diag::new(
                E001_UNKNOWN_NAME,
                Level::Error,
                &self.file,
                span,
                format!("`{}` does not name a visible part.", joined),
            )
            .with_fix(note, vec![]),
        );
    }

    /// Longest-prefix resolution of a dotted chain (ast.rs module docs).
    /// Remaining segments are field accesses, validated by the shape checker
    /// in a later increment — not here.
    fn resolve_nameref(&mut self, segs: &[String], span: Span) {
        for k in (1..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            let mut candidates: Vec<String> = Vec::new();
            if k == 1 {
                let n = &segs[0];
                if self.local_visible(n) {
                    candidates.push(format!("the local `{}`", n));
                }
                if self.enclosing.contains(n) {
                    candidates.push(format!("this part's `{}`", n));
                }
                if let Some(fulls) = self.bare.get(n) {
                    for f in fulls {
                        candidates.push(format!("`{}`", f));
                    }
                }
                if STD_FNS.contains(&n.as_str()) {
                    candidates.push(format!("the builtin `{}`", n));
                }
            } else if self.fulls.contains(&prefix) {
                candidates.push(format!("`{}`", prefix));
            }
            if candidates.len() == 1 {
                return;
            }
            if candidates.len() > 1 {
                // Attach a qualifying rewrite only when the ambiguous prefix
                // is the entire chain: the span then covers exactly the text
                // to replace. (Chains are single spans in the AST.)
                let part_fulls: Vec<String> = self
                    .bare
                    .get(&segs[0])
                    .cloned()
                    .unwrap_or_default();
                let mut d = Diag::new(
                    E002_AMBIGUOUS_NAME,
                    Level::Error,
                    &self.file,
                    span,
                    format!(
                        "`{}` is ambiguous: it could be {}.",
                        prefix,
                        candidates.join(" or ")
                    ),
                );
                if k == segs.len() && part_fulls.len() > 1 && candidates.len() == part_fulls.len()
                {
                    d = d.with_fix(
                        format!(
                            "Qualify it; alternatives: {}.",
                            part_fulls
                                .iter()
                                .map(|f| format!("`{}`", f))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        vec![Edit {
                            file: self.file.clone(),
                            start: span.start,
                            end: span.end,
                            text: part_fulls[0].clone(),
                        }],
                    );
                } else {
                    d = d.with_fix("Qualify the name with its space.".to_string(), vec![]);
                }
                self.diags.push(d);
                return;
            }
        }

        // Nothing matched any prefix: E001.
        let joined = segs.join(".");
        let first = &segs[0];
        if segs.len() == 1 && matches!(first.as_str(), "null" | "nil" | "undefined") {
            self.diags.push(
                Diag::new(
                    E001_UNKNOWN_NAME,
                    Level::Error,
                    &self.file,
                    span,
                    format!("`{}` does not exist; absence is `none`.", first),
                )
                .with_fix(
                    "Replace it with `none`.".to_string(),
                    vec![Edit {
                        file: self.file.clone(),
                        start: span.start,
                        end: span.end,
                        text: "none".to_string(),
                    }],
                ),
            );
            return;
        }
        let mut note = String::new();
        let candidates = self
            .bare
            .keys()
            .map(|s| s.as_str())
            .chain(self.fulls.iter().map(|s| s.as_str()))
            .chain(self.enclosing.iter().map(|s| s.as_str()))
            .chain(STD_FNS.iter().copied());
        if let Some(n) = nearest_name(&joined, candidates) {
            note.push_str(&format!("Did you mean `{}`?", n));
        }
        if let Some(homes) = self.all_parts_by_bare.get(first) {
            let visible = self.bare.contains_key(first);
            if !visible {
                if let Some(home) = homes.first() {
                    if !note.is_empty() {
                        note.push(' ');
                    }
                    note.push_str(&format!(
                        "`{}.{}` exists; add `use {}` to `{}` to bring it into scope.",
                        home, first, home, self.space
                    ));
                }
            }
        }
        if note.is_empty() {
            note = "Declare it, or check the spelling.".to_string();
        }
        let cause = if segs.len() == 1 {
            format!("`{}` is not a name in scope.", joined)
        } else {
            format!("`{}` does not resolve to a visible name.", joined)
        };
        self.diags.push(
            Diag::new(E001_UNKNOWN_NAME, Level::Error, &self.file, span, cause)
                .with_fix(note, vec![]),
        );
    }

    fn walk_expr(&mut self, e: &SExpr, fnlit_ok: bool) {
        match &e.expr {
            Expr::Text(_) | Expr::Number(_) | Expr::Bool(_) | Expr::NoneLit => {}
            Expr::NameRef(segs) => self.resolve_nameref(segs, e.span),
            Expr::List(items) => {
                for it in items {
                    match it {
                        ListItem::Item(x) | ListItem::Spread(x) => self.walk_expr(x, false),
                    }
                }
            }
            Expr::MapLit(items) => {
                for it in items {
                    match it {
                        MapItem::Entry(_, _, v) => self.walk_expr(v, false),
                        MapItem::Spread(x) => self.walk_expr(x, false),
                    }
                }
            }
            Expr::Field(b, _, _) => self.walk_expr(b, false),
            Expr::Index(b, i) => {
                self.walk_expr(b, false);
                self.walk_expr(i, false);
            }
            Expr::Call(callee, args) => {
                self.walk_expr(callee, false);
                for a in args {
                    // A function literal may be the root of a call argument.
                    self.walk_expr(a, true);
                }
            }
            Expr::Unary(_, x) => self.walk_expr(x, false),
            Expr::Assert(x) => self.walk_expr(x, false),
            Expr::Binary(_, l, r) => {
                self.walk_expr(l, false);
                self.walk_expr(r, false);
            }
            Expr::IfExpr(cond, then, els) => {
                self.walk_expr(cond, false);
                self.walk_block(then);
                self.walk_block(els);
            }
            Expr::FnLit(params, body) => {
                if !fnlit_ok {
                    self.diags.push(
                        Diag::new(
                            E024_FNLIT_POSITION,
                            Level::Error,
                            &self.file,
                            e.span,
                            "a function literal is only allowed as a property's value or as a call argument.".to_string(),
                        )
                        .with_fix(
                            "Make it a named property and reference it by name.".to_string(),
                            vec![],
                        ),
                    );
                }
                self.locals.push(BTreeSet::new());
                for p in params {
                    self.walk_shape(&p.shape);
                    self.declare_local(&p.name, p.name_span);
                }
                match body.as_ref() {
                    FnBody::Expr(x) => self.walk_expr(x, false),
                    FnBody::Block(stmts) => self.walk_block(stmts),
                }
                self.locals.pop();
            }
        }
    }

    fn walk_block(&mut self, stmts: &[Stmt]) {
        self.locals.push(BTreeSet::new());
        for s in stmts {
            self.walk_stmt(s);
        }
        self.locals.pop();
    }

    fn walk_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(name, span, e) => {
                self.walk_expr(e, false);
                self.declare_local(name, *span);
            }
            Stmt::Assign(name, span, e) => {
                self.check_assign_target(name, *span);
                self.walk_expr(e, false);
            }
            Stmt::If(cond, then, els) => {
                self.walk_expr(cond, false);
                self.walk_block(then);
                if let Some(els) = els {
                    self.walk_block(els);
                }
            }
            Stmt::For(vars, iter, body) => {
                self.walk_expr(iter, false);
                self.locals.push(BTreeSet::new());
                for (v, sp) in vars {
                    self.declare_local(v, *sp);
                }
                for st in body {
                    self.walk_stmt(st);
                }
                self.locals.pop();
            }
            Stmt::Return(Some(e), _) => self.walk_expr(e, false),
            Stmt::Return(None, _) => {}
            Stmt::Expr(e) => self.walk_expr(e, false),
        }
    }

    fn check_assign_target(&mut self, name: &str, span: Span) {
        if self.storage.contains(name) {
            return;
        }
        if self.enclosing.contains(name) {
            self.diags.push(
                Diag::new(
                    E025_BAD_ASSIGN,
                    Level::Error,
                    &self.file,
                    span,
                    format!("`{}` is not a state property and cannot be assigned.", name),
                )
                .with_fix(
                    format!(
                        "Declare `{}` with `state` or `stored` on its base layer, or compute a new value instead.",
                        name
                    ),
                    vec![],
                ),
            );
            return;
        }
        if self.local_visible(name) {
            self.diags.push(
                Diag::new(
                    E025_BAD_ASSIGN,
                    Level::Error,
                    &self.file,
                    span,
                    format!("`{}` is a `let` local; locals are single-assignment.", name),
                )
                .with_fix(format!("Bind a new name instead of reassigning `{}`.", name), vec![]),
            );
            return;
        }
        let nearest = nearest_name(name, self.enclosing.iter().map(|s| s.as_str()));
        let note = match nearest {
            Some(n) => format!("Did you mean `{}`?", n),
            None => "Declare it as a storage property on this part first.".to_string(),
        };
        self.diags.push(
            Diag::new(
                E001_UNKNOWN_NAME,
                Level::Error,
                &self.file,
                span,
                format!("`{}` is not a property of this part.", name),
            )
            .with_fix(note, vec![]),
        );
        let _ = &self.program; // used for future shape-aware checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    /// Lex+parse+resolve a set of (path, source) fixtures. Asserts the
    /// front end produced no diagnostics so failures point at the resolver.
    fn resolve_srcs(files: &[(&str, &str)]) -> (Program, Vec<Diag>) {
        let mut entries = Vec::new();
        for (path, src) in files {
            let (toks, lex_diags) = lexer::lex(path, src);
            assert!(lex_diags.is_empty(), "lex diags in {}: {:?}", path, lex_diags);
            let (ast, parse_diags) = parser::parse(path, &toks);
            assert!(
                parse_diags.is_empty(),
                "parse diags in {}: {:?}",
                path,
                parse_diags
            );
            entries.push(FileEntry {
                path: path.to_string(),
                ast: ast.expect("fixture must parse"),
            });
        }
        resolve(entries)
    }

    fn ids(diags: &[Diag]) -> Vec<&'static str> {
        diags.iter().map(|d| d.id).collect()
    }

    #[test]
    fn transitive_visibility_and_order() {
        let (prog, diags) = resolve_srcs(&[
            ("a.ash", "space a\n\npart Widget {\n  x: text\n}\n"),
            ("b.ash", "space b\nuse a\n\npart Mid {\n  y: text\n}\n"),
            (
                "c.ash",
                "space c\nuse b\n\npart Top {\n  w: Widget?\n  go = () => Widget\n}\n",
            ),
        ]);
        assert!(diags.is_empty(), "{:?}", diags);
        assert_eq!(prog.order, vec!["std", "a", "b", "c"]);
        assert!(prog.spaces["c"].closure.contains("a"));
        assert!(prog.spaces["c"].closure.contains("std"));
        assert!(!prog.spaces["c"].closure.contains("c"));
    }

    #[test]
    fn e001_unknown_with_nearest_note() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart Widget {\n  x: text\n}\n\npart Use {\n  go = () => Wodget\n}\n",
        )]);
        assert_eq!(ids(&diags), vec!["E001"]);
        let note = &diags[0].fix.as_ref().unwrap().note;
        assert!(note.contains("Widget"), "note was: {}", note);
    }

    #[test]
    fn e001_null_gets_none_edit() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  go = () => null\n}\n",
        )]);
        assert_eq!(ids(&diags), vec!["E001"]);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, "none");
    }

    #[test]
    fn e002_ambiguous_bare_with_qualify_fix() {
        let (_, diags) = resolve_srcs(&[
            ("a.ash", "space chat.data\n\npart Message {\n  x: text\n}\n"),
            ("b.ash", "space note\n\npart Message {\n  y: text\n}\n"),
            (
                "c.ash",
                "space app\nuse chat.data\nuse note\n\npart V {\n  go = () => Message\n}\n",
            ),
        ]);
        assert_eq!(ids(&diags), vec!["E002"]);
        assert!(diags[0].cause.contains("ambiguous"));
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, "chat.data.Message");
        assert!(fix.note.contains("note.Message"));
    }

    #[test]
    fn e002_shadowing_local() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  go = () => {\n    let len = 1\n    return len\n  }\n}\n",
        )]);
        // One shadow error for the let; the later `len` read then resolves
        // to the builtin, so no second diagnostic.
        assert_eq!(ids(&diags), vec!["E002"]);
        assert!(diags[0].cause.contains("shadows"));
    }

    #[test]
    fn e003_case_collision_props() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  userName: text\n  user_name: text\n}\n",
        )]);
        assert_eq!(ids(&diags), vec!["E003"]);
        assert!(diags[0].cause.contains("userName") && diags[0].cause.contains("user_name"));
    }

    #[test]
    fn e008_use_of_part_with_rewrite() {
        let (_, diags) = resolve_srcs(&[
            ("a.ash", "space demo\n\npart Widget {\n  x: text\n}\n"),
            ("b.ash", "space app\nuse demo.Widget\n\npart V {\n  w: Widget?\n}\n"),
        ]);
        assert_eq!(ids(&diags), vec!["E008"]);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, "demo");
        // Recovery keeps the intended edge, so Widget still resolves.
    }

    #[test]
    fn e014_duplicate_bare_part() {
        let (_, diags) = resolve_srcs(&[
            ("one.ash", "space demo\n\npart Widget {\n  x: text\n}\n"),
            ("two.ash", "space demo\n\npart Widget {\n  y: text\n}\n"),
        ]);
        assert_eq!(ids(&diags), vec!["E014"]);
        assert!(diags[0].cause.contains("one.ash"));
    }

    #[test]
    fn e015_cycle() {
        let (prog, diags) = resolve_srcs(&[
            ("a.ash", "space a\nuse b\n\npart A {\n  x: text\n}\n"),
            ("b.ash", "space b\nuse a\n\npart B {\n  y: text\n}\n"),
        ]);
        assert_eq!(ids(&diags), vec!["E015"]);
        assert!(diags[0].cause.contains("`a` uses `b` uses `a`"));
        // Order still total and deterministic after the break.
        assert_eq!(prog.order, vec!["std", "a", "b"]);
    }

    #[test]
    fn e017_std_layer_and_std_space() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart std.Request {\n  extra: text\n}\n",
        )]);
        assert_eq!(ids(&diags), vec!["E017"]);

        let (_, diags2) = resolve_srcs(&[("s.ash", "space std\n\npart X {\n  x: text\n}\n")]);
        assert_eq!(ids(&diags2), vec!["E017"]);
    }

    #[test]
    fn dotted_declaration_typo_e001() {
        let (_, diags) = resolve_srcs(&[
            ("a.ash", "space chat.data\n\npart Message {\n  x: text\n}\n"),
            (
                "b.ash",
                "space audit\nuse chat.data\n\npart chat.data.Mesage {\n  y: text\n}\n",
            ),
        ]);
        assert_eq!(ids(&diags), vec!["E001"]);
        let note = &diags[0].fix.as_ref().unwrap().note;
        assert!(note.contains("chat.data.Message"), "note: {}", note);
    }

    #[test]
    fn w001_unordered_layers_with_use_insertion() {
        let (prog, diags) = resolve_srcs(&[
            ("base.ash", "space base\n\npart W {\n  x: text\n}\n"),
            ("p.ash", "space p\nuse base\n\npart base.W {\n  y: text\n}\n"),
            ("q.ash", "space q\nuse base\n\npart base.W {\n  z: text\n}\n"),
        ]);
        assert_eq!(ids(&diags), vec!["W001"]);
        assert_eq!(diags[0].level, Level::Warn);
        let fix = diags[0].fix.as_ref().unwrap();
        // q is lexicographically larger: it gains `use p`, inserted in q's file.
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].file, "q.ash");
        assert_eq!(fix.edits[0].text, "use p\n");
        assert_eq!(fix.edits[0].start.line, 2);
        assert_eq!(fix.edits[0].start.col, 1);
        // Deterministic layer order: base, then p, then q (lexicographic).
        let layers: Vec<&str> = prog.parts["base.W"]
            .layers
            .iter()
            .map(|l| l.space.as_str())
            .collect();
        assert_eq!(layers, vec!["base", "p", "q"]);
    }

    #[test]
    fn determinism_under_shuffle() {
        let fixtures: Vec<(&str, &str)> = vec![
            ("a.ash", "space a\n\npart A {\n  x: text\n}\n"),
            ("b.ash", "space b\nuse a\n\npart a.A {\n  y: text\n}\n"),
            ("c.ash", "space c\nuse b\n\npart a.A {\n  z: text\n}\n"),
        ];
        let (p1, d1) = resolve_srcs(&fixtures);
        let mut shuffled = fixtures.clone();
        shuffled.reverse();
        let (p2, d2) = resolve_srcs(&shuffled);
        assert!(d1.is_empty() && d2.is_empty());
        assert_eq!(p1.order, p2.order);
        let l1: Vec<&str> = p1.parts["a.A"].layers.iter().map(|l| l.space.as_str()).collect();
        let l2: Vec<&str> = p2.parts["a.A"].layers.iter().map(|l| l.space.as_str()).collect();
        assert_eq!(l1, l2);
        assert_eq!(l1, vec!["a", "b", "c"]);
    }

    #[test]
    fn e024_fnlit_positions() {
        // Legal: property value root, call argument root.
        let (_, ok) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  f = (x: number) => x\n  go = () => map([1], (v: number) => v)\n}\n",
        )]);
        assert!(ok.is_empty(), "{:?}", ok);

        // Illegal: let-bound and inside a list literal.
        let (_, bad) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  go = () => {\n    let f = (x: number) => x\n    return [(y: number) => y]\n  }\n}\n",
        )]);
        let e024s = bad.iter().filter(|d| d.id == "E024").count();
        assert_eq!(e024s, 2, "{:?}", bad);
    }

    #[test]
    fn e025_assign_targets() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  fixed: text = \"v\"\n  state n: number = 0\n  go = () => {\n    n = 1\n    fixed = \"w\"\n  }\n}\n",
        )]);
        assert_eq!(ids(&diags), vec!["E025"]);
        assert!(diags[0].cause.contains("`fixed`"));
    }

    #[test]
    fn foreign_resolves_and_duplicate_is_e013() {
        let (_, ok) = resolve_srcs(&[(
            "a.ash",
            "space net\n\nforeign fetch: (url: text) -> data\n\npart W {\n  go = (u: text) => fetch(u)\n}\n",
        )]);
        assert!(ok.is_empty(), "{:?}", ok);

        let (_, dup) = resolve_srcs(&[(
            "a.ash",
            "space net\n\nforeign fetch: (url: text) -> data\nforeign fetch: (url: text) -> data\n",
        )]);
        assert_eq!(ids(&dup), vec!["E013"]);
    }

    #[test]
    fn std_names_resolve() {
        let (_, diags) = resolve_srcs(&[(
            "a.ash",
            "space a\n\npart W {\n  port = 8080\n  handle pipe = (req: std.Request) => req.user\n  go = () => log.info(\"x\", { a: id() })\n}\n",
        )]);
        assert!(diags.is_empty(), "{:?}", diags);
    }
}

/// Levenshtein distance, early-outing above `cap`.
fn lev(a: &str, b: &str, cap: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > cap {
        return cap + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            row.push((prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = row;
    }
    prev[b.len()]
}

/// The candidate within edit distance 2 of `target`, closest first;
/// ties broken lexicographically.
fn nearest_name<'a>(target: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for c in candidates {
        let d = lev(target, c, 2);
        if d <= 2 {
            let better = match &best {
                None => true,
                Some((bd, bn)) => d < *bd || (d == *bd && c < bn.as_str()),
            };
            if better {
                best = Some((d, c.to_string()));
            }
        }
    }
    best.map(|(_, n)| n)
}
