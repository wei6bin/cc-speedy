---
name: gep-code-review
description: Perform a thorough, language-agnostic code review covering correctness, design, naming, style, code smells, and basic security
license: MIT
compatibility: opencode
metadata:
  audience: developers
  workflow: review
---

## What I do

I conduct a structured code review across six dimensions and produce an
actionable, prioritised report. I am language-agnostic: the same principles
apply whether the code is Rust, Python, TypeScript, Go, or any other language.

---

## Review dimensions

### 1. Bugs & Correctness

- Off-by-one errors, wrong loop bounds, incorrect comparisons.
- Incorrect handling of edge cases: empty input, zero, null/None/nil, negative numbers.
- Early-return or `?`/exception propagation that silently discards errors instead of continuing (e.g. `line.ok()?` inside a loop that should use `continue`).
- Logic inversions — conditions that are accidentally negated.
- Concurrency issues: data races, lock ordering, missing synchronisation, TOCTOU.
- Resource leaks: file handles, DB connections, sockets not closed on all paths.
- Integer overflow / underflow.
- Floating-point equality comparisons.

### 2. Design & Architecture

- Single Responsibility: does each function/module/class do exactly one thing?
- DRY (Don't Repeat Yourself): identify duplicated logic that should be extracted (e.g. near-identical functions that differ only in one argument).
- Abstraction level: mixing high-level orchestration with low-level detail in one function.
- Tight coupling: hard-coded dependencies that should be injected or parameterised.
- Unnecessary statefulness: mutable shared state where pure functions would suffice.
- Missing or wrong abstractions: repeated "last N path segments" logic spread across files instead of a single utility.
- Dead code: functions, branches, or variables that are defined but never reachable.
- Premature optimisation that makes the code harder to understand without a proven need.
- Functions that are too long (rule of thumb: > 40 lines deserves scrutiny).

### 3. Naming Conventions

- Names should reveal intent: `get_data` says nothing; `fetch_pending_orders` does.
- Avoid abbreviations unless they are universally understood (`i`, `url`, `id` are fine; `rnhst` is not).
- Boolean variables and functions should read as predicates: `is_empty`, `has_permission`, `can_retry`.
- Consistency: if the codebase uses `snake_case`, do not introduce `camelCase` or `PascalCase` for the same kind of identifier.
- Avoid misleading names: a function called `path_last_two` that actually returns a configurable number of segments is misleading.
- Constants should be `UPPER_SNAKE_CASE` (most languages) or clearly distinguished from mutable variables.
- Avoid `temp`, `tmp2`, `data2`, `obj`, `stuff` as permanent names.

### 4. Style & Consistency

- Consistent indentation and whitespace (flag only genuine inconsistencies, not style preferences if a formatter is in use).
- Imports grouped logically: standard library → third-party → local. Stray imports in the middle of a file should be moved to the top.
- Magic numbers and magic strings: replace with named constants.
- Overly complex boolean expressions: simplify with intermediate named variables.
- Deep nesting (> 3 levels): consider early returns or extraction.
- Long lines that wrap awkwardly (context-dependent; flag only when they harm readability).
- Trailing whitespace, inconsistent line endings.
- Commented-out code left in production files.
- TODO/FIXME comments without a ticket reference or owner.
- Version strings that have drifted from the actual state (e.g. `version = "0.1.0"` when significant features have landed).

### 5. Code Smells

- **Feature envy**: a function that accesses another module's internals more than its own.
- **Data clumps**: the same group of variables always passed together — make them a struct/record.
- **Primitive obsession**: using raw strings/ints where a newtype or enum would be safer.
- **Long parameter lists**: > 4 parameters is a hint to introduce a config struct.
- **Shotgun surgery**: a single conceptual change requires edits in many unrelated files.
- **Speculative generalisation**: abstractions added "just in case" that are not yet needed.
- **Inappropriate intimacy**: two modules that know too much about each other's internals.
- **Switch/match exhaustiveness**: pattern-matching on enums without handling all variants explicitly.
- **Silent failure**: returning a default/empty value on error instead of propagating it, making bugs invisible.
- **Hardcoded environment assumptions**: timezone offsets, absolute paths, locale-specific formatting baked in.

### 6. Security

- **Injection**: any place where external input is interpolated into a shell command, SQL query, file path, or template without sanitisation. Flag `format!("... {user_input} ...")` fed directly to a subprocess.
- **Path traversal**: user-supplied path components that are not sanitised (strip `..`, validate against an allowed root).
- **Credential exposure**: secrets, API keys, or tokens in source files, log statements, or error messages.
- **Error message leakage**: stack traces, internal paths, or DB schema details returned to untrusted callers.
- **Privilege escalation surface**: code running with elevated privileges that does more than strictly necessary.
- **Pipe/channel deadlocks**: writing to stdin of a child process without draining stdout concurrently can deadlock when the pipe buffer fills — use a thread or async I/O to drain.
- **Mutex / lock misuse**: `expect` / `unwrap` on a poisoned mutex halts the process; handle or recover from poisoning.
- **Time-of-check to time-of-use (TOCTOU)**: checking existence/permissions of a file and then using it without an atomic operation.
- **Hardcoded defaults that weaken security**: empty passwords, `0.0.0.0` binds, debug modes left on in production paths.
- **Dependency supply chain**: flag obviously outdated or yanked dependency versions; note if there is no lock file.
- **Clipboard / inter-process data**: sensitive data written to clipboard or shared memory should be cleared after use.

---

## Scope

### Default — code change (diff)

Unless the user says otherwise, review only the **current code change**:

1. Run `git diff HEAD` (or `git diff --staged` if changes are staged) to obtain the diff.
2. For each changed file, also read enough surrounding context (the full function or block containing the change) to assess correctness and design — do not limit yourself to the raw diff lines.
3. Restrict findings to code that was **added or modified** in the diff. Do not report pre-existing issues in unchanged lines unless they are directly relevant to the change (e.g. a bug the change depends on).

### Extended scopes (user-requested)

| User says | What to review |
|---|---|
| "review this file" / `@file` | The single named file in full |
| "review this module" / directory | All source files under that directory |
| "review the whole codebase" | All source files reachable from the project root; group findings by file |
| "review the PR" | Equivalent to default diff scope against the base branch |

When reviewing more than a diff, **group findings by file** and add a per-file mini-summary before the global summary table.

---

## Process

1. **Determine scope** (see above) before reading any code.
2. **Read the code** in full within that scope before commenting. Understand intent before judging implementation.
3. **Prioritise findings** using three tiers:
   - `[BUG]` — incorrect behaviour, data loss, crash, or security vulnerability. Must fix.
   - `[DESIGN]` — structural issue that will cause maintenance pain. Should fix.
   - `[STYLE]` — consistency, naming, minor clarity. Nice to fix.
4. **For each finding**, state:
   - The file and line reference (e.g. `src/sessions.rs:47`).
   - The category tag.
   - A one-sentence description of the problem.
   - A concrete fix or example of the corrected code.
5. **End with a summary** table: total counts per tier, and an overall verdict (Ready / Needs work / Major issues).

---

## When to use me

Invoke this skill when:

- Committing or raising a PR — reviews the diff by default.
- Auditing a specific file or module before merging.
- Doing a pre-release quality pass over the whole codebase.
- Onboarding to an unfamiliar codebase.

If no explicit scope is given, default to the current git diff.

---

## Output format

```
## Code Review — <file or PR title>

### [BUG] <short title>
**Location:** `path/to/file.ext:line`
**Problem:** <one sentence>
**Fix:**
\`\`\`
corrected code snippet
\`\`\`

### [DESIGN] <short title>
...

### [STYLE] <short title>
...

---

## Summary

| Tier    | Count |
|---------|-------|
| BUG     | N     |
| DESIGN  | N     |
| STYLE   | N     |

**Verdict:** Ready / Needs work / Major issues
```
