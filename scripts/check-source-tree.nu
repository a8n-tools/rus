#!/usr/bin/env nu

# Reject drift between `src/` and the source trees in README.md and CLAUDE.md.
# Every `src/**/*.rs` file needs a row and every row needs a file, so the
# comparison runs in both directions.
#
# Why this exists: the trees lost the whole `src/oidc/` block (RUS-28), and then
# `lib.rs`, `testing.rs` and `handlers/webhook.rs` (RUS-29). Only a human ever
# compared the two lists, and nobody did.
#
# Only the `src/` block of each tree is compared, and only the file name column
# of it. Descriptions are free text and are never read, so rewording a comment
# cannot fail a build; a module with no row, or a row naming no module, is the
# only drift reported. Rows outside `src/` are a curated map, not an inventory,
# and are deliberately left alone.
#
# Static like the manifest half of `scripts/check-cargo-tests-ran.nu`: no cargo,
# no docker, no compilation, so it fails in milliseconds rather than after a build.
#
# Usage:
#   nu scripts/check-source-tree.nu --self-test
#   nu scripts/check-source-tree.nu
#
# Exit codes:
# - 0: every doc's `src/` block and `src/` itself name exactly the same files.
# - 1: they disagree, or a block could not be parsed. A parse that stopped
#      matching is a failure, never a silent pass: an unrecognised tree shape
#      would otherwise report "no drift" for a tree it never read.

const DOCS = [
    {path: "README.md", heading: "## Project Structure"}
    {path: "CLAUDE.md", heading: "### Source Structure"}
]

# One nesting level of a box-drawing tree is exactly four columns.
const INDENT_RE = '^(?<prefix>(?:│   |    |├── |└── )*)(?<rest>\S.*)$'

# The `src/` subtree of a full tree path, anchored to a component boundary so
# `rus/src/db.rs` yields `src/db.rs` and `static/index.html` yields nothing.
const SRC_RE = '(?:^|/)(?<rel>src/.+)$'

const FENCE = "```"

# The fenced block under a heading. Returns {lines, error}; a heading or fence
# that has moved is an error rather than an empty, silently-passing block.
def fenced-block [doc: string, heading: string, label: string]: nothing -> record {
    let lines = ($doc | lines)
    let starts = ($lines | enumerate | where {|row| ($row.item | str trim) == $heading} | get index)
    if ($starts | is-empty) {
        return {lines: [], error: $"($label): found no `($heading)` heading, so nothing was compared."}
    }
    let after = ($lines | skip (($starts | first) + 1))
    let fences = ($after | enumerate | where {|row| ($row.item | str trim) == $FENCE} | get index)
    if ($fences | length) < 2 {
        return {lines: [], error: $"($label): found no fenced tree under `($heading)`, so nothing was compared."}
    }
    let open = ($fences | first)
    let close = ($fences | get 1)
    {lines: ($after | skip ($open + 1) | first ($close - $open - 1)), error: ""}
}

# Walk a box-drawing tree into full paths, directories keeping their trailing
# slash. Returns {paths, error}: any line whose indent does not resolve is an
# error, because a tree drawn a new way must be re-read rather than half-read.
def tree-paths [lines: list<string>, label: string]: nothing -> record {
    mut stack = []
    mut paths = []
    for line in $lines {
        let trimmed = ($line | str trim --right)
        if ($trimmed | is-empty) { continue }
        let parsed = ($trimmed | parse --regex $INDENT_RE)
        if ($parsed | is-empty) {
            return {paths: [], error: $"($label): cannot read the indent of `($trimmed)`; the tree is no longer drawn with 4-column box-drawing prefixes."}
        }
        # Graphemes, not bytes: a box-drawing character is three bytes wide.
        let depth = (($parsed | get prefix.0 | str length --grapheme-clusters) / 4 | into int)
        if $depth > ($stack | length) {
            return {paths: [], error: $"($label): `($trimmed)` sits ($depth) levels deep under a parent ($stack | length) levels deep, so the tree skips a level and cannot be walked."}
        }
        let name = ($parsed | get rest.0 | split row --regex '\s+' | first)
        $stack = ($stack | first $depth)
        $paths = ($paths | append (($stack | str join "") + $name))
        if ($name | str ends-with "/") {
            $stack = ($stack | append $name)
        }
    }
    {paths: $paths, error: ""}
}

# The `src/` rows of one tree, split into files and directories.
def src-rows [doc: string, heading: string, label: string]: nothing -> record {
    let block = (fenced-block $doc $heading $label)
    if ($block.error | is-not-empty) {
        return {files: [], dirs: [], error: $block.error}
    }
    let walked = (tree-paths $block.lines $label)
    if ($walked.error | is-not-empty) {
        return {files: [], dirs: [], error: $walked.error}
    }
    let rel = ($walked.paths | each {|p| $p | parse --regex $SRC_RE} | flatten | get rel? | default [])
    if ($rel | is-empty) {
        return {files: [], dirs: [], error: $"($label): the tree under `($heading)` has no `src/` rows at all, so nothing was compared."}
    }
    {
        files: ($rel | where {|p| not ($p | str ends-with "/")} | sort)
        dirs: ($rel | where {|p| $p | str ends-with "/"} | sort)
        error: ""
    }
}

# Compare both directions. `files` are the real `src/**/*.rs` paths.
def violations [files: list<string>, rows: record, label: string]: nothing -> list<string> {
    mut found = []
    if ($files | is-empty) {
        $found = ($found | append "no src/**/*.rs files were found, so there was nothing to compare")
    }
    if ($rows.files | is-empty) {
        $found = ($found | append $"($label): the tree lists no module under src/, so there was nothing to compare")
    }
    for row in ($rows.files | uniq --repeated) {
        $found = ($found | append $"($label): ($row) has more than one row")
    }
    for row in ($rows.files | uniq) {
        if not ($row in $files) {
            $found = ($found | append $"($label): row ($row) names no file; there is no ($row) on disk. Delete or correct the row.")
        }
    }
    for file in $files {
        if not ($file in $rows.files) {
            $found = ($found | append $"($label): ($file) has no row in the tree. Add one with a one-line comment in the style of its neighbours.")
        }
    }
    $found
}

# A directory row whose directory is gone would otherwise hide a whole subtree:
# its children go with it, so neither side has anything left to disagree about.
def dir-violations [dirs: list<string>, label: string]: nothing -> list<string> {
    $dirs
    | where {|d| ($d | str trim --right --char "/" | path type) != "dir" }
    | each {|d| $"($label): row ($d) names no directory; there is no ($d) on disk." }
}

# Prove the parser still reads the real tree shape and that each drift is still
# rejected. Without it a parser that stopped matching would pass every job
# silently, which is the same blindness the guard exists to remove.
def run-self-test [] {
    let readme_shaped = "
## Project Structure

```
rus/
├── src/
│   ├── lib.rs               # Library root
│   ├── main.rs              # Binary wiring
│   └── handlers/
│       ├── mod.rs
│       └── webhook.rs       # Maintenance webhook (saas)
├── static/
│   └── index.html           # Landing page
└── justfile
```
"
    let expected = ["src/handlers/mod.rs" "src/handlers/webhook.rs" "src/lib.rs" "src/main.rs"]
    let parsed = (src-rows $readme_shaped "## Project Structure" "sample")
    if ($parsed.error | is-not-empty) {
        print --stderr $"[check-source-tree] SELF-TEST FAILED: the parser rejected a well-formed tree: ($parsed.error)"
        exit 1
    }
    if $parsed.files != $expected {
        print --stderr $"[check-source-tree] SELF-TEST FAILED: the parser no longer reads the real tree shape: ($parsed.files | to json --raw)"
        exit 1
    }
    if $parsed.dirs != ["src/handlers/"] {
        print --stderr $"[check-source-tree] SELF-TEST FAILED: the parser read the directory rows as ($parsed.dirs | to json --raw)"
        exit 1
    }
    if (violations $expected $parsed "sample" | is-not-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a tree matching the filesystem was rejected."
        exit 1
    }

    # Both directions, and both empty sides.
    if (violations ($expected | append "src/testing.rs") $parsed "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a module with no row was accepted."
        exit 1
    }
    if (violations ($expected | first 3) $parsed "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a row naming no module was accepted."
        exit 1
    }
    if (violations [] $parsed "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: an empty src/ was accepted."
        exit 1
    }
    if (violations $expected {files: [], dirs: []} "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a tree with no src/ rows was accepted."
        exit 1
    }
    if (violations $expected {files: ($parsed.files | append "src/lib.rs"), dirs: []} "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a module listed twice was accepted."
        exit 1
    }
    if (dir-violations ["src/nope/"] "sample" | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a directory row naming no directory was accepted."
        exit 1
    }

    # Free text is not part of the comparison, so rewording a comment or
    # dropping one entirely must leave the verdict alone.
    let reworded = ($readme_shaped | str replace "# Library root" "# owns every module, and every target compiles through it")
    if (src-rows $reworded "## Project Structure" "sample").files != $expected {
        print --stderr "[check-source-tree] SELF-TEST FAILED: rewording a description changed which modules were read."
        exit 1
    }

    # Anything the parser cannot read is a failure, never a silent pass.
    let renamed = ($readme_shaped | str replace "## Project Structure" "## Layout")
    if ((src-rows $renamed "## Project Structure" "sample").error | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a renamed heading was accepted instead of failing loudly."
        exit 1
    }
    let unfenced = ($readme_shaped | str replace --all $FENCE "~~~")
    if ((src-rows $unfenced "## Project Structure" "sample").error | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a tree with no fence was accepted instead of failing loudly."
        exit 1
    }
    let redrawn = ($readme_shaped | str replace "│   ├── lib.rs" "  |- lib.rs")
    if ((src-rows $redrawn "## Project Structure" "sample").error | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a row drawn a new way was accepted instead of failing loudly."
        exit 1
    }
    let skipped = ($readme_shaped | str replace "├── src/" "│   │   ├── src/")
    if ((src-rows $skipped "## Project Structure" "sample").error | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a tree that skips a nesting level was accepted."
        exit 1
    }
    let no_src = ($readme_shaped | str replace --all "src/" "lib/")
    if ((src-rows $no_src "## Project Structure" "sample").error | is-empty) {
        print --stderr "[check-source-tree] SELF-TEST FAILED: a tree with no src/ block was accepted."
        exit 1
    }

    print "[check-source-tree] SELF-TEST OK: a missing row, an extra row, a duplicate row, an empty side and an unreadable tree are all rejected, a matching tree and a reworded description are not."
}

export def main [
    --self-test # check the guard still detects drift, then exit
] {
    if $self_test {
        run-self-test
        return
    }

    if not ("Cargo.toml" | path exists) {
        print --stderr "[check-source-tree] FAILED: run this from the repository root."
        exit 1
    }

    let files = (glob src/**/*.rs | path relative-to $env.PWD | sort)
    mut found = []
    for doc in $DOCS {
        if not ($doc.path | path exists) {
            $found = ($found | append $"($doc.path): does not exist.")
            continue
        }
        let rows = (src-rows (open --raw $doc.path) $doc.heading $doc.path)
        if ($rows.error | is-not-empty) {
            $found = ($found | append $rows.error)
            continue
        }
        $found = ($found | append (violations $files $rows $doc.path) | append (dir-violations $rows.dirs $doc.path))
    }

    if ($found | is-not-empty) {
        print --stderr "[check-source-tree] FAILED:"
        for problem in $found {
            print --stderr $"  - ($problem)"
        }
        print --stderr ""
        print --stderr "The src/ block of each source tree must list every file in src/**/*.rs, one row each, and no row may name a file that does not exist."
        exit 1
    }

    let docs = ($DOCS | get path | str join ", ")
    print $"[check-source-tree] OK: all ($files | length) files in src/ have a row in ($docs), and every row names a file."
}
