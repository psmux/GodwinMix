use super::*;

fn parse(spec: &str) -> Source {
    Source::parse(spec).unwrap_or_else(|e| panic!("`{spec}` should parse: {e}"))
}

#[test]
fn every_form_in_06_section_2_parses() {
    assert_eq!(
        parse("owner/gmx-ndi"),
        Source::GitHub { owner: "owner".into(), repo: "gmx-ndi".into(), version: None }
    );
    assert_eq!(
        parse("https://gitlab.com/x/y.git"),
        Source::Git { url: "https://gitlab.com/x/y.git".into(), reference: None }
    );
    assert_eq!(
        parse("cargo:gmx-ndi"),
        Source::Cargo { package: "gmx-ndi".into(), version: None }
    );
    assert_eq!(
        parse("npm:@x/gmx-chat"),
        Source::Npm { package: "@x/gmx-chat".into(), version: None }
    );
    assert_eq!(
        parse("pypi:gmx-director"),
        Source::PyPi { package: "gmx-director".into(), version: None }
    );
    assert_eq!(
        parse("oci:ghcr.io/x/gmx-ai-vision:1.0"),
        Source::Oci { reference: "ghcr.io/x/gmx-ai-vision:1.0".into() }
    );
    assert_eq!(parse("./my-plugin"), Source::Path("./my-plugin".into()));
}

#[test]
fn a_version_can_be_pinned_on_every_registry_form() {
    assert_eq!(
        parse("owner/gmx-ndi@1.2.0"),
        Source::GitHub {
            owner: "owner".into(),
            repo: "gmx-ndi".into(),
            version: Some("1.2.0".into())
        }
    );
    assert_eq!(
        parse("cargo:gmx-ndi@0.3.1"),
        Source::Cargo { package: "gmx-ndi".into(), version: Some("0.3.1".into()) }
    );
    assert_eq!(
        parse("pypi:gmx-director@2.0.0"),
        Source::PyPi { package: "gmx-director".into(), version: Some("2.0.0".into()) }
    );
}

#[test]
fn an_npm_scope_is_not_read_as_a_version() {
    assert_eq!(
        parse("npm:@godwinmix/gmx-chat@1.4.0"),
        Source::Npm {
            package: "@godwinmix/gmx-chat".into(),
            version: Some("1.4.0".into())
        }
    );
    assert_eq!(
        parse("npm:@godwinmix/gmx-chat"),
        Source::Npm { package: "@godwinmix/gmx-chat".into(), version: None }
    );
}

#[test]
fn a_git_reference_rides_after_a_hash() {
    assert_eq!(
        parse("https://gitlab.com/x/y.git#v2"),
        Source::Git {
            url: "https://gitlab.com/x/y.git".into(),
            reference: Some("v2".into())
        }
    );
    assert_eq!(
        parse("git+ssh://git@example.com/x/y.git"),
        Source::Git { url: "ssh://git@example.com/x/y.git".into(), reference: None }
    );
}

#[test]
fn paths_are_told_from_repositories_by_the_leading_dot_or_slash() {
    assert!(matches!(parse("."), Source::Path(_)));
    assert!(matches!(parse("./plugins/clock"), Source::Path(_)));
    assert!(matches!(parse("../clock"), Source::Path(_)));
    assert!(matches!(parse("/opt/gmx/clock"), Source::Path(_)));
    assert!(matches!(parse("~/clock"), Source::Path(_)));
    assert!(matches!(parse("C:\\plugins\\clock"), Source::Path(_)));
    // Two segments with no dot is a repository, which is the whole ambiguity.
    assert!(matches!(parse("psmux/clock"), Source::GitHub { .. }));
}

#[test]
fn a_pasted_github_web_url_works_like_owner_slash_repo() {
    assert_eq!(
        parse("https://github.com/psmux/gmx-ndi"),
        Source::GitHub { owner: "psmux".into(), repo: "gmx-ndi".into(), version: None }
    );
}

#[test]
fn a_bare_name_is_refused_with_every_form_listed() {
    let err = Source::parse("ndi").expect_err("a bare name is not a source");
    let text = format!("{err}");
    for form in ["owner/repo", "cargo:", "npm:", "pypi:", "oci:", "./my-plugin"] {
        assert!(text.contains(form), "the refusal should name {form}: {text}");
    }
    assert!(text.contains("gmx marketplace add"), "{text}");
}

#[test]
fn a_url_that_is_neither_a_git_source_nor_github_is_refused() {
    let err = Source::parse("https://example.com/plugins/ndi.tar.gz")
        .expect_err("a tarball URL is not one of the forms");
    assert!(format!("{err}").contains(".git"), "{err}");
}

#[test]
fn only_a_path_avoids_the_network() {
    assert!(!parse("./x").needs_network());
    assert!(parse("owner/repo").needs_network());
    assert!(parse("cargo:x").needs_network());
}

#[test]
fn offline_refuses_a_network_source_and_names_the_one_that_works() {
    let ctx = FetchCtx {
        offline: true,
        ..FetchCtx::new(std::env::temp_dir().join("gmx-offline"), "linux-x86_64")
    };
    let err = fetch(&parse("owner/repo"), &ctx).expect_err("offline");
    assert!(format!("{err}").contains("gmx plugin add ./"), "{err}");
}

#[test]
fn what_a_source_displays_as_round_trips_through_the_parser() {
    for spec in [
        "owner/gmx-ndi",
        "owner/gmx-ndi@1.2.0",
        "cargo:gmx-ndi@0.3.1",
        "npm:@x/gmx-chat@1.0.0",
        "pypi:gmx-director",
        "oci:ghcr.io/x/y:1.0",
        "https://gitlab.com/x/y.git#v2",
    ] {
        assert_eq!(parse(spec).display(), spec, "`{spec}` should survive a round trip");
    }
}

#[test]
fn the_manifest_root_is_found_at_the_top_or_one_level_in() {
    let root = std::env::temp_dir().join(format!("gmx-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let inner = root.join("gmx-clock-1.0.0");
    std::fs::create_dir_all(&inner).expect("a tree");
    std::fs::write(inner.join("gmx-plugin.toml"), "[plugin]\n").expect("a manifest");
    assert_eq!(find_manifest_root(&root).expect("it is found"), inner);

    std::fs::write(root.join("gmx-plugin.toml"), "[plugin]\n").expect("a manifest");
    assert_eq!(find_manifest_root(&root).expect("the top wins"), root);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_archive_with_no_manifest_lists_what_was_in_it() {
    let root = std::env::temp_dir().join(format!("gmx-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a tree");
    std::fs::write(root.join("README.md"), "hello").expect("a file");
    let err = find_manifest_root(&root).expect_err("there is no manifest");
    assert!(format!("{err}").contains("README.md"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}
