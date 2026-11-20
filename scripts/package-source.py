"""Archive the actual build sources without changing Git refs or including local settings."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

root = Path(__file__).resolve().parent.parent
output = Path(sys.argv[1]).resolve()
paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root).decode('utf-8').split('\0')
files = sorted({path for path in paths if path and not any(part in {'.git', '.opencowork', '.tmp', 'target', 'dist'} for part in Path(path).parts) and not Path(path).name.startswith('.env')})
digest = hashlib.sha256()
output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
    for name in files:
        source = root / name
        if not source.is_file():
            continue
        content = source.read_bytes()
        digest.update(name.encode('utf-8') + b'\0' + hashlib.sha256(content).digest())
        archive.writestr('OpenCowork-source/' + name.replace('\\', '/'), content)
    archive.writestr('OpenCowork-source/BUILD-SNAPSHOT.json', json.dumps({
        'baseCommit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
        'localChanges': True,
        'sourceTreeSha256': digest.hexdigest(),
        'description': 'Actual source snapshot for the attached Windows build. Git refs were not modified.',
    }, indent=2))
print(output)
