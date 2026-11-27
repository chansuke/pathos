//! The direction of dependencies, between the crates of the workspace and
//! between the modules of the app. Test code is not checked: it may reach
//! anything to set up a case.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use syn::visit::Visit;

/// The workspace crates each crate may depend on, outside its
/// dev-dependencies. pathos-core comes first and depends on nothing, so the
/// Lean model is its whole specification (ADR 3).
const CRATES: &[(&str, &[&str])] = &[
    ("pathos-core", &[]),
    ("pathos-render", &["pathos-core"]),
    ("pathos-measure", &["pathos-core", "pathos-render"]),
    ("pathos-playback", &["pathos-core", "pathos-render"]),
    ("pathos-reaper", &["pathos-core", "pathos-render"]),
    ("pathos-agent", &["pathos-core", "pathos-measure"]),
    (
        "pathos",
        &[
            "pathos-agent",
            "pathos-core",
            "pathos-playback",
            "pathos-render",
        ],
    ),
];

/// The modules of the app, lowest layer first. A module may use the modules
/// of the layers before its own, never those of its own layer or later ones;
/// `main.rs` is above all of them.
const LAYERS: &[&[&str]] = &[
    // Plain Rust, with no window: their tests run without one.
    &["file", "layout", "meter", "readout"],
    &["editor", "theme"],
    &["controller"],
    // The views that gpui draws.
    &["agent"],
];

/// The modules that draw with gpui. The others are plain Rust, so their
/// tests need no window.
const VIEWS: &[&str] = &["agent", "theme"];

/// Uses of `main.rs` from a module that are allowed: the window's view,
/// whose `Context` a view of a part of the window draws in.
const ROOT_ITEMS: &[(&str, &str)] = &[("agent", "Pathos")];

#[test]
fn crates_depend_only_on_the_crates_below_them() {
    let metadata = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let allowed: BTreeMap<&str, &[&str]> = CRATES.iter().copied().collect();
    let mut problems = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or_default();
        let Some(allowed) = allowed.get(name) else {
            problems.push(format!(
                "{name} is not in CRATES in tests/architecture.rs; add it with the workspace crates it may use"
            ));
            continue;
        };
        for dependency in package["dependencies"].as_array().into_iter().flatten() {
            if dependency["kind"].as_str() == Some("dev") {
                continue;
            }
            let to = dependency["name"].as_str().unwrap_or_default();
            if name == "pathos-core" {
                problems.push(format!(
                    "pathos-core depends on {to}. pathos-core stays dependency-free so that the Lean model is its whole \
                     specification (ADR 3): put the code that needs {to} in pathos-render, pathos-agent or a new crate above the core"
                ));
            } else if to.starts_with("pathos") && !allowed.contains(&to) {
                problems.push(format!(
                    "{name} depends on {to}, which CRATES in tests/architecture.rs does not allow. Move what both need into a \
                     crate below them, or let the caller pass it in; do not add the edge to CRATES if it makes a cycle"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn app_modules_use_only_lower_layers() {
    let (unknown, violations) = check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(
        unknown.is_empty(),
        "These modules of the app are not in LAYERS in tests/architecture.rs: {unknown:?}.\n\
         Put each in the lowest layer that holds everything it uses."
    );
    let new: Vec<_> = violations.iter().map(|v| v.message.as_str()).collect();
    assert!(
        new.is_empty(),
        "{} uses break the layers of the app:\n  {}\n\n\
         A module may use only modules in lower layers (LAYERS in tests/architecture.rs), and only the views in VIEWS \
         may use gpui. Fix the design, not the check: move what both modules need into a lower module, such as the \
         colors into theme.rs, or let the caller pass it in. Move drawing to main.rs or a view; layout returns rects.",
        new.len(),
        new.join("\n  ")
    );
}

#[test]
fn the_check_reports_uses_of_higher_layers_and_gpui_and_ignores_tests() {
    let src = std::env::temp_dir().join(format!("pathos-architecture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();
    let write = |name: &str, text: &str| std::fs::write(src.join(name), text).unwrap();
    write(
        "main.rs",
        "mod agent;\nmod editor;\nmod file;\nmod layout;\nmod meter;\nmod readout;\nstruct Pathos;\nconst TEXT: u32 = 0;\n",
    );
    write(
        "agent.rs",
        "use crate::{Pathos, TEXT};\nuse crate::layout::Rect;\n",
    );
    write(
        "layout.rs",
        "pub struct Rect;\nfn up() { crate::editor::Editor::new(); }\n#[cfg(test)]\nmod tests { use crate::agent::Card; }\n",
    );
    write("editor.rs", "use gpui::Div;\n");
    for name in ["file.rs", "meter.rs", "readout.rs"] {
        write(name, "");
    }
    let (unknown, violations) = check(&src);
    let _ = std::fs::remove_dir_all(&src);
    assert!(unknown.is_empty(), "{unknown:?}");
    let found: BTreeSet<_> = violations
        .iter()
        .map(|v| (v.from.as_str(), v.item.as_str()))
        .collect();
    assert_eq!(
        found,
        BTreeSet::from([("agent", "TEXT"), ("layout", "editor"), ("editor", "gpui")]),
    );
}

/// A use that points up or sideways: the module, the item or module it uses,
/// and a message with its place.
struct Violation {
    from: String,
    item: String,
    message: String,
}

/// Modules missing from LAYERS, and each use that breaks the layers.
fn check(src: &Path) -> (BTreeSet<String>, Vec<Violation>) {
    let mut rank = BTreeMap::new();
    for (index, layer) in LAYERS.iter().enumerate() {
        for module in *layer {
            assert!(
                rank.insert(*module, index).is_none(),
                "{module} is in LAYERS twice"
            );
        }
    }
    let root = parse(&src.join("main.rs"));
    let modules: BTreeSet<String> = root
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(module) if !is_test_only(&module.attrs) => {
                Some(module.ident.to_string())
            }
            _ => None,
        })
        .collect();
    let unknown = modules
        .iter()
        .filter(|module| !rank.contains_key(module.as_str()))
        .cloned()
        .collect();
    let mut violations = Vec::new();
    for module in &modules {
        let file = src.join(format!("{module}.rs"));
        let mut uses = Uses::default();
        uses.visit_file(&parse(&file));
        for (path, line) in uses.paths {
            let at = format!("src/{module}.rs:{line}");
            let mut found = |item: &str, why: String| {
                violations.push(Violation {
                    from: module.clone(),
                    item: item.to_string(),
                    message: format!("{at}: `{module}` uses `{}`, {why}", path.join("::")),
                });
            };
            match path.first().map(String::as_str) {
                Some("gpui") if !VIEWS.contains(&module.as_str()) => {
                    found(
                        "gpui",
                        "but only the views in VIEWS draw with gpui".to_string(),
                    );
                }
                Some("crate") => {
                    let Some(second) = path.get(1) else { continue };
                    if modules.contains(second) {
                        let allowed = match (rank.get(module.as_str()), rank.get(second.as_str())) {
                            (Some(from), Some(to)) => to < from,
                            _ => true,
                        };
                        if !allowed {
                            found(
                                second,
                                format!("which is not in a lower layer than `{module}`"),
                            );
                        }
                    } else if !ROOT_ITEMS.contains(&(module.as_str(), second.as_str())) {
                        found(
                            second,
                            "which belongs to main.rs, above every module".to_string(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    (unknown, violations)
}

fn parse(file: &Path) -> syn::File {
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    syn::parse_file(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// Whether an item is compiled only for tests: `#[cfg(test)]`, or a `cfg`
/// whose condition requires `test`, such as `cfg(all(test, unix))`.
fn is_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr.meta.require_list().is_ok_and(|list| {
                let condition = list.tokens.to_string().replace(' ', "");
                condition == "test"
                    || condition.starts_with("all(")
                        && condition.split([',', '(', ')']).any(|part| part == "test")
            })
    })
}

/// Every path a file names outside test code, each `use` flattened into
/// one path per name, with its line.
#[derive(Default)]
struct Uses {
    paths: Vec<(Vec<String>, usize)>,
}

impl Uses {
    fn flatten(&mut self, tree: &syn::UseTree, prefix: Vec<String>) {
        let with = |segment: String| {
            let mut path = prefix.clone();
            path.push(segment);
            path
        };
        match tree {
            syn::UseTree::Path(path) => self.flatten(&path.tree, with(path.ident.to_string())),
            syn::UseTree::Name(name) => {
                let ident = name.ident.to_string();
                let line = name.ident.span().start().line;
                let path = if ident == "self" { prefix } else { with(ident) };
                self.paths.push((path, line));
            }
            syn::UseTree::Rename(rename) => {
                let line = rename.ident.span().start().line;
                self.paths.push((with(rename.ident.to_string()), line));
            }
            syn::UseTree::Glob(glob) => {
                self.paths
                    .push((prefix, glob.star_token.spans[0].start().line));
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    self.flatten(tree, prefix.clone());
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Uses {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attrs = match item {
            syn::Item::Const(i) => &i.attrs,
            syn::Item::Enum(i) => &i.attrs,
            syn::Item::Fn(i) => &i.attrs,
            syn::Item::Impl(i) => &i.attrs,
            syn::Item::Mod(i) => &i.attrs,
            syn::Item::Static(i) => &i.attrs,
            syn::Item::Struct(i) => &i.attrs,
            syn::Item::Use(i) => &i.attrs,
            _ => return syn::visit::visit_item(self, item),
        };
        if !is_test_only(attrs) {
            syn::visit::visit_item(self, item);
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.flatten(&item.tree, Vec::new());
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if segments.len() > 1 {
            let line = path.segments[0].ident.span().start().line;
            self.paths.push((segments, line));
        }
        syn::visit::visit_path(self, path);
    }
}
