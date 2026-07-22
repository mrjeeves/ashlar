//! CLI (reference §11).
//!
//! Commands that exist: `check [path] [--human]`, `fix [path]`,
//! `build [path]`. Nothing else — a command that doesn't fully work is not
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
        ashlar fix [path]\n  \
        ashlar build [path]\n  \
        ashlar fmt [path] [--check]\n  \
        ashlar run [path]\n  \
        ashlar rename <part-or-part.prop> <new-name> [path] [--plan]\n  \
        ashlar rekind <part.prop> <kind> [path] [--plan]\n";

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Cmd {
        Check { path: String, human: bool },
        Fix { path: String },
        Build { path: String },
        Fmt { path: String, check_only: bool },
        Run { path: String },
        Rename { target: String, new_name: String, path: String, plan_only: bool },
        Rekind { target: String, kind: String, path: String, plan_only: bool },
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
            "fix" => Ok(Cmd::Fix {
                path: one_path(rest)?,
            }),
            "build" => Ok(Cmd::Build {
                path: one_path(rest)?,
            }),
            "rename" | "rekind" => {
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
                if name == "rename" {
                    Ok(Cmd::Rename {
                        target: positionals[0].clone(),
                        new_name: positionals[1].clone(),
                        path,
                        plan_only,
                    })
                } else {
                    Ok(Cmd::Rekind {
                        target: positionals[0].clone(),
                        kind: positionals[1].replace('+', " "),
                        path,
                        plan_only,
                    })
                }
            }
            "run" => Ok(Cmd::Run {
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
            Cmd::Fix { path } => run_fix(&path),
            Cmd::Build { path } => run_build(&path),
            Cmd::Fmt { path, check_only } => run_fmt(&path, check_only),
            Cmd::Run { path } => run_serve(&path),
            Cmd::Rename { target, new_name, path, plan_only } => {
                run_refactor(&path, plan_only, |srcs| {
                    // A target matching a part renames the part; otherwise
                    // the last segment names a property of the prefix part.
                    let checked = ashlar::check_sources(srcs.to_vec());
                    if checked.program.parts.contains_key(&target) {
                        ashlar::refactor::plan_rename_part(srcs, &new_name, &target)
                    } else if let Some((part, prop)) = target.rsplit_once('.') {
                        ashlar::refactor::plan_rename_prop(srcs, part, prop, &new_name)
                    } else {
                        Err(ashlar::refactor::Refusal(format!(
                            "`{}` names neither a part nor a part.property.",
                            target
                        )))
                    }
                })
            }
            Cmd::Rekind { target, kind, path, plan_only } => {
                run_refactor(&path, plan_only, |srcs| match target.rsplit_once('.') {
                    Some((part, prop)) => ashlar::refactor::plan_rekind(srcs, part, prop, &kind),
                    None => Err(ashlar::refactor::Refusal(
                        "rekind takes `<part>.<property>`.".to_string(),
                    )),
                })
            }
        }
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
        if plan_only {
            return 0;
        }
        match ashlar::refactor::execute(&sources, &plan) {
            Ok(after) => {
                for (rel, text) in &after {
                    let orig = sources.iter().find(|(p, _)| p == rel).map(|(_, s)| s);
                    if orig != Some(text) {
                        if let Err(e) = std::fs::write(root.join(rel), text) {
                            eprintln!("error writing {}: {}", rel, e);
                            return 1;
                        }
                        eprintln!("rewrote: {}", rel);
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

    fn run_serve(path: &str) -> i32 {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        match ashlar::http::serve(
            std::path::PathBuf::from(path),
            None,
            |port| eprintln!("serving on http://127.0.0.1:{}", port),
            stop,
        ) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{}", e);
                1
            }
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

    fn run_fix(path: &str) -> i32 {
        let root = Path::new(path);
        let result = ashlar::check_project(root);

        match ashlar::fixup::apply_fixes(root, &result.diags) {
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

        let text = ashlar::manifest::render(&result.program, &result.composed);
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
                    path: ".".to_string()
                }
            );
        }

        #[test]
        fn fix_with_path() {
            let cmd = parse(&args(&["fix", "some/proj"])).unwrap();
            assert_eq!(
                cmd,
                Cmd::Fix {
                    path: "some/proj".to_string()
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
