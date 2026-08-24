#!/usr/bin/env nu

# Run each feature leg's cargo test step and prove the run tested something.
#
# Why this exists: cargo exits 0 when a leg collects no tests, when a name
# filter matches nothing, and when every case is #[ignore]d. The test steps in
# `just pre-commit` and .forgejo/workflows/check.yml could therefore report
# green having asserted nothing (RUS-23). RUS-20 closed the same hole on the
# JavaScript side in static/tests/run.mjs.
#
# Nothing here is hand-listed. The legs come from the [[bin]] required-features
# in Cargo.toml, so a new build mode is covered the moment its binary lands,
# and the whole justfile plus the workflow are re-read on every run, so an
# invocation added beside this guard is reported instead of quietly escaping it.
#
# Usage:
#   nu scripts/check-cargo-tests-ran.nu --self-test
#   nu scripts/check-cargo-tests-ran.nu --lib --runner "docker run --rm ... <image>"
#   nu scripts/check-cargo-tests-ran.nu --leg saas --runner "docker compose run ... app"
#   nu scripts/check-cargo-tests-ran.nu
#
# Exit codes:
# - 0: every leg ran, nothing was ignored or filtered out, every floor was met.
# - 1: a leg failed, ran nothing, was filtered, or passed fewer than its floor.

# Every recipe in the justfile and every step in this workflow must reach the
# test harness through this guard. The named recipe must also still exist.
const GUARDED_RECIPE = "pre-commit"
const GUARDED_WORKFLOW = ".forgejo/workflows/check.yml"

# Floors, not targets: measured on main for RUS-23 and rounded down by roughly
# seven percent, so retiring a stale case does not fail the build while an
# empty, filtered or all-ignored run cannot clear them. Raise as the legs grow.
# --lib scope, what `just pre-commit` runs: 290 standalone, 222 saas.
const MIN_LIB_PASSED = {standalone: 270, saas: 205}
# All targets, what CI and `just test` / `just test-saas` run: 307 standalone
# (lib 290 + tests/ 17), 222 saas (lib 222). RUS-24 dropped the duplicate
# bin-target copy of the unit suite, so these fell from 597 and 444 without a
# single test being retired.
const MIN_FULL_PASSED = {standalone: 285, saas: 205}

# Fold a run's harness output into counts. A run that printed no summary line
# reports zero summaries, which is itself a violation.
def summarize [output: string]: nothing -> record {
    let rows = (
        $output
        | parse --regex 'test result: (?<status>\w+)\. (?<passed>\d+) passed; (?<failed>\d+) failed; (?<ignored>\d+) ignored; (?<measured>\d+) measured; (?<filtered>\d+) filtered out'
    )
    if ($rows | is-empty) {
        return {summaries: 0, passed: 0, failed: 0, ignored: 0, filtered: 0}
    }
    {
        summaries: ($rows | length)
        passed: ($rows | get passed | into int | math sum)
        failed: ($rows | get failed | into int | math sum)
        ignored: ($rows | get ignored | into int | math sum)
        filtered: ($rows | get filtered | into int | math sum)
    }
}

# Legs are whatever the shipped binaries require, so a third build mode is
# guarded as soon as its [[bin]] lands rather than when someone remembers.
def discover-legs []: nothing -> table {
    let manifest = (open Cargo.toml)
    let defaults = ($manifest.features?.default? | default [])
    $manifest.bin?
    | default []
    | each {|entry| $entry | get required-features? | default [] }
    | flatten
    | uniq
    | sort
    | each {|feature|
        let flags = if $feature in $defaults {
            ["--features" $feature]
        } else {
            ["--no-default-features" "--features" $feature]
        }
        {leg: $feature, flags: $flags}
    }
}

# --leg picks one discovered leg. An unknown name is an error, never an empty
# selection: running no leg at all is the vacuous run this guard exists to catch.
def select-legs [legs: table, name: string]: nothing -> record {
    if ($name | is-empty) {
        return {legs: $legs, error: ""}
    }
    let chosen = ($legs | where leg == $name)
    if ($chosen | is-empty) {
        let known = ($legs | get leg | str join ", ")
        return {legs: [], error: $"--leg ($name) matches no feature leg; known legs are ($known)"}
    }
    {legs: $chosen, error: ""}
}

def run-leg [leg: record, prefix: list<string>, lib_only: bool]: nothing -> record {
    let scope = if $lib_only { ["--lib"] } else { [] }
    let argv = ($prefix | append ["cargo" "test"] | append $scope | append $leg.flags)

    print $"[check-cargo-tests] ($argv | str join ' ')"
    let exe = ($argv | first)
    let args = ($argv | skip 1)
    let result = (do { run-external $exe ...$args } | complete)
    print $result.stdout
    if ($result.stderr | str trim | is-not-empty) {
        print --stderr $result.stderr
    }

    {leg: $leg.leg, exit: $result.exit_code} | merge (summarize $result.stdout)
}

# Every way a run can look green without having tested anything.
def violations [rows: table, floors: record]: nothing -> list<string> {
    if ($rows | is-empty) {
        return ["no feature leg ran at all, so nothing was tested"]
    }
    let configured = ($floors | columns)
    mut found = []
    for row in $rows {
        if $row.exit != 0 {
            $found = ($found | append $"($row.leg): the test step exited ($row.exit)")
        }
        if $row.summaries == 0 {
            $found = ($found | append $"($row.leg): printed no test result line, so the harness never ran")
        }
        if $row.ignored > 0 {
            $found = ($found | append $"($row.leg): ($row.ignored) ignored, and an ignored test looks green while proving nothing")
        }
        if $row.filtered > 0 {
            $found = ($found | append $"($row.leg): ($row.filtered) filtered out by a name filter")
        }
        if $row.passed < 1 {
            $found = ($found | append $"($row.leg): passed nothing")
        }
        if not ($row.leg in $configured) {
            $found = ($found | append $"($row.leg): no floor is configured for this leg, so add one to scripts/check-cargo-tests-ran.nu rather than leaving it unbounded")
        } else if $row.passed < ($floors | get $row.leg) {
            $found = ($found | append $"($row.leg): ($row.passed) passed and the floor is ($floors | get $row.leg)")
        }
    }
    $found
}

# Body of a justfile recipe: every line indented under its header.
def recipe-body [path: string, name: string]: nothing -> list<string> {
    let lines = (open --raw $path | lines)
    let start = ($lines | enumerate | where {|row| $row.item | str starts-with $"($name):"} | get 0?.index)
    if ($start | is-empty) {
        return []
    }
    let rest = ($lines | skip ($start + 1))
    let stop = (
        $rest
        | enumerate
        | where {|row| ($row.item | str trim | is-not-empty) and ($row.item !~ '^\s') }
        | get 0?.index
    )
    if ($stop | is-empty) { $rest } else { $rest | first $stop }
}

# A test invocation that does not come through here is an unguarded leg.
def bypasses [lines: list<string>]: nothing -> list<string> {
    $lines
    | where {|line| not ($line | str trim | str starts-with "#") }
    | where {|line| $line =~ 'cargo\s+test' }
    | each {|line| $line | str trim }
}

# Re-read both call sites each run so the guarded set is whatever they invoke,
# not a list in here that drifts the first time someone adds a step. The whole
# justfile is scanned, so a raw invocation in any recipe is reported (RUS-26).
def call-site-violations []: nothing -> list<string> {
    mut found = []
    if ("justfile" | path exists) {
        if (recipe-body "justfile" $GUARDED_RECIPE | is-empty) {
            $found = ($found | append $"justfile: no ($GUARDED_RECIPE) recipe body found, so the gate it fronts has gone missing")
        }
        for line in (bypasses (open --raw "justfile" | lines)) {
            $found = ($found | append $"justfile: `($line)` bypasses this guard")
        }
    } else {
        $found = ($found | append "justfile: not found")
    }
    if ($GUARDED_WORKFLOW | path exists) {
        for line in (bypasses (open --raw $GUARDED_WORKFLOW | lines)) {
            $found = ($found | append $"($GUARDED_WORKFLOW): `($line)` bypasses this guard")
        }
    } else {
        $found = ($found | append $"($GUARDED_WORKFLOW): not found")
    }
    $found
}

# Prove the guard still rejects the runs that look green. Without this a broken
# parser would pass every job silently, the same blindness one level up.
def run-self-test [floors: record] {
    let leg = ($floors | columns | first)
    let floor = ($floors | get $leg)
    let healthy = {leg: $leg, exit: 0, summaries: 1, passed: $floor, failed: 0, ignored: 0, filtered: 0}
    let rejects = [
        {name: "a vacuous run", rows: [($healthy | merge {passed: 0})]}
        {name: "an all-ignored run", rows: [($healthy | merge {passed: 0, ignored: 12})]}
        {name: "a filtered run", rows: [($healthy | merge {passed: 1, filtered: 289})]}
        {name: "a run that printed no summary", rows: [($healthy | merge {summaries: 0, passed: 0})]}
        {name: "a run under its floor", rows: [($healthy | merge {passed: ($floor - 1)})]}
        {name: "a leg with no configured floor", rows: [($healthy | merge {leg: "unlisted"})]}
        {name: "a run that failed", rows: [($healthy | merge {exit: 101, failed: 1})]}
        {name: "a run with no legs at all", rows: []}
    ]

    for case in $rejects {
        if (violations $case.rows $floors | is-empty) {
            print --stderr $"[check-cargo-tests] SELF-TEST FAILED: ($case.name) was accepted."
            exit 1
        }
    }
    let rejected = (violations [$healthy] $floors)
    if ($rejected | is-not-empty) {
        print --stderr $"[check-cargo-tests] SELF-TEST FAILED: a healthy run was rejected: ($rejected | str join '; ')"
        exit 1
    }

    # The parser is the other half of the guard: a regex that stopped matching
    # the harness would report zero summaries and reject everything, but one
    # that matched the wrong groups would wave a filtered run through.
    let parsed = (summarize "test result: ok. 3 passed; 0 failed; 12 ignored; 0 measured; 289 filtered out; finished in 0.47s")
    if ($parsed.summaries != 1) or ($parsed.passed != 3) or ($parsed.ignored != 12) or ($parsed.filtered != 289) {
        print --stderr $"[check-cargo-tests] SELF-TEST FAILED: the summary parser read ($parsed)."
        exit 1
    }
    if (summarize "Compiling rus v0.15.1").summaries != 0 {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: output carrying no summary line parsed as a real run."
        exit 1
    }

    # --leg is the one way a caller can narrow the run, so it must never narrow
    # it to nothing: an empty selection has no row left to violate anything.
    let sample = [{leg: "standalone", flags: []}, {leg: "saas", flags: []}]
    if ((select-legs $sample "nope").error | is-empty) {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: --leg with an unknown name selected zero legs silently."
        exit 1
    }
    if ((select-legs $sample "nope").legs | is-not-empty) {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: --leg with an unknown name still returned legs to run."
        exit 1
    }
    if ((select-legs $sample "saas").legs | get leg) != ["saas"] {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: --leg saas did not select exactly the saas leg."
        exit 1
    }
    if ((select-legs $sample "").legs | length) != ($sample | length) {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: an absent --leg did not run every leg."
        exit 1
    }
    # A narrowed run keeps the selected leg's own floor rather than inheriting
    # the first one, so `just test-saas` cannot pass on a standalone-sized floor.
    let narrowed = {leg: ($floors | columns | last), exit: 0, summaries: 1, passed: 1, failed: 0, ignored: 0, filtered: 0}
    if (violations [$narrowed] $floors | is-empty) {
        print --stderr "[check-cargo-tests] SELF-TEST FAILED: a single-leg run under its own floor was accepted."
        exit 1
    }

    print "[check-cargo-tests] SELF-TEST OK: vacuous, ignored, filtered, silent, under-floor and unfloored runs are all rejected, and --leg cannot select nothing."
}

export def main [
    --runner: string = "" # command that fronts cargo, e.g. a docker run wrapper
    --lib # restrict each leg to the library target, as `just pre-commit` does
    --leg: string = "" # run only this feature leg, as `just test` does (default: all)
    --self-test # prove the guard still rejects a vacuous run, then exit
] {
    let floors = if $lib { $MIN_LIB_PASSED } else { $MIN_FULL_PASSED }
    if $self_test {
        run-self-test $floors
        return
    }

    if not ("Cargo.toml" | path exists) {
        print --stderr "[check-cargo-tests] FAILED: run this from the repository root."
        exit 1
    }

    let drift = (call-site-violations)
    if ($drift | is-not-empty) {
        print --stderr "[check-cargo-tests] FAILED: a test step runs outside this guard:"
        for problem in $drift {
            print --stderr $"  - ($problem)"
        }
        exit 1
    }

    let discovered = (discover-legs)
    if ($discovered | is-empty) {
        print --stderr "[check-cargo-tests] FAILED: no [[bin]] required-features in Cargo.toml, so no feature leg could be derived."
        exit 1
    }

    let selected = (select-legs $discovered $leg)
    if ($selected.error | is-not-empty) {
        print --stderr $"[check-cargo-tests] FAILED: ($selected.error)."
        exit 1
    }
    let legs = $selected.legs

    let prefix = ($runner | split row --regex '\s+' | where {|part| $part != ""})
    let rows = ($legs | each {|leg| run-leg $leg $prefix $lib})
    let configured = ($floors | columns)
    print ""
    print (
        $rows
        | each {|row| $row | merge {floor: (if ($row.leg in $configured) { $floors | get $row.leg } else { "unset" })} }
        | select leg passed floor ignored filtered exit
    )

    let found = (violations $rows $floors)
    if ($found | is-not-empty) {
        print --stderr "[check-cargo-tests] FAILED:"
        for problem in $found {
            print --stderr $"  - ($problem)"
        }
        exit 1
    }

    let scope = if $lib { "--lib" } else { "all targets" }
    print $"[check-cargo-tests] OK: ($rows | get passed | math sum) tests passed at scope ($scope) across legs ($rows | get leg | str join ', '), each at or above its floor."
}
