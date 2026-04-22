<#
.SYNOPSIS
    Bump a semver git tag and push it.

.DESCRIPTION
    Finds the highest vX.Y.Z tag in the current repo, increments it,
    creates a new annotated tag, and pushes it to origin.

.EXAMPLE
    .\increment_version.ps1 --major          # v1.2.3 → v2.0.0
    .\increment_version.ps1 --minor          # v1.2.3 → v1.3.0
    .\increment_version.ps1 --patch          # v1.2.3 → v1.2.4
    .\increment_version.ps1 --version 2.1.0  # explicit version
#>

function Show-Usage {
    Write-Host "Usage: increment_version.ps1 --major | --minor | --patch | --version <version>"
    Write-Host ""
    Write-Host "  --major             Increment major, reset minor and patch to 0"
    Write-Host "  --minor             Increment minor, reset patch to 0"
    Write-Host "  --patch             Increment patch"
    Write-Host "  --version <version> Set an explicit version (format: X.Y.Z or vX.Y.Z)"
    Write-Host ""
    Write-Host "  Only one argument is allowed."
}

# Manual arg parsing so --flag (double-dash) works correctly
$mode = ""
$explicitVersion = ""
$i = 0
while ($i -lt $args.Count) {
    $arg = $args[$i]
    switch ($arg) {
        { $_ -in "--major", "-major" } {
            if ($mode) { Write-Error "Only one flag is allowed at a time."; exit 1 }
            $mode = "major"
        }
        { $_ -in "--minor", "-minor" } {
            if ($mode) { Write-Error "Only one flag is allowed at a time."; exit 1 }
            $mode = "minor"
        }
        { $_ -in "--patch", "-patch" } {
            if ($mode) { Write-Error "Only one flag is allowed at a time."; exit 1 }
            $mode = "patch"
        }
        { $_ -in "--version", "-version" } {
            if ($mode) { Write-Error "Only one flag is allowed at a time."; exit 1 }
            $mode = "explicit"
            $i++
            if ($i -ge $args.Count) { Write-Error "--version requires a value"; exit 1 }
            $explicitVersion = $args[$i]
        }
        default {
            Write-Error "Unknown argument: $arg"
            Show-Usage
            exit 1
        }
    }
    $i++
}

if (-not $mode) {
    Show-Usage
    exit 1
}

# Find the highest semver tag
$latest = git tag --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname 2>$null | Select-Object -First 1

if (-not $latest) {
    $latest = "v0.0.0"
    Write-Host "No existing version tags found — starting from $latest"
} else {
    Write-Host "Current version: $latest"
}

$v = $latest.TrimStart('v')
$parts = $v -split '\.'
[int]$maj = $parts[0]
[int]$min = $parts[1]
[int]$pat = $parts[2]

switch ($mode) {
    "major"    { $maj++; $min = 0; $pat = 0 }
    "minor"    { $min++; $pat = 0 }
    "patch"    { $pat++ }
    "explicit" {
        $ver = $explicitVersion.TrimStart('v')
        if ($ver -notmatch '^\d+\.\d+\.\d+$') {
            Write-Error "Version must be in format X.Y.Z or vX.Y.Z"
            exit 1
        }
        $p = $ver -split '\.'
        $maj = [int]$p[0]; $min = [int]$p[1]; $pat = [int]$p[2]
    }
}

$newTag = "v${maj}.${min}.${pat}"

if (git tag --list | Where-Object { $_ -eq $newTag }) {
    Write-Error "Tag $newTag already exists."
    exit 1
}

Write-Host "New version:     $newTag"

git tag -a $newTag -m "Release $newTag"
git push origin $newTag

Write-Host "✓ Tagged and pushed $newTag"
