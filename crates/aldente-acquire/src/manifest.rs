//! Reading source roots and service hints from Clojure build manifests, as
//! data only (`07-safety.md`, "Build files are data").

use aldente_model::RootKind;

use crate::edn::{Edn, read_all};
use crate::walk::Manifest;

/// A root as written in a manifest, relative to the manifest's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredRoot {
    pub rel: String,
    pub kind: RootKind,
    /// deps.edn alias or Leiningen profile name, without the colon.
    pub alias: Option<String>,
    /// A tool default (deps.edn `src`, Leiningen `src`/`test`) rather than
    /// written in the file; dropped silently when the directory is absent.
    pub default: bool,
}

/// What one manifest says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestInfo {
    pub roots: Vec<DeclaredRoot>,
    /// Whether the manifest declares roots at all (including tool defaults such
    /// as deps.edn's implicit `["src"]`). If no manifest of a unit does, the
    /// unit falls back to `src`, `test` or its own directory.
    pub declares_roots: bool,
    /// Declares an entry point: Leiningen `:main`, or a shadow-cljs build.
    pub service: bool,
}

/// Parses `src` as the given manifest.
///
/// Example: `parse(DepsEdn, "{:aliases {:test {:extra-paths [\"test\"]}}}")`
/// gives roots `src` (the deps.edn default) and `test` (test, alias `test`).
pub fn parse(manifest: Manifest, src: &str) -> Result<ManifestInfo, String> {
    let forms = read_all(src).map_err(|e| e.to_string())?;
    match manifest {
        Manifest::DepsEdn => Ok(parse_deps(single_map(&forms)?)),
        Manifest::ProjectClj => parse_project(&forms),
        Manifest::ShadowCljs => Ok(parse_shadow(single_map(&forms)?)),
        Manifest::BbEdn => Ok(parse_bb(single_map(&forms)?)),
    }
}

/// The only form of an EDN file, which must be a map.
///
/// Example: `[Map([])]` gives the map; `[]` or `[Vector([])]` give errors.
fn single_map(forms: &[Edn]) -> Result<&Edn, String> {
    match forms {
        [form @ Edn::Map(_)] => Ok(form),
        [_] => Err("top-level form is not a map".to_owned()),
        [] => Err("file is empty".to_owned()),
        _ => Err("more than one top-level form".to_owned()),
    }
}

/// deps.edn: `:paths` (default `["src"]`) and each alias's `:paths`,
/// `:extra-paths` and `:replace-paths`. Never marks a service: aliases with
/// `-m` are as often test runners and tools as entry points; `-main` (R13)
/// decides later.
///
/// Example: `{:paths ["src"] :aliases {:test {:extra-paths ["test"]}}}` gives
/// roots `src` and `test` (alias `test`).
fn parse_deps(map: &Edn) -> ManifestInfo {
    let mut roots = match map.get_kw("paths") {
        Some(paths) => strings(paths, None, RootKind::Source),
        None => vec![default_root("src", RootKind::Source)],
    };
    if let Some(Edn::Map(aliases)) = map.get_kw("aliases") {
        for (name, body) in aliases {
            let Some(alias) = keyword_name(name) else {
                continue;
            };
            for key in ["paths", "extra-paths", "replace-paths"] {
                if let Some(paths) = body.get_kw(key) {
                    roots.extend(strings(paths, Some(alias), RootKind::Source));
                }
            }
        }
    }
    ManifestInfo {
        roots,
        declares_roots: true,
        service: false,
    }
}

/// project.clj: the `defproject` form's `:source-paths` (default `["src"]`),
/// `:test-paths` (default `["test"]`), the same keys inside `:profiles`, and
/// `:main` for the service flag. Values that are code (read-eval, unquote)
/// are not strings and are ignored.
///
/// Example: `(defproject a "1.0" :main a.core)` gives roots `src`, `test`
/// and `service: true`.
fn parse_project(forms: &[Edn]) -> Result<ManifestInfo, String> {
    let items = forms
        .iter()
        .find_map(|f| match f {
            Edn::List(items) if items.first() == Some(&Edn::Symbol("defproject".to_owned())) => {
                Some(items)
            }
            _ => None,
        })
        .ok_or_else(|| "no defproject form".to_owned())?;
    let options = Edn::Map(
        items
            .get(3..)
            .unwrap_or_default()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|kv| (kv[0].clone(), kv[1].clone()))
            .collect(),
    );
    let mut roots = lein_paths(&options, None, true);
    if let Some(Edn::Map(profiles)) = options.get_kw("profiles") {
        for (name, body) in profiles {
            if let Some(profile) = keyword_name(name) {
                roots.extend(lein_paths(body, Some(profile), false));
            }
        }
    }
    roots.extend(cljsbuild_paths(&options));
    Ok(ManifestInfo {
        roots,
        declares_roots: true,
        service: options.get_kw("main").is_some(),
    })
}

/// Leiningen `:source-paths` and `:test-paths` of one options map; the
/// defaults apply only at the top level, not in profiles.
///
/// Example: `{:test-paths ["spec"]}` at top level gives `src` and `spec`.
fn lein_paths(options: &Edn, profile: Option<&str>, defaults: bool) -> Vec<DeclaredRoot> {
    let mut roots = Vec::new();
    for (key, default, kind) in [
        ("source-paths", "src", RootKind::Source),
        ("test-paths", "test", RootKind::Test),
    ] {
        match options.get_kw(key) {
            Some(paths) => roots.extend(strings(paths, profile, kind)),
            None if defaults => roots.push(default_root(default, kind)),
            None => {}
        }
    }
    roots
}

/// lein-cljsbuild `:source-paths` from every build under `:cljsbuild :builds`
/// (a vector of build maps or a map of id to build), deduplicated, alias
/// `cljsbuild`. Build ids do not decide test-ness, since test builds also
/// list `src`; the directory name does.
///
/// Example: `{:cljsbuild {:builds [{:source-paths ["src"]}]}}` gives root
/// `src` (alias `cljsbuild`).
fn cljsbuild_paths(options: &Edn) -> Vec<DeclaredRoot> {
    let builds: Vec<&Edn> = match options.get_kw("cljsbuild").and_then(|c| c.get_kw("builds")) {
        Some(Edn::Map(by_id)) => by_id.iter().map(|(_, b)| b).collect(),
        Some(list) => list.items().unwrap_or_default().iter().collect(),
        None => Vec::new(),
    };
    let mut roots: Vec<DeclaredRoot> = Vec::new();
    for build in builds {
        if let Some(paths) = build.get_kw("source-paths") {
            for r in strings(paths, Some("cljsbuild"), RootKind::Source) {
                if !roots.contains(&r) {
                    roots.push(r);
                }
            }
        }
    }
    roots
}

/// shadow-cljs build targets that only run tests; they do not make a service.
const SHADOW_TEST_TARGETS: &[&str] = &["node-test", "browser-test", "karma"];

/// shadow-cljs.edn: `:source-paths`; any build with a non-test `:target`
/// marks a service.
///
/// Example: `{:source-paths ["src"] :builds {:app {:target :browser}}}` gives
/// root `src` and `service: true`.
fn parse_shadow(map: &Edn) -> ManifestInfo {
    let paths = map.get_kw("source-paths");
    let service = match map.get_kw("builds") {
        Some(Edn::Map(builds)) => builds.iter().any(|(_, b)| match b.get_kw("target") {
            Some(Edn::Keyword(t)) => !SHADOW_TEST_TARGETS.contains(&t.as_str()),
            Some(_) => true,
            None => false,
        }),
        _ => false,
    };
    ManifestInfo {
        roots: paths
            .map(|p| strings(p, None, RootKind::Source))
            .unwrap_or_default(),
        declares_roots: paths.is_some(),
        service,
    }
}

/// bb.edn: `:paths`.
///
/// Example: `{:paths ["script"]}` gives root `script`.
fn parse_bb(map: &Edn) -> ManifestInfo {
    let paths = map.get_kw("paths");
    ManifestInfo {
        roots: paths
            .map(|p| strings(p, None, RootKind::Source))
            .unwrap_or_default(),
        declares_roots: paths.is_some(),
        service: false,
    }
}

/// The string items of a vector or list as roots; anything else is skipped.
/// A root whose alias or directory name says "test" is a test root even when
/// `kind` is `Source`.
///
/// Example: `["src" :x "test"]` gives `src` (source) and `test` (test).
fn strings(paths: &Edn, alias: Option<&str>, kind: RootKind) -> Vec<DeclaredRoot> {
    paths
        .items()
        .unwrap_or_default()
        .iter()
        .filter_map(Edn::as_str)
        .map(|rel| root(rel, alias, kind))
        .collect()
}

/// One declared root, upgrading `kind` to `Test` when the alias name contains
/// `test` or any directory in the path is `test` or `tests`.
///
/// Example: `root("test/cljs", None, Source)` and `root("src/test/clojure",
/// None, Source)` have kind `Test`; `root("src", None, Source)` stays `Source`.
fn root(rel: &str, alias: Option<&str>, kind: RootKind) -> DeclaredRoot {
    let looks_like_test = alias.is_some_and(|a| a.contains("test"))
        || rel.split('/').any(|c| matches!(c, "test" | "tests"));
    DeclaredRoot {
        rel: rel.to_owned(),
        kind: if looks_like_test {
            RootKind::Test
        } else {
            kind
        },
        alias: alias.map(str::to_owned),
        default: false,
    }
}

/// A root the tool assumes when the manifest does not say.
///
/// Example: `default_root("src", Source)` has `default: true`.
fn default_root(rel: &str, kind: RootKind) -> DeclaredRoot {
    DeclaredRoot {
        default: true,
        ..root(rel, None, kind)
    }
}

/// The name of a keyword.
///
/// Example: `Keyword("test")` gives `Some("test")`; `Str("x")` gives `None`.
fn keyword_name(form: &Edn) -> Option<&str> {
    match form {
        Edn::Keyword(name) => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(rel: &str, kind: RootKind, alias: Option<&str>) -> DeclaredRoot {
        DeclaredRoot {
            rel: rel.into(),
            kind,
            alias: alias.map(Into::into),
            default: false,
        }
    }

    fn dflt(rel: &str, kind: RootKind) -> DeclaredRoot {
        DeclaredRoot {
            default: true,
            ..root(rel, kind, None)
        }
    }

    use RootKind::{Source, Test};

    #[test]
    fn deps_edn_paths_and_alias_paths() {
        let info = parse(
            Manifest::DepsEdn,
            r#"{:paths ["src" "resources" :other-alias]
                :aliases {:test {:extra-paths ["test"]}
                          :dev {:extra-paths ["dev"] :replace-paths ["x"]}
                          :old {:paths ["legacy"]}
                          :bad "not a map"}}"#,
        )
        .unwrap();
        assert_eq!(
            info.roots,
            [
                root("src", Source, None),
                root("resources", Source, None),
                root("test", Test, Some("test")),
                root("dev", Source, Some("dev")),
                root("x", Source, Some("dev")),
                root("legacy", Source, Some("old")),
            ]
        );
        assert!(info.declares_roots);
        assert!(!info.service, "deps.edn alone never marks a service");
    }

    #[test]
    fn deps_edn_defaults_to_src() {
        let info = parse(Manifest::DepsEdn, "{:deps {}}").unwrap();
        assert_eq!(info.roots, [dflt("src", Source)]);
        assert!(info.declares_roots);
    }

    #[test]
    fn test_kind_comes_from_alias_or_directory_name() {
        let info = parse(
            Manifest::DepsEdn,
            r#"{:paths ["src" "test"] :aliases {:integration-test {:extra-paths ["it"]}}}"#,
        )
        .unwrap();
        assert_eq!(info.roots[1], root("test", Test, None));
        assert_eq!(info.roots[2], root("it", Test, Some("integration-test")));
    }

    #[test]
    fn project_clj_paths_profiles_and_main() {
        let info = parse(
            Manifest::ProjectClj,
            r#"(defproject a/b "1.0"
                 :source-paths ["src/clj"]
                 :test-paths ["spec"]
                 :profiles {:dev {:source-paths ["dev"]} :uberjar {:aot :all}}
                 :main a.core)"#,
        )
        .unwrap();
        assert_eq!(
            info.roots,
            [
                root("src/clj", Source, None),
                root("spec", Test, None),
                root("dev", Source, Some("dev")),
            ]
        );
        assert!(info.service);
    }

    #[test]
    fn project_clj_defaults() {
        let info = parse(Manifest::ProjectClj, r#"(defproject a "1.0")"#).unwrap();
        assert_eq!(info.roots, [dflt("src", Source), dflt("test", Test)]);
        assert!(!info.service);
    }

    #[test]
    fn project_clj_code_is_data() {
        // Hostile case 5: read-eval and unquote values are not strings, so they
        // contribute nothing; the literal strings still count.
        let info = parse(
            Manifest::ProjectClj,
            r#"(defproject evil "0.1.0"
                 :source-paths ["src" #=(clojure.java.shell/sh "touch" "x")]
                 :test-paths ~(do (spit "y" "z") ["test"])
                 :main evil.core)"#,
        )
        .unwrap();
        assert_eq!(info.roots, [root("src", Source, None)]);
        assert!(info.service);
    }

    #[test]
    fn project_clj_cljsbuild_source_paths() {
        // Seen in metosin/reitit's examples: `:source-paths []` at the top and
        // the ClojureScript roots only under lein-cljsbuild.
        let info = parse(
            Manifest::ProjectClj,
            r#"(defproject frontend "0.1.0"
                 :source-paths []
                 :cljsbuild {:builds [{:id "app" :source-paths ["src"]}
                                      {:id "test" :source-paths ["src" "test/cljs"]}]})"#,
        )
        .unwrap();
        assert_eq!(
            info.roots,
            [
                dflt("test", Test),
                root("src", Source, Some("cljsbuild")),
                root("test/cljs", Test, Some("cljsbuild")),
            ]
        );
        // The map form of :builds works too.
        let map_form = parse(
            Manifest::ProjectClj,
            r#"(defproject a "1" :source-paths [] :test-paths []
                 :cljsbuild {:builds {:dev {:source-paths ["src-cljs"]}}})"#,
        )
        .unwrap();
        assert_eq!(
            map_form.roots,
            [root("src-cljs", Source, Some("cljsbuild"))]
        );
    }

    #[test]
    fn shadow_cljs_test_builds_are_not_services() {
        let info = parse(
            Manifest::ShadowCljs,
            r#"{:builds {:node-test {:target :node-test}
                         :browser-test {:target :browser-test}
                         :karma {:target :karma}}}"#,
        )
        .unwrap();
        assert!(!info.service);
    }

    #[test]
    fn project_clj_without_defproject_is_an_error() {
        assert!(parse(Manifest::ProjectClj, "(println 1)").is_err());
    }

    #[test]
    fn shadow_cljs_source_paths_and_builds() {
        let info = parse(
            Manifest::ShadowCljs,
            r#"{:source-paths ["src/main" "src/test"]
                :builds {:app {:target :browser :modules {:main {:init-fn app.core/init}}}}}"#,
        )
        .unwrap();
        assert_eq!(
            info.roots,
            [root("src/main", Source, None), root("src/test", Test, None)]
        );
        assert!(info.service);
        let bare = parse(Manifest::ShadowCljs, "{:deps true}").unwrap();
        assert!(!bare.declares_roots);
        assert!(!bare.service);
    }

    #[test]
    fn bb_edn_paths() {
        let info = parse(Manifest::BbEdn, r#"{:paths ["script"] :tasks {}}"#).unwrap();
        assert_eq!(info.roots, [root("script", Source, None)]);
        assert!(
            !parse(Manifest::BbEdn, "{:tasks {}}")
                .unwrap()
                .declares_roots
        );
    }

    #[test]
    fn malformed_manifests_are_errors() {
        assert!(parse(Manifest::DepsEdn, "{:paths [").is_err());
        assert!(parse(Manifest::DepsEdn, "[1 2]").is_err());
        assert!(parse(Manifest::DepsEdn, "{} {}").is_err());
        assert!(parse(Manifest::ShadowCljs, "").is_err());
    }
}
