#!/bin/sh
# Sets the version of the workspace, in the ten lines that say it.
#
# The version is declared once, under `[workspace.package]`, and every member
# says `version.workspace = true`. The nine path dependencies of
# `[workspace.dependencies]` repeat it, because a path dependency that names a
# version has to name the one the crate carries — so there are ten lines and
# nine of them are a copy. This is what keeps the ten saying one thing.
#
# `cargo set-version` of `cargo-edit` does the same job. It is not used here: it
# is a build of its own on whatever machine makes a release, and it does not
# know the places of this repository that name the version in prose.
#
# Cargo.lock is refreshed for the members alone, so nothing of the registry
# moves with a version bump.
#
# What it does not touch: a file that names the version in a sentence. Those are
# listed at the end and left to be read, because what belongs in the sentence is
# not the number alone. CHANGESET.md is not among them — it is written about a
# tag and goes on naming that tag.
#
# Nothing is committed and nothing is tagged.

set -eu

name=${0##*/}
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
manifest=$root/Cargo.toml

if [ $# -ne 1 ]; then
	echo "usage: $name <version>" >&2
	echo "       $name 1.2.0" >&2
	exit 2
fi
wanted=$1

if ! printf '%s' "$wanted" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
	echo "$name: not a version: $wanted" >&2
	exit 2
fi

if ! grep -q '^\[workspace\]' "$manifest" 2>/dev/null; then
	echo "$name: $manifest is not the manifest of the workspace" >&2
	exit 1
fi

read_version() {
	awk '
		/^\[/ { section = $0 }
		section == "[workspace.package]" && /^version[ \t]*=/ {
			if (match($0, /"[^"]+"/)) {
				print substr($0, RSTART + 1, RLENGTH - 2)
				exit
			}
		}
	' "$manifest"
}

current=$(read_version)

if [ -z "$current" ]; then
	echo "$name: no version under [workspace.package]" >&2
	exit 1
fi

if [ "$current" = "$wanted" ]; then
	echo "$name: the workspace is already $wanted"
	exit 0
fi

temporary=$manifest.version.$$
trap 'rm -f "$temporary"' EXIT INT TERM

awk -v wanted="$wanted" '
	/^\[/ { section = $0 }
	section == "[workspace.package]" && /^version[ \t]*=/ {
		sub(/"[^"]*"/, "\"" wanted "\"")
		print
		next
	}
	/path[ \t]*=[ \t]*"crates\// {
		sub(/version[ \t]*=[ \t]*"[^"]*"/, "version = \"" wanted "\"")
		print
		next
	}
	{ print }
' "$manifest" > "$temporary"

mv "$temporary" "$manifest"

if [ "$(read_version)" != "$wanted" ]; then
	echo "$name: the version under [workspace.package] did not take" >&2
	exit 1
fi

if grep -E 'path[ \t]*=[ \t]*"crates/' "$manifest" | grep -qF "\"$current\""; then
	echo "$name: a path dependency still names $current" >&2
	exit 1
fi

echo "Cargo.toml: $current -> $wanted, the workspace and its path dependencies"

( cd "$root" && cargo update --workspace --quiet )
echo "Cargo.lock: refreshed for the members of the workspace"

if command -v git > /dev/null 2>&1 && [ -d "$root/.git" ]; then
	left=$(
		cd "$root" &&
		git grep -nF "$current" -- . \
			':!Cargo.toml' ':!Cargo.lock' ':!CHANGESET.md' ':!tools' || true
	)
	if [ -n "$left" ]; then
		echo
		echo "still naming $current, and left alone:"
		printf '%s\n' "$left" | sed 's/^/  /'
	fi
fi

echo
echo "nothing was committed. the checks, then the commit, then the tag:"
echo "  cargo fmt --all && cargo clippy --workspace --all-targets"
echo "  cargo test --workspace && cargo audit"
echo "  git commit -a -m 'Release $wanted' && git tag v$wanted"
