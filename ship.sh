#!/usr/bin/env bash
# Release G'MIC Video: bump version, test, build the AppImage, tag, push and publish a GitHub release.
#
# Usage: ./ship.sh [patch|minor|major|X.Y.Z] [--notes "text"] [--draft] [--skip-tests] [--dry-run]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

BUMP="patch"
NOTES=""
DRAFT=0
SKIP_TESTS=0
DRY_RUN=0

usage() {
    sed -n '2,4p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        patch|minor|major) BUMP="$1" ;;
        [0-9]*.[0-9]*.[0-9]*) BUMP="$1" ;;
        --notes) shift; NOTES="${1:?--notes needs a value}" ;;
        --notes=*) NOTES="${1#--notes=}" ;;
        --draft) DRAFT=1 ;;
        --skip-tests) SKIP_TESTS=1 ;;
        --dry-run|-n) DRY_RUN=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

step() { echo; echo "==> $*"; }
die()  { echo "ERROR: $*" >&2; exit 1; }
# Run a command, or only print it in --dry-run mode.
run() {
    if [[ $DRY_RUN -eq 1 ]]; then
        echo "   [dry-run] $*"
    else
        "$@"
    fi
}

# ---------------------------------------------------------------- 1. Preconditions
step "Checking preconditions"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not a git repository"
git remote get-url origin >/dev/null 2>&1 || die "no 'origin' remote"
command -v gh >/dev/null || die "GitHub CLI 'gh' is not installed"
gh auth status >/dev/null 2>&1 || die "'gh' is not logged in (run: gh auth login)"
git lfs version >/dev/null 2>&1 || die "git-lfs is not installed"
command -v cargo >/dev/null || die "cargo is not installed"
if [[ -n "$(git status --porcelain)" ]]; then
    git status --short >&2
    die "working tree is not clean; commit or stash your changes first"
fi
BRANCH="$(git symbolic-ref --short HEAD 2>/dev/null)" || die "detached HEAD; check out a branch first"
echo "   branch: $BRANCH, remote: $(git remote get-url origin)"

# ---------------------------------------------------------------- 2. Version
CURRENT="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version *=/{gsub(/.*= *"|".*/,""); print; exit}' Cargo.toml)"
[[ "$CURRENT" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]] || die "cannot read version from Cargo.toml (got '$CURRENT')"
MAJ="${BASH_REMATCH[1]}"; MIN="${BASH_REMATCH[2]}"; PAT="${BASH_REMATCH[3]}"
case "$BUMP" in
    patch) NEW="$MAJ.$MIN.$((PAT + 1))" ;;
    minor) NEW="$MAJ.$((MIN + 1)).0" ;;
    major) NEW="$((MAJ + 1)).0.0" ;;
    *)     NEW="$BUMP" ;;
esac
[[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "invalid version '$NEW'"
TAG="v$NEW"
git fetch --tags --quiet origin 2>/dev/null || true
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && die "tag $TAG already exists"
step "Version: $CURRENT -> $NEW ($TAG)"

# Undo the version bump if anything fails before the release commit.
BUMPED=0
restore() {
    if [[ $BUMPED -eq 1 ]]; then
        echo "Restoring Cargo.toml / Cargo.lock" >&2
        git checkout -- Cargo.toml Cargo.lock
    fi
}
trap 'rc=$?; if [[ $rc -ne 0 ]]; then restore; echo "ship.sh failed (exit $rc)" >&2; fi' EXIT

# ---------------------------------------------------------------- 3. Bump
step "Bumping Cargo.toml and rebuilding"
if [[ $DRY_RUN -eq 1 ]]; then
    echo "   [dry-run] set [package] version = \"$NEW\" in Cargo.toml"
else
    BUMPED=1
    awk -v new="$NEW" '
        /^\[package\]/ {p=1; print; next}
        /^\[/ {p=0}
        p && !done && /^version *=/ {print "version = \"" new "\""; done=1; next}
        {print}
    ' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
fi
run cargo build --release
if [[ $DRY_RUN -eq 0 ]]; then
    GOT="$(./target/release/GmicVideo --version | awk '{print $NF}')"
    [[ "$GOT" == "$NEW" ]] || die "binary reports version '$GOT', expected '$NEW'"
    echo "   binary reports version $GOT"
fi

# ---------------------------------------------------------------- 4. Tests
if [[ $SKIP_TESTS -eq 1 ]]; then
    step "Skipping tests (--skip-tests)"
else
    step "Running tests"
    # One test at a time: the real-time playback tests miss their frame targets under parallel load.
    run cargo test --release -- --test-threads=1
    run python3 Tests/test_visual_search_thumbnails.py
    run python3 Tests/test_halftone_effect.py
    run env QT_QPA_PLATFORM=offscreen python3 Tests/test_playback_gui.py
    for loc in pt_BR de_DE ja_JP fr_FR zh_CN; do
        run env LANG="$loc.UTF-8" QT_QPA_PLATFORM=offscreen python3 src_gui/gmicvideo_gui.py --test
    done
fi

# ---------------------------------------------------------------- 5. Build AppImage
step "Building AppImage"
DIST="$ROOT_DIR/dist"
ASSET="$DIST/GmicVideo-$NEW-x86_64.AppImage"
SUMS="$ASSET.sha256"
run ./packaging/build_appimage.sh
run mkdir -p "$DIST"
run cp -f "$ROOT_DIR/GmicVideo-x86_64.AppImage" "$ASSET"
if [[ $DRY_RUN -eq 1 ]]; then
    echo "   [dry-run] sha256sum $(basename "$ASSET") > $(basename "$SUMS")"
else
    (cd "$DIST" && sha256sum "$(basename "$ASSET")" > "$(basename "$SUMS")")
    echo "   $(cat "$SUMS")"
fi

# ---------------------------------------------------------------- 6. Release notes
step "Writing release notes"
LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"
NOTES_FILE="$(mktemp)"
{
    if [[ -n "$NOTES" ]]; then
        echo "$NOTES"
    else
        if [[ -n "$LAST_TAG" ]]; then
            echo "## Changes since $LAST_TAG"
            RANGE="$LAST_TAG..HEAD"
        else
            echo "## Changes"
            RANGE="HEAD"
        fi
        echo
        git log --no-merges --pretty='- %s (%h)' $RANGE
    fi
    cat <<EOF

## Install
\`\`\`bash
chmod +x GmicVideo-$NEW-x86_64.AppImage
./GmicVideo-$NEW-x86_64.AppImage
\`\`\`
Verify the download with \`sha256sum -c GmicVideo-$NEW-x86_64.AppImage.sha256\`.

**Requires on the host:** \`python3\` with PySide6, KDE Kirigami, \`ffmpeg\`/\`ffprobe\`.
A Vulkan driver is optional (GPU effects; falls back to the CPU otherwise).
EOF
} > "$NOTES_FILE"
sed 's/^/   | /' "$NOTES_FILE"

# ---------------------------------------------------------------- 7. Commit, tag, push
step "Committing, tagging and pushing"
run git add Cargo.toml Cargo.lock
run git commit -m "Release $TAG"
BUMPED=0
run git tag -a "$TAG" -m "G'MIC Video $TAG"
run git push origin "HEAD:$BRANCH" --follow-tags

# ---------------------------------------------------------------- 8. GitHub release
step "Creating GitHub release"
GH_ARGS=(release create "$TAG" "$ASSET" "$SUMS" --title "G'MIC Video $TAG" --notes-file "$NOTES_FILE")
[[ $DRAFT -eq 1 ]] && GH_ARGS+=(--draft)
run gh "${GH_ARGS[@]}"
rm -f "$NOTES_FILE"

if [[ $DRY_RUN -eq 1 ]]; then
    step "Dry run finished; nothing was changed."
else
    step "Released: $(gh release view "$TAG" --json url -q .url)"
fi
