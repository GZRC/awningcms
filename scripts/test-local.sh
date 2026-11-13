#!/usr/bin/env bash
# Builds the image, starts a disposable stack on 127.0.0.1:18080 and runs the
# end-to-end smoke test against it. Never points at a real installation.
set -euo pipefail
cd "$(dirname "$0")/.."
docker build -t awningcms:test .
python3 - <<'PY'
from pathlib import Path
import secrets
private = Path('private')
private.mkdir(mode=0o700, exist_ok=True)
for name, value in [('test-db-password.secret', 'local-disposable-test-only'),
                    ('test-setup-token.secret', secrets.token_hex(32))]:
    path = private / name
    path.write_text(value + '\n')
    # Read by UID 10001 inside the container; test values only.
    path.chmod(0o644)
PY
compose=(docker compose -f infra/compose.test.yaml)
trap '"${compose[@]}" down -v >/dev/null 2>&1 || true' EXIT
"${compose[@]}" down -v >/dev/null 2>&1 || true
"${compose[@]}" up -d --wait
CMS_TEST_ALLOW_SETUP=1 CMS_TEST_DB_FIXTURES=1 \
CMS_TEST_BASE_URL=http://localhost:18080 CMS_TEST_ORIGIN=http://localhost:18080 \
CMS_TEST_CLIENT_BASE_URL=http://127.0.0.1:18080 \
CMS_TEST_SETUP_TOKEN="$(cat private/test-setup-token.secret)" \
    python3 tests/smoke.py
