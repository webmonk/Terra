//! B1 evaluator-authority ratchet (audits B1-D1, B1-D2, and B1-D3).
//!
//! `StackEvaluator` is the CPU executor. Terra-core must not grow another
//! compiled graph until that graph lands with a production executor, a
//! production caller, and an end-to-end result test.
//! The retired parallel multi-field state generation must likewise stay absent
//! until it has a real executor and end-to-end caller.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn stack_evaluator_remains_the_single_cpu_authority() {
    let eval_path = manifest_dir().join("src/eval/mod.rs");
    let source = read(&eval_path);

    for forbidden in ["last_graph", "compile_graph", "compile_eval_graph"] {
        assert!(
            !source.contains(forbidden),
            "{} contains retired evaluator-graph token `{forbidden}`; StackEvaluator must execute the tree walk directly",
            eval_path.display()
        );
    }

    assert!(
        source.contains("pub fn evaluate_nodes"),
        "{} no longer exposes the established CPU tree-walk authority; update this ratchet only with equivalent end-to-end coverage",
        eval_path.display()
    );
}

#[test]
fn inert_core_graph_and_operator_scaffolding_stays_retired() {
    let terrain_eval = manifest_dir().join("src/terrain_eval");
    assert!(
        !terrain_eval.exists(),
        "{} reintroduced retired evaluator scaffolding; a future evaluator generation requires an executor, production caller, and end-to-end result test",
        terrain_eval.display()
    );

    let mut sources = Vec::new();
    collect_rs(&manifest_dir().join("src"), &mut sources);
    let mut compilers = Vec::new();
    for path in sources {
        let source = production_source(&read(&path));
        for (line_index, line) in source.lines().enumerate() {
            let tokens: Vec<_> = line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .collect();
            for pair in tokens.windows(2) {
                if pair[0] == "fn" && pair[1].contains("compile") && pair[1].contains("graph") {
                    compilers.push(format!(
                        "{}:{} ({})",
                        path.display(),
                        line_index + 1,
                        pair[1]
                    ));
                }
            }
        }
    }

    assert!(
        compilers.is_empty(),
        "terra-core contains a production graph compiler without ratcheted execution evidence:\n  {}\nA future graph must land with its executor, non-test caller, and an end-to-end result test; then update this guard deliberately.",
        compilers.join("\n  ")
    );
}

#[test]
fn unused_multi_field_state_stays_retired() {
    let context = manifest_dir().join("src/fields/context.rs");
    assert!(
        !context.exists(),
        "{} reintroduced the retired parallel field-state bridge; multi-field execution must enter through a production executor and end-to-end caller",
        context.display()
    );

    let core_dir = manifest_dir();
    let crates_dir = core_dir
        .parent()
        .expect("terra-core lives below the workspace crates directory");
    let mut sources = Vec::new();
    let crate_entries = fs::read_dir(crates_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", crates_dir.display()));
    for entry in crate_entries.flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rs(&src, &mut sources);
        }
    }

    let mut references = Vec::new();
    for path in sources {
        let source = production_source(&read(&path));
        for (line_index, line) in source.lines().enumerate() {
            let tokens: Vec<_> = line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .collect();
            for forbidden in ["TerrainContext", "terrain_eval"] {
                if tokens.contains(&forbidden) {
                    references.push(format!(
                        "{}:{} ({forbidden})",
                        path.display(),
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        references.is_empty(),
        "production sources reference retired multi-field evaluator state:\n  {}\nA future replacement requires a production executor, caller, and end-to-end result test.",
        references.join("\n  ")
    );
}

#[test]
fn metadata_only_tile_evaluator_stays_retired() {
    let terrain = manifest_dir().join("src/terrain");
    for retired in ["executor.rs", "work.rs"] {
        let path = terrain.join(retired);
        assert!(
            !path.exists(),
            "{} reintroduced metadata-only tiled evaluation; a replacement requires a production executor, caller, payload publication, and end-to-end result test",
            path.display()
        );
    }

    let core_dir = manifest_dir();
    let crates_dir = core_dir
        .parent()
        .expect("terra-core lives below the workspace crates directory");
    let mut sources = Vec::new();
    for entry in fs::read_dir(crates_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", crates_dir.display()))
        .flatten()
    {
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rs(&src, &mut sources);
        }
    }

    let forbidden = [
        "execute_vector_height_tile",
        "TerrainWorkScheduler",
        "TerrainWorkItem",
        "publish_fallback_result",
    ];
    let mut references = Vec::new();
    for path in sources {
        let source = production_source(&read(&path));
        for (line_index, line) in source.lines().enumerate() {
            let tokens: Vec<_> = line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .collect();
            for retired in forbidden {
                if tokens.contains(&retired) {
                    references.push(format!("{}:{} ({retired})", path.display(), line_index + 1));
                }
            }
        }
    }
    assert!(
        references.is_empty(),
        "production sources reference retired tiled-evaluator metadata:\n  {}",
        references.join("\n  ")
    );

    let pyramid = read(&terrain.join("pyramid.rs"));
    assert!(
        pyramid.contains("handle: TilePageHandle") && pyramid.contains("pub fn publish_resident"),
        "TerrainPyramid residency must require and store a backend payload handle"
    );
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Strip comments/string contents and omit `#[cfg(test)]` items. This is a
/// deliberately small source tripwire, not a Rust parser.
fn production_source(source: &str) -> String {
    let mut output = String::new();
    let mut pending_test_item = false;
    let mut skipped_depth: Option<i32> = None;

    for raw in source.lines() {
        let line = strip_comments_and_strings(raw);
        let delta = brace_delta(&line);

        if let Some(depth) = skipped_depth.as_mut() {
            *depth += delta;
            if *depth <= 0 {
                skipped_depth = None;
            }
            output.push('\n');
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            pending_test_item = true;
            output.push('\n');
            continue;
        }

        if pending_test_item {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                output.push('\n');
                continue;
            }
            if delta > 0 {
                skipped_depth = Some(delta);
            }
            pending_test_item = false;
            output.push('\n');
            continue;
        }

        output.push_str(&line);
        output.push('\n');
    }

    output
}

fn brace_delta(line: &str) -> i32 {
    line.bytes().fold(0, |depth, byte| match byte {
        b'{' => depth + 1,
        b'}' => depth - 1,
        _ => depth,
    })
}

fn strip_comments_and_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            out.push(' ');
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(' ');
        } else if ch == '/' && chars.peek() == Some(&'/') {
            break;
        } else {
            out.push(ch);
        }
    }

    out
}
