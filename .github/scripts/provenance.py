from hashlib import sha256
from pathlib import Path
import json
import os

root = Path("release-assets")
artifacts = []

for path in sorted(root.rglob("*")):
    if not path.is_file():
        continue
    data = path.read_bytes()
    artifacts.append({
        "path": str(path.relative_to(root)).replace("\\", "/"),
        "sha256": sha256(data).hexdigest(),
        "size": path.stat().st_size,
    })

manifest = {
    "schema": "pst2pdf.release-provenance/v1",
    "repository": os.environ["GITHUB_REPOSITORY"],
    "workflow": os.environ["GITHUB_WORKFLOW"],
    "run_id": os.environ["GITHUB_RUN_ID"],
    "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
    "ref": os.environ["GITHUB_REF"],
    "sha": os.environ["GITHUB_SHA"],
    "version": os.environ["VERSION"],
    "release_tag": os.environ["RELEASE_TAG"],
    "prerelease": os.environ["PRERELEASE"],
    "artifacts": artifacts,
}

out = Path("dist")
out.mkdir(parents=True, exist_ok=True)
target = out / f"provenance-{os.environ['VERSION']}.json"
target.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
