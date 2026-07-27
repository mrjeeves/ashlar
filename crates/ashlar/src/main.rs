//! CLI (reference §11).
//!
//! Every command in the reference's toolchain table exists here: `check`,
//! `fix`, `build`, `fmt`, `run`, `rename`, `rekind`, `move`, `radius`,
//! `vendor`. Nothing else — a command that doesn't fully work is not
//! present at all, so any other input (including no input) is a usage
//! error, never a stub.
//!
//! Argument parsing lives in `cli::parse`, a pure function over `&[String]`
//! so it's unit-testable without touching the process's real argv or exit
//! code. `main` itself is the only place that talks to the outside world:
//! `std::env::args`, stdout/stderr, and `std::process::exit`.

mod cli {
    use ashlar::diag::Diag;
    use std::path::Path;

    pub const USAGE: &str = "usage:\n  \
        ashlar check [path] [--human]\n  \
        ashlar fix [id] [path]\n  \
        ashlar build [path]\n  \
        ashlar fmt [path] [--check]\n  \
        ashlar run [part] [path] [--port n] [--mesh [network]]\n  \
        ashlar rename <space-part-or-prop> <new-name> [path] [--plan]\n  \
        ashlar rekind <part.prop> <kind> [path] [--plan]\n  \
        ashlar move <part> <space> [path] [--plan]\n  \
        ashlar radius <full-name> [path]\n  \
    ashlar delta [path]\n  \
        ashlar vendor <source> [path]\n  \
        ashlar foreign check [space] [path]\n  \
        ashlar mesh [path]\n";

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Cmd {
        Check { path: String, human: bool },
        Fix { path: String, id: Option<String> },
        Build { path: String },
        Fmt { path: String, check_only: bool },
        Run {
            path: String,
            part: Option<String>,
            port: Option<u16>,
            /// `--mesh` with no name means the mesh the machine's daemon
            /// calls its default; `--mesh <name>` names one. Absent is
            /// `None`: a site is not published unless asked.
            mesh: Option<String>,
        },
        Rename { target: String, new_name: String, path: String, plan_only: bool },
        Rekind { target: String, kind: String, path: String, plan_only: bool },
        Move { part: String, space: String, path: String, plan_only: bool },
        Radius { target: String, path: String },
        Delta { path: String },
        Vendor { source: String, path: String },
        ForeignCheck { space: Option<String>, path: String },
        Mesh { path: String },
    }

    /// Parse the command and its arguments (everything after the binary
    /// name — callers pass `std::env::args().skip(1)`). `Err` carries a
    /// reason for diagnosis; every `Err` is treated identically by `main`:
    /// print the fixed usage text and exit 2.
    pub fn parse(args: &[String]) -> Result<Cmd, String> {
        let (name, rest) = args
            .split_first()
            .ok_or_else(|| "no command given".to_string())?;

        match name.as_str() {
            "check" => {
                let mut path: Option<String> = None;
                let mut human = false;
                for a in rest {
                    if a == "--human" {
                        if human {
                            return Err("`--human` given twice".to_string());
                        }
                        human = true;
                    } else if let Some(p) = positional(a, &path)? {
                        path = Some(p);
                    }
                }
                Ok(Cmd::Check {
                    path: path.unwrap_or_else(default_path),
                    human,
                })
            }
            "fix" => {
                // `fix [id] [path]`: a diagnostic id (E006, W001) filters
                // which machine fixes apply (reference §11).
                let mut id: Option<String> = None;
                let mut path: Option<String> = None;
                for a in rest {
                    if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    }
                    let looks_like_id = a.len() == 4
                        && (a.starts_with('E') || a.starts_with('W'))
                        && a[1..].chars().all(|c| c.is_ascii_digit());
                    if looks_like_id && id.is_none() {
                        id = Some(a.clone());
                    } else if path.is_none() {
                        path = Some(a.clone());
                    } else {
                        return Err("too many arguments".to_string());
                    }
                }
                Ok(Cmd::Fix {
                    path: path.unwrap_or_else(default_path),
                    id,
                })
            }
            "build" => Ok(Cmd::Build {
                path: one_path(rest)?,
            }),
            "rename" | "rekind" | "move" => {
                let mut positionals: Vec<String> = Vec::new();
                let mut plan_only = false;
                for a in rest {
                    if a == "--plan" {
                        plan_only = true;
                    } else if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    } else {
                        positionals.push(a.clone());
                    }
                }
                if positionals.len() < 2 || positionals.len() > 3 {
                    return Err(format!("`{}` takes a target, a new value, and an optional path", name));
                }
                let path = positionals.get(2).cloned().unwrap_or_else(default_path);
                match name.as_str() {
                    "rename" => Ok(Cmd::Rename {
                        target: positionals[0].clone(),
                        new_name: positionals[1].clone(),
                        path,
                        plan_only,
                    }),
                    "move" => Ok(Cmd::Move {
                        part: positionals[0].clone(),
                        space: positionals[1].clone(),
                        path,
                        plan_only,
                    }),
                    _ => Ok(Cmd::Rekind {
                        target: positionals[0].clone(),
                        kind: positionals[1].replace('+', " "),
                        path,
                        plan_only,
                    }),
                }
            }
            "radius" => {
                let mut positionals: Vec<String> = Vec::new();
                for a in rest {
                    if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    }
                    positionals.push(a.clone());
                }
                if positionals.is_empty() || positionals.len() > 2 {
                    return Err("`radius` takes a full name and an optional path".to_string());
                }
                Ok(Cmd::Radius {
                    target: positionals[0].clone(),
                    path: positionals.get(1).cloned().unwrap_or_else(default_path),
                })
            }
            "delta" => {
                let mut positionals: Vec<String> = Vec::new();
                for a in rest {
                    if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    }
                    positionals.push(a.clone());
                }
                if positionals.len() > 1 {
                    return Err("`delta` takes an optional path".to_string());
                }
                Ok(Cmd::Delta {
                    path: positionals.first().cloned().unwrap_or_else(default_path),
                })
            }
            "vendor" => {
                let mut positionals: Vec<String> = Vec::new();
                for a in rest {
                    if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    }
                    positionals.push(a.clone());
                }
                if positionals.is_empty() || positionals.len() > 2 {
                    return Err("`vendor` takes a source tree and an optional path".to_string());
                }
                Ok(Cmd::Vendor {
                    source: positionals[0].clone(),
                    path: positionals.get(1).cloned().unwrap_or_else(default_path),
                })
            }
            "foreign" => {
                // `foreign check [space] [path]` (§9.10): prove every declared
                // foreign name is reachable before a user's request finds out.
                let mut positionals: Vec<String> = Vec::new();
                for a in rest {
                    if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    }
                    positionals.push(a.clone());
                }
                match positionals.first().map(|s| s.as_str()) {
                    Some("check") => {}
                    Some(other) => return Err(format!("unknown foreign command `{}`", other)),
                    None => return Err("`foreign` takes a command: `check`".to_string()),
                }
                let rest = &positionals[1..];
                // One argument is a path if it names a directory, else a space.
                let (space, path) = match rest.len() {
                    0 => (None, default_path()),
                    1 => {
                        let a = rest[0].clone();
                        if Path::new(&a).is_dir() {
                            (None, a)
                        } else {
                            (Some(a), default_path())
                        }
                    }
                    2 => (Some(rest[0].clone()), rest[1].clone()),
                    _ => return Err("`foreign check` takes an optional space and path".to_string()),
                };
                Ok(Cmd::ForeignCheck { space, path })
            }
            "run" => {
                // `run [part] [path] [--port n]` (reference §9.1, §11): one
                // positional is a part name unless it names a directory on
                // disk; two are part then path. `--port` overrides the
                // program's `port` at run time — a deployment fact, never
                // written in source (B5), so the same project can serve on
                // any port without editing a line.
                let mut positionals: Vec<String> = Vec::new();
                let mut port: Option<u16> = None;
                let mut mesh: Option<String> = None;
                let mut it = rest.iter().peekable();
                while let Some(a) = it.next() {
                    if a == "--port" {
                        let v = it
                            .next()
                            .ok_or_else(|| "`--port` needs a number".to_string())?;
                        let n: u16 = v
                            .parse()
                            .map_err(|_| format!("`--port` takes a number, not `{}`", v))?;
                        port = Some(n);
                    } else if a == "--mesh" {
                        // The name is optional: `--mesh` alone takes the
                        // machine's default mesh, `--mesh home` names one. A
                        // following `--flag` is the next flag, and a token
                        // naming a directory is the project path — the same
                        // rule this command already uses to tell a part name
                        // from a path.
                        let named = match it.peek() {
                            Some(v) if !v.starts_with("--") && !Path::new(v.as_str()).is_dir() => {
                                Some((*v).clone())
                            }
                            _ => None,
                        };
                        if named.is_some() {
                            it.next();
                        }
                        mesh = Some(named.unwrap_or_default());
                    } else if a.starts_with("--") {
                        return Err(format!("unknown flag `{}`", a));
                    } else {
                        positionals.push(a.clone());
                    }
                }
                let (path, part) = match positionals.len() {
                    0 => (default_path(), None),
                    1 => {
                        let a = positionals[0].clone();
                        if Path::new(&a).is_dir() {
                            (a, None)
                        } else {
                            (default_path(), Some(a))
                        }
                    }
                    2 => (positionals[1].clone(), Some(positionals[0].clone())),
                    _ => {
                        return Err(
                            "`run` takes an optional part and an optional path".to_string()
                        )
                    }
                };
                Ok(Cmd::Run { path, part, port, mesh })
            }
            "mesh" => Ok(Cmd::Mesh {
                path: one_path(rest)?,
            }),
            "fmt" => {
                let mut path: Option<String> = None;
                let mut check_only = false;
                for a in rest {
                    if a == "--check" {
                        if check_only {
                            return Err("`--check` given twice".to_string());
                        }
                        check_only = true;
                    } else if let Some(p) = positional(a, &path)? {
                        path = Some(p);
                    }
                }
                Ok(Cmd::Fmt {
                    path: path.unwrap_or_else(default_path),
                    check_only,
                })
            }
            other => Err(format!("unknown command `{}`", other)),
        }
    }

    fn default_path() -> String {
        ".".to_string()
    }

    /// A command with no flags at all: at most one positional path argument.
    fn one_path(rest: &[String]) -> Result<String, String> {
        let mut path: Option<String> = None;
        for a in rest {
            if let Some(p) = positional(a, &path)? {
                path = Some(p);
            }
        }
        Ok(path.unwrap_or_else(default_path))
    }

    /// Accept `a` as the (only) positional path argument, or reject it as
    /// an unknown flag / surplus argument.
    fn positional(a: &str, path_so_far: &Option<String>) -> Result<Option<String>, String> {
        if a.starts_with("--") {
            Err(format!("unknown flag `{}`", a))
        } else if path_so_far.is_some() {
            Err("too many arguments".to_string())
        } else {
            Ok(Some(a.to_string()))
        }
    }

    /// Run a parsed command; returns the process exit code.
    pub fn run(cmd: Cmd) -> i32 {
        match cmd {
            Cmd::Check { path, human } => run_check(&path, human),
            Cmd::Fix { path, id } => run_fix(&path, id.as_deref()),
            Cmd::Build { path } => run_build(&path),
            Cmd::Fmt { path, check_only } => run_fmt(&path, check_only),
            Cmd::Run { path, part, port, mesh } => run_serve(&path, part, port, mesh),
            Cmd::Mesh { path } => run_mesh(&path),
            Cmd::Rename { target, new_name, path, plan_only } => {
                run_refactor(&path, plan_only, |srcs| plan_rename(srcs, &target, &new_name))
            }
            Cmd::Rekind { target, kind, path, plan_only } => {
                run_refactor(&path, plan_only, |srcs| match target.rsplit_once('.') {
                    Some((part, prop)) => ashlar::refactor::plan_rekind(srcs, part, prop, &kind),
                    None => Err(ashlar::refactor::Refusal(
                        "rekind takes `<part>.<property>`.".to_string(),
                    )),
                })
            }
            Cmd::Move { part, space, path, plan_only } => {
                run_refactor(&path, plan_only, |srcs| {
                    ashlar::refactor::plan_move(srcs, &part, &space)
                })
            }
            Cmd::Radius { target, path } => run_radius(&path, &target),
        Cmd::Delta { path } => run_delta(&path),
            Cmd::Vendor { source, path } => run_vendor(&path, &source),
            Cmd::ForeignCheck { space, path } => run_foreign_check(&path, space.as_deref()),
        }
    }

    /// `rename`'s target resolution (reference §11: a space, part, or
    /// property). A name that is both a part and a space refuses as
    /// ambiguous rather than guessing.
    fn plan_rename(
        srcs: &[(String, String)],
        target: &str,
        new_name: &str,
    ) -> Result<ashlar::refactor::Plan, ashlar::refactor::Refusal> {
        let checked = ashlar::check_sources(srcs.to_vec());
        let is_part = checked.program.parts.contains_key(target);
        let is_space = checked.program.spaces.contains_key(target);
        if is_part && is_space {
            return Err(ashlar::refactor::Refusal(format!(
                "`{}` names both a part and a space; rename one of them out of the collision first.",
                target
            )));
        }
        if is_part {
            ashlar::refactor::plan_rename_part(srcs, new_name, target)
        } else if is_space {
            ashlar::refactor::plan_rename_space(srcs, target, new_name)
        } else if let Some((part, prop)) = target.rsplit_once('.') {
            ashlar::refactor::plan_rename_prop(srcs, part, prop, new_name)
        } else {
            Err(ashlar::refactor::Refusal(format!(
                "`{}` names neither a space, a part, nor a part.property.",
                target
            )))
        }
    }

    /// `ashlar radius <full-name>` (reference §11): print every location a
    /// rename of the name would touch, touching nothing. Implemented as
    /// the real plan against a fresh probe name, so the printed radius is
    /// exactly the rename's radius — same code path, no drift.
    /// `ashlar delta` — what the working tree changed about the program's
    /// derived state since the last `ashlar build` (C9).
    ///
    /// The question `check` cannot answer on its own: an edit that resequences
    /// composition order is deterministic, correct, and invisible, so W002
    /// flags it and this prints it in full. Touches nothing.
    fn run_delta(path: &str) -> i32 {
        let root = Path::new(path);
        let Some(base) = ashlar::delta::load(root) else {
            println!(
                "no baseline: `ashlar.manifest` is absent or unreadable, so there is nothing to \
                 compare against. Run `ashlar build` to record one."
            );
            return 0;
        };
        let result = ashlar::check_project(root);
        if result.has_errors() {
            eprintln!("the program does not compile; `ashlar check` first.");
            print_diags(&result.diags, false);
            return 1;
        }
        let changes = ashlar::delta::order_changes(&base, &result.program);
        if changes.is_empty() {
            println!("no change to composition order since the last build.");
            return 0;
        }
        println!("composition order changed: {} part(s)", changes.len());
        for c in &changes {
            println!("  {}", c.part);
            println!("    before  {}", c.before.join(" -> "));
            println!("    after   {}", c.after.join(" -> "));
        }
        println!(
            "\nEvery `stack`, `pipe`, `append` and `deep` property on those parts composes in the \
             new order. Nothing is broken; this is the report, not a refusal."
        );
        0
    }

    fn run_radius(path: &str, target: &str) -> i32 {
        let root = Path::new(path);
        let mut sources: Vec<(String, String)> = Vec::new();
        for file in ashlar::find_ash_files(root) {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            match std::fs::read_to_string(&file) {
                Ok(s) => sources.push((rel, s)),
                Err(e) => {
                    eprintln!("error reading {}: {}", rel, e);
                    return 1;
                }
            }
        }
        let checked = ashlar::check_sources(sources.clone());
        let mut probe = "radius_probe".to_string();
        let taken = |name: &str| {
            checked.program.parts.keys().any(|p| {
                p.rsplit('.').next() == Some(name) || p == name
            }) || checked.program.spaces.contains_key(name)
                || checked
                    .composed
                    .values()
                    .any(|cp| cp.props.contains_key(name))
        };
        while taken(&probe) {
            probe.push('_');
        }
        match plan_rename(&sources, target, &probe) {
            Ok(plan) => {
                println!("radius of `{}`: {} site(s)", target, plan.changes.len());
                for c in &plan.changes {
                    println!(
                        "  {}:{}:{}  `{}`",
                        c.file, c.span.start.line, c.span.start.col, c.old
                    );
                }
                // A rename's radius includes what lives OUTSIDE sources:
                // stored keys and foreign host libraries.
                for (old, _) in &plan.state_part_renames {
                    println!("  .ashlar-state.json  keys under `{}`", old);
                }
                for (old, _) in &plan.state_prop_renames {
                    println!("  .ashlar-state.json  `{}`", old);
                }
                for (old, _) in &plan.foreign_renames {
                    println!("  {}", old);
                }
                if let Some((old, _)) = &plan.foreign_key_rename {
                    if let Some(file) = binding_file_with_key(root, old) {
                        println!("  {}  key `{}`", file, old);
                    }
                }
                for note in &plan.notes {
                    println!("  note: {}", note);
                }
                0
            }
            Err(ashlar::refactor::Refusal(reason)) => {
                eprintln!("cannot compute the radius: {}", reason);
                1
            }
        }
    }

    /// `ashlar vendor <source>` (reference §11, G-series: no registry —
    /// dependencies are code vendored into the tree). Copies the source
    /// tree's `.ash` files under `vendor/<name>/`, refuses on space
    /// collisions BEFORE copying, and rolls the copy back entirely if the
    /// combined project does not check clean.
    fn run_vendor(path: &str, source: &str) -> i32 {
        let root = Path::new(path);
        let src_root = Path::new(source);
        if !src_root.is_dir() {
            eprintln!("refused: `{}` is not a directory.", source);
            return 1;
        }
        let name = src_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "vendored".to_string());
        let vendor_dir = root.join("vendor");
        let vendor_dir_existed = vendor_dir.exists();
        let dest = vendor_dir.join(&name);
        if dest.exists() {
            eprintln!(
                "refused: `vendor/{}` already exists; remove it to re-vendor.",
                name
            );
            return 1;
        }
        let files = ashlar::find_ash_files(src_root);
        if files.is_empty() {
            eprintln!("refused: `{}` contains no .ash files.", source);
            return 1;
        }
        // Space collision check before anything is written.
        let mut incoming: Vec<(String, String)> = Vec::new();
        for f in &files {
            let rel = f
                .strip_prefix(src_root)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/");
            match std::fs::read_to_string(f) {
                Ok(s) => incoming.push((rel, s)),
                Err(e) => {
                    eprintln!("error reading {}: {}", f.display(), e);
                    return 1;
                }
            }
        }
        let theirs = ashlar::check_sources(incoming.clone());
        let ours = ashlar::check_project(root);
        let collisions: Vec<&String> = theirs
            .program
            .spaces
            .keys()
            .filter(|s| ours.program.spaces.contains_key(*s))
            .collect();
        if !collisions.is_empty() {
            eprintln!(
                "refused: the tree declares space(s) this project already has: {}.",
                collisions
                    .iter()
                    .map(|s| format!("`{}`", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return 1;
        }
        // Copy, then verify the combined project; errors roll it all back.
        for (rel, text) in &incoming {
            let target = dest.join(rel);
            if let Some(dir) = target.parent() {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("error creating {}: {}", dir.display(), e);
                    let _ = std::fs::remove_dir_all(&dest);
                    if !vendor_dir_existed {
                        let _ = std::fs::remove_dir(&vendor_dir);
                    }
                    return 1;
                }
            }
            if let Err(e) = std::fs::write(&target, text) {
                eprintln!("error writing {}: {}", target.display(), e);
                let _ = std::fs::remove_dir_all(&dest);
                if !vendor_dir_existed {
                    let _ = std::fs::remove_dir(&vendor_dir);
                }
                return 1;
            }
        }
        let combined = ashlar::check_project(root);
        if combined.has_errors() {
            print_diags(&combined.diags, false);
            let rollback = std::fs::remove_dir_all(&dest);
            if !vendor_dir_existed {
                let _ = std::fs::remove_dir(&vendor_dir);
            }
            match rollback {
                Ok(()) => eprintln!(
                    "refused: the combined project does not check; `vendor/{}` rolled back.",
                    name
                ),
                Err(e) => eprintln!(
                    "refused: the combined project does not check — and rolling back `vendor/{}` FAILED ({}); remove it by hand.",
                    name, e
                ),
            }
            return 1;
        }
        eprintln!(
            "vendored {} file(s) into vendor/{} (spaces: {})",
            incoming.len(),
            name,
            theirs
                .program
                .spaces
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        0
    }

    /// Migrate `.ashlar-state.json` keys after a rename/move touching
    /// `stored` properties (ADR-0007's orphaned-rows note, closed). The
    /// file is a flat map of `space.Part.prop` keys (plus `__users`);
    /// migration rewrites keys and writes atomically via a temp file.
    fn migrate_state(root: &Path, plan: &ashlar::refactor::Plan) -> Result<usize, String> {
        if plan.state_part_renames.is_empty() && plan.state_prop_renames.is_empty() {
            return Ok(0);
        }
        let state_path = root.join(".ashlar-state.json");
        let text = match std::fs::read_to_string(&state_path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(0); // no state file, nothing to migrate
            }
            Err(e) => {
                return Err(format!(
                    "{} exists but could not be read: {}.",
                    state_path.display(),
                    e
                ));
            }
        };
        let Some(ashlar::eval::V::Map(m)) = ashlar::eval::from_json(&text) else {
            return Err(format!(
                "{} is not a JSON object; refusing to migrate it.",
                state_path.display()
            ));
        };
        let mut migrated = 0usize;
        let mut out: std::collections::BTreeMap<String, ashlar::eval::V> =
            std::collections::BTreeMap::new();
        'keys: for (k, v) in m {
            for (old, new) in &plan.state_prop_renames {
                if &k == old {
                    out.insert(new.clone(), v);
                    migrated += 1;
                    continue 'keys;
                }
            }
            for (old, new) in &plan.state_part_renames {
                if let Some((part, prop)) = k.rsplit_once('.') {
                    if part == old {
                        out.insert(format!("{}.{}", new, prop), v);
                        migrated += 1;
                        continue 'keys;
                    }
                }
            }
            out.insert(k, v);
        }
        if migrated > 0 {
            let tmp = root.join(".ashlar-state.json.tmp");
            let rendered = ashlar::eval::to_json(&ashlar::eval::V::Map(out));
            std::fs::write(&tmp, rendered).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &state_path).map_err(|e| e.to_string())?;
        }
        Ok(migrated)
    }

    /// Shared refactor driver: load sources, plan, report the blast
    /// radius (E3), then apply-and-verify unless `--plan`.
    fn run_refactor(
        path: &str,
        plan_only: bool,
        plan_fn: impl FnOnce(&[(String, String)]) -> Result<ashlar::refactor::Plan, ashlar::refactor::Refusal>,
    ) -> i32 {
        let root = Path::new(path);
        let mut sources: Vec<(String, String)> = Vec::new();
        for file in ashlar::find_ash_files(root) {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            match std::fs::read_to_string(&file) {
                Ok(s) => sources.push((rel, s)),
                Err(e) => {
                    eprintln!("error reading {}: {}", rel, e);
                    return 1;
                }
            }
        }
        let plan = match plan_fn(&sources) {
            Ok(p) => p,
            Err(ashlar::refactor::Refusal(reason)) => {
                eprintln!("refused: {}", reason);
                return 1;
            }
        };
        eprintln!("{}: {} change(s)", plan.description, plan.changes.len());
        for c in &plan.changes {
            eprintln!(
                "  {}:{}:{}  `{}` -> `{}`",
                c.file, c.span.start.line, c.span.start.col, c.old, c.new
            );
        }
        // Stored state, foreign libraries, and the foreign binding key are all
        // radius too (E3).
        print_side_effects(root, &plan);
        if plan_only {
            return 0;
        }
        match ashlar::refactor::execute(&sources, &plan) {
            Ok(after) => {
                let changed: Vec<(&String, &String)> = after
                    .iter()
                    .filter(|(rel, text)| {
                        sources.iter().find(|(p, _)| p == *rel).map(|(_, s)| s) != Some(text)
                    })
                    .collect();
                if let Err(e) = write_all_or_restore(root, &sources, &changed) {
                    eprintln!("{}", e);
                    return 1;
                }
                for (rel, _) in &changed {
                    eprintln!("rewrote: {}", rel);
                }
                match migrate_state(root, &plan) {
                    Ok(0) => {}
                    Ok(n) => eprintln!("migrated {} stored key(s) in .ashlar-state.json", n),
                    Err(e) => {
                        eprintln!("warning: state migration failed: {}", e);
                        eprintln!("sources are consistent; fix the state file by hand.");
                    }
                }
                // The derivation rule names one library per platform
                // extension; at most one of them is really on disk, so report
                // the moves and say "none" once rather than per extension.
                let mut moved = 0usize;
                for (old, new) in &plan.foreign_renames {
                    let (from, to) = (root.join(old), root.join(new));
                    if !from.exists() {
                        continue;
                    }
                    match std::fs::rename(&from, &to) {
                        Ok(()) => {
                            moved += 1;
                            eprintln!("moved: {} -> {}", old, new);
                        }
                        Err(e) => {
                            eprintln!("warning: could not move {}: {}", old, e);
                            eprintln!("move it by hand or the runtime will not find it.");
                        }
                    }
                }
                if moved == 0 && !plan.foreign_renames.is_empty() {
                    eprintln!("note: no derived foreign library on disk; nothing to move.");
                }
                match migrate_binding_key(root, &plan) {
                    Ok(false) => {}
                    Ok(true) => {
                        let (old, new) = plan
                            .foreign_key_rename
                            .as_ref()
                            .expect("rewritten only when set");
                        eprintln!("rebound: foreign key `{}` -> `{}`", old, new);
                    }
                    Err(e) => {
                        eprintln!("warning: could not rewrite the foreign binding key: {}", e);
                        eprintln!("rewrite it by hand or the space falls back to the derived library path.");
                    }
                }
                0
            }
            Err(ashlar::refactor::Refusal(reason)) => {
                eprintln!("refused: {}", reason);
                1
            }
        }
    }

    /// The non-source effects a plan carries, printed with the radius.
    fn print_side_effects(root: &Path, plan: &ashlar::refactor::Plan) {
        for (old, new) in &plan.state_part_renames {
            eprintln!("  .ashlar-state.json  `{}.*` -> `{}.*`", old, new);
        }
        for (old, new) in &plan.state_prop_renames {
            eprintln!("  .ashlar-state.json  `{}` -> `{}`", old, new);
        }
        for (old, new) in &plan.foreign_renames {
            eprintln!("  {} -> {}", old, new);
        }
        for note in &plan.notes {
            eprintln!("  note: {}", note);
        }
        if let Some((old, new)) = &plan.foreign_key_rename {
            if let Some(file) = binding_file_with_key(root, old) {
                eprintln!("  {}  key `{}` -> `{}`", file, old, new);
            }
        }
    }

    /// The binding file's display name, but only when it actually carries this
    /// space key — the radius names what is really there, never a phantom.
    fn binding_file_with_key(root: &Path, key: &str) -> Option<String> {
        let path = ashlar::foreign::binding_path(root);
        let text = std::fs::read_to_string(&path).ok()?;
        ashlar::foreign::rename_key(&text, key, key)?;
        Some(
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "foreign.json".to_string()),
        )
    }

    /// Carry a space rename into the binding file. The key is a name the
    /// program binds by, so leaving it behind would silently unbind the
    /// capability (E2) — and rewriting only the key keeps the file
    /// byte-identical when the rename is reversed (E4).
    fn migrate_binding_key(root: &Path, plan: &ashlar::refactor::Plan) -> Result<bool, String> {
        let Some((old, new)) = &plan.foreign_key_rename else {
            return Ok(false);
        };
        let path = ashlar::foreign::binding_path(root);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(false);
        };
        let Some(rewritten) = ashlar::foreign::rename_key(&text, old, new) else {
            return Ok(false);
        };
        std::fs::write(&path, rewritten).map_err(|e| format!("{}: {}", path.display(), e))?;
        Ok(true)
    }

    /// Two-phase source writes: stage every changed file to a temp sibling,
    /// then rename all into place. A staging failure aborts with nothing
    /// touched; a rename failure restores the originals already replaced,
    /// so the tree is never left half-rewritten (E4).
    fn write_all_or_restore(
        root: &Path,
        sources: &[(String, String)],
        changed: &[(&String, &String)],
    ) -> Result<(), String> {
        let mut staged: Vec<(String, std::path::PathBuf)> = Vec::new();
        for (rel, text) in changed {
            let tmp = root.join(format!("{}.ashtmp", rel));
            if let Err(e) = std::fs::write(&tmp, text) {
                for (_, t) in &staged {
                    let _ = std::fs::remove_file(t);
                }
                let _ = std::fs::remove_file(&tmp);
                return Err(format!(
                    "error staging {}: {} — nothing was changed.",
                    rel, e
                ));
            }
            staged.push(((*rel).clone(), tmp));
        }
        let mut replaced: Vec<String> = Vec::new();
        for (rel, tmp) in &staged {
            if let Err(e) = std::fs::rename(tmp, root.join(rel)) {
                // Restore what was already swapped from the in-memory
                // originals, and clear remaining temps.
                for done in &replaced {
                    if let Some((_, orig)) = sources.iter().find(|(p, _)| p == done) {
                        let _ = std::fs::write(root.join(done), orig);
                    }
                }
                for (_, t) in &staged {
                    let _ = std::fs::remove_file(t);
                }
                return Err(format!(
                    "error writing {}: {} — originals restored, nothing changed.",
                    rel, e
                ));
            }
            replaced.push(rel.clone());
        }
        Ok(())
    }

    /// `ashlar run [part] [--port n] [--mesh [network]]` (§9.1, §11).
    ///
    /// `--mesh` publishes the port this origin is serving to a private mesh,
    /// so the people on it reach the site with no proxy, no forwarded port
    /// and no public address (ADR-0013). It is `--port`'s sibling: `--port`
    /// says where this origin listens, `--mesh` says who can reach it, and
    /// neither is written in source (B5).
    ///
    /// Publishing failing does NOT stop the server. A site that is up and
    /// unpublished is a site; a process that refused to serve because a
    /// daemon was missing is an outage, and the reason is one line above the
    /// address either way.
    fn run_serve(
        path: &str,
        part: Option<String>,
        port: Option<u16>,
        mesh: Option<String>,
    ) -> i32 {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let root = std::path::PathBuf::from(path);
        // One link for the whole run: the co-process that published the site
        // is the one asked to take it back down.
        let link: std::sync::Arc<std::sync::Mutex<ashlar::mesh::Link>> =
            std::sync::Arc::new(std::sync::Mutex::new(ashlar::mesh::Link::new()));
        let published: std::sync::Arc<std::sync::Mutex<Option<u16>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let on_bind = {
            let root = root.clone();
            let mesh = mesh.clone();
            let link = link.clone();
            let published = published.clone();
            move |port: u16, root_part: &str| {
                if let Some(network) = &mesh {
                    let mut link = link.lock().expect("not poisoned");
                    match link.publish(&root, port, network, root_part) {
                        Ok(p) => {
                            *published.lock().expect("not poisoned") = Some(port);
                            eprintln!("{}", p.line());
                        }
                        Err(e) => {
                            eprintln!("not published: {}", e);
                            eprintln!("`ashlar mesh` reports what this machine's mesh can do.");
                        }
                    }
                }
                eprintln!("serving {} on http://127.0.0.1:{}", root_part, port);
            }
        };
        let outcome = ashlar::http::serve(root.clone(), part, port, on_bind, stop);
        // Leaving the mesh is the counterpart of joining it, and it runs on
        // the way out of a clean shutdown and a failed one alike: the daemon
        // outlives this process, so a site left published is one a peer can
        // still see and never reach.
        let bound = *published.lock().expect("not poisoned");
        if let Some(port) = bound {
            let mut link = link.lock().expect("not poisoned");
            if let Err(e) = link.withdraw(&root, port) {
                eprintln!("warning: could not take the site off the mesh: {}", e);
            }
        }
        match outcome {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{}", e);
                1
            }
        }
    }

    /// `ashlar mesh` (§11): what this machine's mesh can answer, before a
    /// request finds out. The sibling of `ashlar foreign check` — the mesh is
    /// reached across the same boundary, so its reachability is proven the
    /// same way, on demand, against the bindings in force.
    fn run_mesh(path: &str) -> i32 {
        let report = ashlar::mesh::Link::new().report(Path::new(path));
        for (name, value) in &report.facts {
            println!("{:<10} {}", name, value);
        }
        for problem in &report.problems {
            println!("unreachable {}", problem);
        }
        if report.ok() {
            0
        } else {
            1
        }
    }

    /// `ashlar foreign check [space]` (§9.10, ADR-0017): prove every declared
    /// foreign name is actually reachable — dlsym each native symbol, or make
    /// the worker speak the protocol — so an unreachable capability is a
    /// build-time correction instead of a fault a user's request discovers.
    fn run_foreign_check(path: &str, only: Option<&str>) -> i32 {
        let root = Path::new(path);
        let result = ashlar::check_project(root);
        if result.has_errors() {
            eprintln!("the project does not compile; fix that first:");
            print_diags(&result.diags, true);
            return 1;
        }

        // Declared foreign names, grouped by the space that owns them.
        let mut by_space: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for info in result.program.foreigns.values() {
            let name = result.program.files[info.file_idx].ast.foreigns[info.foreign_idx]
                .name
                .clone();
            by_space.entry(info.space.clone()).or_default().push(name);
        }
        if let Some(space) = only {
            by_space.retain(|s, _| s == space);
            if by_space.is_empty() {
                eprintln!("`{}` declares no foreign functions.", space);
                return 1;
            }
        }
        if by_space.is_empty() {
            println!("no foreign declarations in this project.");
            return 0;
        }

        let mut failed = false;
        for (space, names) in &by_space {
            let reach = ashlar::foreign::check_space(root, space, names);
            let where_ = if reach.detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", reach.detail)
            };
            if reach.problems.is_empty() {
                println!(
                    "ok   {} via {}{} — {} name(s) reachable",
                    space,
                    reach.via,
                    where_,
                    names.len()
                );
            } else {
                failed = true;
                println!("FAIL {} via {}{}", space, reach.via, where_);
                for p in &reach.problems {
                    println!("       {}", p);
                }
            }
        }
        if failed {
            1
        } else {
            0
        }
    }

    fn run_fmt(path: &str, check_only: bool) -> i32 {
        let root = Path::new(path);
        let mut changed = 0usize;
        let mut broken = 0usize;
        for file in ashlar::find_ash_files(root) {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let src = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error reading {}: {}", rel, e);
                    return 1;
                }
            };
            match ashlar::fmt::format_source(&rel, &src) {
                Ok(formatted) if formatted != src => {
                    changed += 1;
                    if check_only {
                        println!("would format: {}", rel);
                    } else {
                        if let Err(e) = std::fs::write(&file, &formatted) {
                            eprintln!("error writing {}: {}", rel, e);
                            return 1;
                        }
                        eprintln!("formatted: {}", rel);
                    }
                }
                Ok(_) => {}
                Err(diags) => {
                    // Broken files are never rewritten; their diagnostics
                    // come from `check`, not `fmt`.
                    broken += 1;
                    eprintln!("skipping {} ({} diagnostic(s); run `ashlar check`)", rel, diags.len());
                }
            }
        }
        if check_only && changed > 0 {
            1
        } else if broken > 0 {
            1
        } else {
            0
        }
    }

    fn print_diags(diags: &[Diag], human: bool) {
        for d in diags {
            if human {
                println!("{}", d.human());
            } else {
                println!("{}", d.jsonl());
            }
        }
    }

    fn run_check(path: &str, human: bool) -> i32 {
        let result = ashlar::check_project(Path::new(path));
        print_diags(&result.diags, human);
        if result.has_errors() {
            1
        } else {
            0
        }
    }

    fn run_fix(path: &str, id: Option<&str>) -> i32 {
        let root = Path::new(path);
        let result = ashlar::check_project(root);

        // `fix E006` applies only that id's machine edits (§11).
        let diags: Vec<ashlar::diag::Diag> = match id {
            Some(want) => result.diags.iter().filter(|d| d.id == want).cloned().collect(),
            None => result.diags.clone(),
        };
        match ashlar::fixup::apply_fixes(root, &diags) {
            Ok(files) => {
                for f in &files {
                    eprintln!("fixed: {}", f);
                }
            }
            Err(e) => {
                eprintln!("error applying fixes: {}", e);
                return 1;
            }
        }

        let after = ashlar::check_project(root);
        print_diags(&after.diags, false);
        if after.has_errors() {
            1
        } else {
            0
        }
    }

    fn run_build(path: &str) -> i32 {
        let root = Path::new(path);
        let result = ashlar::check_project(root);
        if result.has_errors() {
            print_diags(&result.diags, false);
            return 1;
        }

        let text = ashlar::manifest::render(&result.program, &result.composed, root);
        match std::fs::write(root.join("ashlar.manifest"), text) {
            Ok(()) => {
                eprintln!("wrote ashlar.manifest");
                0
            }
            Err(e) => {
                eprintln!("error writing ashlar.manifest: {}", e);
                1
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn args(v: &[&str]) -> Vec<String> {
            v.iter().map(|s| s.to_string()).collect()
        }

        #[test]
        fn check_defaults_path_and_human() {
            let cmd = parse(&args(&["check"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Check {
                    path: ".".to_string(),
                    human: false
                }
            );
        }

        #[test]
        fn check_with_path_and_human_either_order() {
            let a = parse(&args(&["check", "proj", "--human"])).unwrap();
            let b = parse(&args(&["check", "--human", "proj"])).unwrap();
            let want = Cmd::Check {
                path: "proj".to_string(),
                human: true,
            };
            assert_eq!(a, want);
            assert_eq!(b, want);
        }

        #[test]
        fn check_human_only_no_path() {
            let cmd = parse(&args(&["check", "--human"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Check {
                    path: ".".to_string(),
                    human: true
                }
            );
        }

        #[test]
        fn fix_defaults_path() {
            let cmd = parse(&args(&["fix"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Fix {
                    path: ".".to_string(),
                    id: None
                }
            );
        }

        #[test]
        fn fix_with_path() {
            let cmd = parse(&args(&["fix", "some/proj"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Fix {
                    path: "some/proj".to_string(),
                    id: None
                }
            );
        }

        #[test]
        fn build_defaults_path() {
            let cmd = parse(&args(&["build"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Build {
                    path: ".".to_string()
                }
            );
        }

        #[test]
        fn build_with_path() {
            let cmd = parse(&args(&["build", "there"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Build {
                    path: "there".to_string()
                }
            );
        }

        #[test]
        fn no_args_is_an_error() {
            assert!(parse(&args(&[])).is_err());
        }

        #[test]
        fn unknown_command_is_an_error() {
            assert!(parse(&args(&["frobnicate"])).is_err());
        }

        #[test]
        fn human_flag_rejected_on_fix() {
            assert!(parse(&args(&["fix", "--human"])).is_err());
        }

        #[test]
        fn human_flag_rejected_on_build() {
            assert!(parse(&args(&["build", "--human"])).is_err());
        }

        #[test]
        fn unknown_flag_is_an_error() {
            assert!(parse(&args(&["check", "--verbose"])).is_err());
        }

        #[test]
        fn duplicate_human_flag_is_an_error() {
            assert!(parse(&args(&["check", "--human", "--human"])).is_err());
        }

        #[test]
        fn two_positional_paths_is_an_error() {
            assert!(parse(&args(&["check", "a", "b"])).is_err());
        }

        #[test]
        fn fix_takes_an_optional_id() {
            let cmd = parse(&args(&["fix", "E006"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Fix {
                    path: ".".to_string(),
                    id: Some("E006".to_string())
                }
            );
            let cmd = parse(&args(&["fix", "W001", "proj"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Fix {
                    path: "proj".to_string(),
                    id: Some("W001".to_string())
                }
            );
        }

        #[test]
        fn run_takes_an_optional_part() {
            // A non-directory argument is a part name (§9.1).
            let cmd = parse(&args(&["run", "chat.app"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Run {
                    path: ".".to_string(),
                    part: Some("chat.app".to_string()),
                    port: None,
                    mesh: None
                }
            );
            let cmd = parse(&args(&["run", "chat.app", "."])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Run {
                    path: ".".to_string(),
                    part: Some("chat.app".to_string()),
                    port: None,
                    mesh: None
                }
            );
        }

        #[test]
        fn run_takes_a_port_override() {
            // `--port` is a deployment fact bound at run time (B5): the
            // source keeps its `port`, the flag overrides where it serves.
            let cmd = parse(&args(&["run", "app", "somewhere", "--port", "8091"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Run {
                    path: "somewhere".to_string(),
                    part: Some("app".to_string()),
                    port: Some(8091),
                    mesh: None
                }
            );
            // A bare `--port` with a value and no positionals is fine too.
            let cmd = parse(&args(&["run", "--port", "9000"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Run { path: ".".to_string(), part: None, port: Some(9000), mesh: None }
            );
        }

        #[test]
        fn run_publishes_to_a_mesh_named_or_default() {
            // covers: B5
            // `--mesh` alone is the machine's default mesh; a following word
            // names one. Both are deployment facts, exactly like `--port`.
            assert_eq!(
                parse(&args(&["run", "--mesh"])).unwrap(),
                Cmd::Run {
                    path: ".".to_string(),
                    part: None,
                    port: None,
                    mesh: Some(String::new())
                }
            );
            assert_eq!(
                parse(&args(&["run", "--mesh", "enclave"])).unwrap(),
                Cmd::Run {
                    path: ".".to_string(),
                    part: None,
                    port: None,
                    mesh: Some("enclave".to_string())
                }
            );
            // A flag after `--mesh` is the next flag, not the mesh's name.
            assert_eq!(
                parse(&args(&["run", "--mesh", "--port", "8091"])).unwrap(),
                Cmd::Run {
                    path: ".".to_string(),
                    part: None,
                    port: Some(8091),
                    mesh: Some(String::new())
                }
            );
            // And a directory after it is the project path — the same rule
            // that tells `run <part>` from `run <path>`.
            let here = std::env::temp_dir().to_string_lossy().to_string();
            assert_eq!(
                parse(&args(&["run", "--mesh", &here])).unwrap(),
                Cmd::Run {
                    path: here,
                    part: None,
                    port: None,
                    mesh: Some(String::new())
                }
            );
            // Absent means unpublished: a site does not join a mesh by
            // running.
            assert!(matches!(
                parse(&args(&["run"])).unwrap(),
                Cmd::Run { mesh: None, .. }
            ));
        }

        #[test]
        fn mesh_takes_an_optional_path() {
            assert_eq!(
                parse(&args(&["mesh"])).unwrap(),
                Cmd::Mesh { path: ".".to_string() }
            );
            assert_eq!(
                parse(&args(&["mesh", "proj"])).unwrap(),
                Cmd::Mesh { path: "proj".to_string() }
            );
        }

        #[test]
        fn foreign_check_parses_space_and_path() {
            // `foreign check` with no arguments checks every space here.
            assert_eq!(
                parse(&args(&["foreign", "check"])).unwrap(),
                Cmd::ForeignCheck { space: None, path: ".".to_string() }
            );
            // One non-directory argument is a space name, not a path.
            assert_eq!(
                parse(&args(&["foreign", "check", "ledger.store"])).unwrap(),
                Cmd::ForeignCheck {
                    space: Some("ledger.store".to_string()),
                    path: ".".to_string()
                }
            );
            assert_eq!(
                parse(&args(&["foreign", "check", "ledger.store", "proj"])).unwrap(),
                Cmd::ForeignCheck {
                    space: Some("ledger.store".to_string()),
                    path: "proj".to_string()
                }
            );
            // `foreign` alone, or with an unknown sub-command, is a usage error.
            assert!(parse(&args(&["foreign"])).is_err());
            assert!(parse(&args(&["foreign", "repair"])).is_err());
        }

        #[test]
        fn run_rejects_a_bad_port() {
            assert!(parse(&args(&["run", "--port", "notaport"])).is_err());
            assert!(parse(&args(&["run", "--port"])).is_err());
        }

        #[test]
        fn move_parses_with_plan_flag() {
            let cmd = parse(&args(&["move", "a.Two", "b", "--plan"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Move {
                    part: "a.Two".to_string(),
                    space: "b".to_string(),
                    path: ".".to_string(),
                    plan_only: true
                }
            );
        }

        #[test]
        fn radius_takes_a_name_and_optional_path() {
            let cmd = parse(&args(&["radius", "a.W"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Radius {
                    target: "a.W".to_string(),
                    path: ".".to_string()
                }
            );
            assert!(parse(&args(&["radius"])).is_err());
            assert!(parse(&args(&["radius", "a", "b", "c"])).is_err());
        }

        #[test]
        fn vendor_takes_a_source_and_optional_path() {
            let cmd = parse(&args(&["vendor", "../lib", "proj"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Vendor {
                    source: "../lib".to_string(),
                    path: "proj".to_string()
                }
            );
            assert!(parse(&args(&["vendor"])).is_err());
        }

        // `run` itself is deliberately not unit-tested here: it calls
        // through to `ashlar::check_project`, which depends on the lexer,
        // parser, resolver, and composer — modules owned by other agents
        // and not this file's concern. `parse` is the pure, contract-owned
        // surface; `run`'s wiring is exercised end-to-end once the rest of
        // the pipeline exists.
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Ok(cmd) => std::process::exit(cli::run(cmd)),
        Err(_) => {
            eprint!("{}", cli::USAGE);
            std::process::exit(2);
        }
    }
}
