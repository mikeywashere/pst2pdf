#!/usr/bin/env bash
# increment_version.sh — bump a semver git tag and push it
#
# Usage:
#   increment_version.sh --major              # v1.2.3 → v2.0.0
#   increment_version.sh --minor              # v1.2.3 → v1.3.0
#   increment_version.sh --patch              # v1.2.3 → v1.2.4
#   increment_version.sh --version <version>  # explicit, e.g. v2.1.0 or 2.1.0

set -euo pipefail

usage() {
    echo "Usage: $0 --major | --minor | --patch | --version <version>"
    echo ""
    echo "  Finds the highest semver tag in the current repo, increments it,"
    echo "  creates a new annotated tag, and pushes it."
    echo ""
    echo "  --major             Increment major, reset minor and patch to 0"
    echo "  --minor             Increment minor, reset patch to 0"
    echo "  --patch             Increment patch"
    echo "  --version <version> Set an explicit version (format: X.Y.Z or vX.Y.Z)"
    echo ""
    echo "  Only one argument is allowed."
    exit 1
}

[[ $# -eq 0 ]] && usage

MODE=""
EXPLICIT_VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --major)
            [[ -n "$MODE" ]] && { echo "Error: only one flag is allowed at a time"; usage; }
            MODE="major"; shift ;;
        --minor)
            [[ -n "$MODE" ]] && { echo "Error: only one flag is allowed at a time"; usage; }
            MODE="minor"; shift ;;
        --patch)
            [[ -n "$MODE" ]] && { echo "Error: only one flag is allowed at a time"; usage; }
            MODE="patch"; shift ;;
        --version)
            [[ -n "$MODE" ]] && { echo "Error: only one flag is allowed at a time"; usage; }
            MODE="explicit"; shift
            [[ $# -eq 0 ]] && { echo "Error: --version requires a value"; usage; }
            EXPLICIT_VERSION="$1"; shift ;;
        *)
            echo "Unknown argument: $1"; usage ;;
    esac
done

# Find the highest semver tag (e.g. v1.2.3)
LATEST=$(git tag --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname 2>/dev/null | head -n1 || true)

if [[ -z "$LATEST" ]]; then
    LATEST="v0.0.0"
    echo "No existing version tags found — starting from $LATEST"
else
    echo "Current version: $LATEST"
fi

# Strip leading 'v' and split into components
VERSION="${LATEST#v}"
IFS='.' read -r MAJOR MINOR PATCH <<< "$VERSION"

case "$MODE" in
    major)
        MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
    minor)
        MINOR=$((MINOR + 1)); PATCH=0 ;;
    patch)
        PATCH=$((PATCH + 1)) ;;
    explicit)
        EXPLICIT_VERSION="${EXPLICIT_VERSION#v}"
        if ! [[ "$EXPLICIT_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "Error: version must be in format X.Y.Z or vX.Y.Z"
            exit 1
        fi
        IFS='.' read -r MAJOR MINOR PATCH <<< "$EXPLICIT_VERSION" ;;
esac

NEW_TAG="v${MAJOR}.${MINOR}.${PATCH}"

if git tag --list | grep -qx "$NEW_TAG"; then
    echo "Error: tag $NEW_TAG already exists"
    exit 1
fi

echo "New version:     $NEW_TAG"

git tag -a "$NEW_TAG" -m "Release $NEW_TAG"
git push origin "$NEW_TAG"

echo "✓ Tagged and pushed $NEW_TAG"
